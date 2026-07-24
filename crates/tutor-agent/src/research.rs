use std::collections::HashMap;
use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_agent::Session;
use llm_harness_runtime::control::cost::CostAggregate;
use llm_harness_runtime::workflow::engine::{
    StepProgress, WorkflowEngine, WorkflowEngineConfig, WorkflowEvent, WorkflowRunRequest,
};
use llm_harness_runtime::workflow::executor::{ExecutorCtx, StepExecutor};
use llm_harness_runtime::workflow::judge::{StepCtx, StepTransitionJudge};
use llm_harness_runtime::workflow::model::{StepResult, StructuredStatus, Transition};
use llm_harness_runtime_knowledge::{KnowledgeAccessContext, KnowledgeCitationPolicy};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;
use tutor_tools::{WebFetchTool, WebSearchTool};

use crate::capability::CapabilityRouter;
use crate::chat::{assistant_message, user_message};
use crate::error::{Result, TutorError};
use crate::event_sink::{SharedEventSink, emit_trace};
use crate::runtime_engine::build_workflow_engine_config;
use crate::runtime_workflow::{research_workflow, validate_research_workflow};

#[derive(Debug, Clone)]
pub struct ResearchWorkflowInput {
    pub request: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchWorkflowSource {
    pub title: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality_label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResearchWorkflowRun {
    pub markdown: String,
    pub sources: Vec<ResearchWorkflowSource>,
    pub cost: CostAggregate,
}

#[derive(Debug, Deserialize)]
struct ResearchReportPayload {
    markdown: String,
    #[serde(default)]
    sources: Vec<ResearchWorkflowSource>,
}

pub async fn run_research_workflow_with_runtime(
    router: &CapabilityRouter,
    input: ResearchWorkflowInput,
    session: Option<Session>,
    _abort_token: Option<CancellationToken>,
    knowledge_access: Option<KnowledgeAccessContext>,
) -> Result<ResearchWorkflowRun> {
    validate_research_workflow()?;

    let workflow_root = router
        .workflow_root
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("llm-tutor-workflow-sessions"))
        .join("research");
    let engine_config = build_workflow_engine_config(
        router.make_client(),
        router.llm.model.clone(),
        router.env.clone(),
        workflow_root,
    );

    let engine = build_research_engine(router, input.clone(), engine_config)?;
    let relay = relay_research_workflow_events(engine.subscribe(), router.event_sink.clone());

    if let Some(session) = session.as_ref() {
        session
            .append_message(user_message(&input.request))
            .await
            .map_err(|err| TutorError::Internal(err.to_string()))?;
    }

    emit_trace(
        &router.event_sink,
        "phase_start",
        serde_json::json!({ "capability": "research", "phase": "workflow" }),
    )
    .await;
    emit_trace(
        &router.event_sink,
        "workflow_validated",
        serde_json::json!({
            "capability": "research",
            "workflow": crate::runtime_workflow::RESEARCH_WORKFLOW_ID,
        }),
    )
    .await;

    let mut request = WorkflowRunRequest::new();
    if let Some(knowledge_access) = knowledge_access {
        request = request.with_extension(knowledge_access);
    }
    let result = engine
        .run_with_request(request)
        .await
        .map_err(|err| TutorError::Internal(format!("research workflow failed: {err}")))?;
    relay.abort();

    let report = engine
        .step_history()
        .await
        .into_iter()
        .rev()
        .find(|record| record.step_id == "write_report")
        .and_then(|record| record.result)
        .ok_or_else(|| TutorError::Internal("research workflow did not write report".into()))?;
    let mut payload = report_payload(report)?;
    score_research_sources(&mut payload.sources);
    if let Some(session) = session.as_ref() {
        session
            .append_message(assistant_message(&payload.markdown))
            .await
            .map_err(|err| TutorError::Internal(err.to_string()))?;
    }

    emit_trace(
        &router.event_sink,
        "research_report_done",
        serde_json::json!({
            "capability": "research",
            "stage": "synthesize",
            "title": "Research report ready",
            "summary": payload.markdown.chars().take(240).collect::<String>(),
            "sources": payload.sources.clone(),
        }),
    )
    .await;
    emit_trace(
        &router.event_sink,
        "phase_end",
        serde_json::json!({ "capability": "research", "phase": "workflow" }),
    )
    .await;
    emit_workflow_runtime_usage(&router.event_sink, &result.cost).await;

    Ok(ResearchWorkflowRun {
        markdown: payload.markdown,
        sources: payload.sources,
        cost: result.cost,
    })
}

fn build_research_engine(
    router: &CapabilityRouter,
    input: ResearchWorkflowInput,
    engine_config: WorkflowEngineConfig,
) -> Result<WorkflowEngine> {
    let mut engine = WorkflowEngine::new(
        research_workflow(),
        engine_config,
        Arc::new(ResearchWorkflowJudge),
    )
    .map_err(|err| TutorError::Internal(format!("research workflow initialization failed: {err}")))?
    .with_executor(
        "tutor.research.prepare",
        Arc::new(PrepareResearchWorkflowExecutor {
            input,
            product_instruction: router.product_instruction.clone(),
        }),
    )
    .with_tool(Arc::new(match router.web_search.clone() {
        Some(config) => WebSearchTool::with_config(config),
        None => WebSearchTool::new(),
    }))
    .with_tool(Arc::new(match router.web_search.clone() {
        Some(config) => WebFetchTool::with_config(config),
        None => WebFetchTool::new(),
    }))
    .with_max_retries(1);

    for tool in router.learner_memory_tools() {
        engine = engine.with_tool(tool);
    }

    for tool in &router.product_tools {
        engine = engine.with_tool(tool.clone());
    }

    if let Some(runtime) = router.knowledge_runtime.clone() {
        let search_runtime = runtime.clone();
        engine = engine.with_step_plugin("search_sources", move || {
            search_runtime.boxed_plugin(KnowledgeCitationPolicy::RequireWhenEvidenceRead)
        });
        let read_runtime = runtime.clone();
        engine = engine.with_step_plugin("read_sources", move || {
            read_runtime.boxed_plugin(KnowledgeCitationPolicy::RequireWhenEvidenceRead)
        });
        engine = engine.with_step_plugin("write_report", move || {
            runtime.boxed_plugin(KnowledgeCitationPolicy::RequireWhenEvidenceRead)
        });
    }

    Ok(engine)
}

struct PrepareResearchWorkflowExecutor {
    input: ResearchWorkflowInput,
    product_instruction: Option<String>,
}

impl StepExecutor for PrepareResearchWorkflowExecutor {
    fn execute<'a>(
        &'a self,
        ctx: &'a ExecutorCtx<'a>,
    ) -> BoxFuture<'a, anyhow::Result<StepResult>> {
        Box::pin(async move {
            {
                let mut context = ctx.context.lock().await;
                context.variables.insert(
                    "research_request".into(),
                    serde_json::json!(self.input.request.clone()),
                );
                if let Some(instruction) = self.product_instruction.as_deref() {
                    context
                        .variables
                        .insert("tutor_instruction".into(), serde_json::json!(instruction));
                }
            }
            Ok(workflow_step_result(
                "research request prepared".into(),
                serde_json::json!({ "prepared": true }),
            ))
        })
    }
}

struct ResearchWorkflowJudge;

impl StepTransitionJudge for ResearchWorkflowJudge {
    fn decide<'a>(&'a self, ctx: &StepCtx<'a>) -> BoxFuture<'a, Transition> {
        let current_step = ctx.current_step.id().clone();
        let structured = ctx.last_result.structured.clone();
        let search_attempts = ctx
            .step_history
            .iter()
            .filter(|record| record.step_id == "search_sources" && record.result.is_some())
            .count();
        Box::pin(async move {
            match current_step.as_str() {
                "prepare_research" => Transition::To("search_sources".into()),
                "search_sources" => Transition::To("read_sources".into()),
                "read_sources" => Transition::To("check_citations".into()),
                "check_citations" => {
                    let verdict = structured
                        .as_ref()
                        .and_then(|value| value.get("verdict"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    if verdict.eq_ignore_ascii_case("pass") {
                        return Transition::To("write_report".into());
                    }
                    if search_attempts < 2 {
                        return Transition::To("search_sources".into());
                    }
                    Transition::Fail {
                        reason: "research citation check failed after repair attempt".into(),
                    }
                }
                "write_report" => Transition::Abort {
                    reason: "research report generated".into(),
                },
                _ => Transition::Fail {
                    reason: format!("research workflow has no transition from {current_step}"),
                },
            }
        })
    }
}

fn report_payload(result: StepResult) -> Result<ResearchReportPayload> {
    if let Some(structured) = result.structured
        && let Ok(payload) = serde_json::from_value::<ResearchReportPayload>(structured)
        && !payload.markdown.trim().is_empty()
    {
        return Ok(ResearchReportPayload {
            markdown: payload.markdown.trim().to_string(),
            sources: payload.sources,
        });
    }
    let markdown = result.output.trim().to_string();
    if markdown.is_empty() {
        return Err(TutorError::Internal(
            "research workflow report is empty".into(),
        ));
    }
    Ok(ResearchReportPayload {
        markdown,
        sources: vec![],
    })
}

fn score_research_sources(sources: &mut [ResearchWorkflowSource]) {
    for source in sources {
        let score = source_quality_score(source);
        source.score = Some(score);
        source.quality_label = Some(source_quality_label(score).into());
    }
}

fn source_quality_score(source: &ResearchWorkflowSource) -> f32 {
    let mut score: f32 = 0.25;
    let url = source.url.trim().to_ascii_lowercase();
    let title = source.title.trim();
    if url.starts_with("https://") {
        score += 0.25;
    } else if url.starts_with("http://") {
        score += 0.12;
    }
    if title.len() >= 8 {
        score += 0.15;
    }
    if source
        .summary
        .as_ref()
        .is_some_and(|value| value.len() >= 40)
    {
        score += 0.15;
    }
    if source_domain(&url).is_some_and(|domain| {
        domain.contains('.') && !domain.ends_with(".test") && !domain.ends_with(".invalid")
    }) {
        score += 0.15;
    }
    score.clamp(0.0, 0.95)
}

fn source_domain(url: &str) -> Option<&str> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    without_scheme
        .split('/')
        .next()
        .filter(|value| !value.is_empty())
}

fn source_quality_label(score: f32) -> &'static str {
    if score >= 0.75 {
        "strong"
    } else if score >= 0.5 {
        "usable"
    } else {
        "weak"
    }
}

fn workflow_step_result(output: String, structured: serde_json::Value) -> StepResult {
    StepResult {
        output,
        structured: Some(structured),
        structured_status: StructuredStatus::NotRequired,
        tool_calls_count: 0,
        session_id: String::new(),
        cost: CostAggregate::default(),
        started_at: None,
        ended_at: None,
    }
}

fn relay_research_workflow_events(
    mut rx: tokio::sync::broadcast::Receiver<WorkflowEvent>,
    sink: Option<SharedEventSink>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut tool_args: HashMap<String, serde_json::Value> = HashMap::new();
        while let Ok(event) = rx.recv().await {
            match event {
                WorkflowEvent::StepStarted { step_id, step_name } => {
                    emit_trace(
                        &sink,
                        "research_stage_start",
                        serde_json::json!({
                            "capability": "research",
                            "stage": research_stage_for_step(&step_id),
                            "title": step_name,
                        }),
                    )
                    .await;
                }
                WorkflowEvent::StepFinished { step_id, result } => {
                    emit_trace(
                        &sink,
                        "research_stage_done",
                        serde_json::json!({
                            "capability": "research",
                            "stage": research_stage_for_step(&step_id),
                            "title": step_id,
                            "summary": result.output.chars().take(240).collect::<String>(),
                            "payload": result.structured,
                        }),
                    )
                    .await;
                }
                WorkflowEvent::StepProgress { progress, .. } => match progress {
                    StepProgress::ToolCallEnd { tool_use_id, args } => {
                        tool_args.insert(tool_use_id, args);
                    }
                    StepProgress::ToolExecutionStart {
                        tool_use_id,
                        tool_name,
                    } => {
                        emit_research_tool_progress(
                            &sink,
                            &tool_name,
                            tool_args.get(&tool_use_id).cloned().unwrap_or_default(),
                        )
                        .await;
                    }
                    _ => {}
                },
                WorkflowEvent::Failed { error } => {
                    emit_trace(
                        &sink,
                        "workflow_failed",
                        serde_json::json!({
                            "capability": "research",
                            "error": error,
                        }),
                    )
                    .await;
                }
                WorkflowEvent::Paused { .. }
                | WorkflowEvent::Resumed
                | WorkflowEvent::Cancelled { .. } => {}
            }
        }
    })
}

async fn emit_research_tool_progress(
    sink: &Option<SharedEventSink>,
    tool_name: &str,
    args: serde_json::Value,
) {
    match tool_name {
        "web_search" => {
            emit_trace(
                sink,
                "research_search",
                serde_json::json!({
                    "capability": "research",
                    "stage": "search",
                    "title": "Search web",
                    "payload": { "args": args },
                }),
            )
            .await;
        }
        "web_fetch" => {
            emit_trace(
                sink,
                "research_read",
                serde_json::json!({
                    "capability": "research",
                    "stage": "read",
                    "title": "Read source",
                    "payload": { "args": args },
                }),
            )
            .await;
        }
        _ => {}
    }
}

fn research_stage_for_step(step_id: &str) -> &'static str {
    match step_id {
        "prepare_research" => "plan",
        "search_sources" => "search",
        "read_sources" => "read",
        "check_citations" | "write_report" => "synthesize",
        _ => "synthesize",
    }
}

async fn emit_workflow_runtime_usage(sink: &Option<SharedEventSink>, cost: &CostAggregate) {
    emit_trace(
        sink,
        "runtime_usage",
        serde_json::json!({
            "capability": "research",
            "input_tokens": cost.total_input_tokens,
            "output_tokens": cost.total_output_tokens,
            "cache_read_tokens": cost.total_cache_read_tokens,
            "cache_write_tokens": cost.total_cache_write_tokens,
            "cost_usd": cost.total_cost,
        }),
    )
    .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scores_research_sources_with_quality_metadata() {
        let mut sources = vec![ResearchWorkflowSource {
            title: "Runtime workflow documentation".into(),
            url: "https://docs.example.com/workflow".into(),
            summary: Some(
                "A reasonably detailed source summary about runtime workflow behavior.".into(),
            ),
            score: None,
            quality_label: None,
        }];

        score_research_sources(&mut sources);

        assert!(sources[0].score.is_some_and(|score| score >= 0.75));
        assert_eq!(sources[0].quality_label.as_deref(), Some("strong"));
    }
}
