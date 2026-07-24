use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::KnowledgeAccessContext;
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde_json::json;
use tutor_agent::CapabilityRouter;

static CREATE_RESEARCH_REPORT_SCHEMA: std::sync::OnceLock<serde_json::Value> =
    std::sync::OnceLock::new();
static PROPOSE_RESEARCH_PLAN_SCHEMA: std::sync::OnceLock<serde_json::Value> =
    std::sync::OnceLock::new();

pub(crate) struct ProposeResearchPlanTool;

pub(crate) struct CreateResearchReportTool {
    router: CapabilityRouter,
}

impl CreateResearchReportTool {
    pub(crate) fn new(router: CapabilityRouter) -> Self {
        Self { router }
    }
}

impl Tool for ProposeResearchPlanTool {
    fn name(&self) -> &str {
        "propose_research_plan"
    }

    fn description(&self) -> &str {
        "Show a research plan for user confirmation. Use this when the user's research goal is mostly clear but the detailed search/read/report workflow should not start until the user confirms. This tool does not search the web or create a report."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        PROPOSE_RESEARCH_PLAN_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short plan title." },
                    "topic": { "type": "string", "description": "Main research topic." },
                    "scope": { "type": "string", "description": "Research scope and boundaries." },
                    "output_format": { "type": "string", "description": "Requested output format such as brief, comparison table, Markdown report, or study note." },
                    "depth": { "type": "string", "enum": ["quick", "standard", "deep"], "description": "Planned research depth." },
                    "time_range": { "type": "string", "description": "Time range or freshness requirement." },
                    "source_preferences": { "type": "array", "items": { "type": "string" }, "description": "Preferred source types such as official docs, papers, news, reports, or Notebook/Knowledge Base material." },
                    "use_notebook": { "type": "boolean", "description": "Whether Notebook context should be used." },
                    "use_knowledge_base": { "type": "boolean", "description": "Whether the selected Knowledge Base should be used." },
                    "steps": { "type": "array", "items": { "type": "string" }, "description": "Planned workflow steps." },
                    "questions": { "type": "array", "items": { "type": "string" }, "description": "Remaining confirmation questions or assumptions." }
                }
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let title = optional_string(&args, "title").unwrap_or_else(|| "Research plan".into());
            let topic = optional_string(&args, "topic").unwrap_or_else(|| "selected topic".into());
            let scope = optional_string(&args, "scope").unwrap_or_else(|| "to be confirmed".into());
            let output_format =
                optional_string(&args, "output_format").unwrap_or_else(|| "Markdown report".into());
            let depth = optional_string(&args, "depth").unwrap_or_else(|| "standard".into());
            let time_range =
                optional_string(&args, "time_range").unwrap_or_else(|| "not specified".into());
            let source_preferences = string_array(&args, "source_preferences");
            let steps = string_array(&args, "steps");
            let questions = string_array(&args, "questions");
            let use_notebook = args["use_notebook"].as_bool().unwrap_or(false);
            let use_knowledge_base = args["use_knowledge_base"].as_bool().unwrap_or(false);

            let content = vec![DataBlock::text(format!(
                "Proposed research plan: {title}. Topic: {topic}. Scope: {scope}. Ask the user to confirm or revise before starting detailed research."
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "title": title,
                    "topic": topic,
                    "scope": scope,
                    "output_format": output_format,
                    "depth": depth,
                    "time_range": time_range,
                    "source_preferences": source_preferences,
                    "use_notebook": use_notebook,
                    "use_knowledge_base": use_knowledge_base,
                    "steps": steps,
                    "questions": questions,
                }),
                false,
            ))
        })
    }
}

impl Tool for CreateResearchReportTool {
    fn name(&self) -> &str {
        "create_research_report"
    }

    fn description(&self) -> &str {
        "Run the detailed Research workflow after the user explicitly asks to produce the report or confirms a research plan. This tool searches/reads sources, verifies citation readiness, writes the Markdown report, and returns report metadata. Do not call it for ordinary clarification chat."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        CREATE_RESEARCH_REPORT_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "request": {
                        "type": "string",
                        "description": "Confirmed research request, including topic, scope, output format, source preferences, freshness requirements, and any relevant conversation context."
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional short report title for UI metadata."
                    }
                },
                "required": ["request"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let request = optional_string(&args, "request").ok_or_else(|| {
                ToolFailure::invalid_arguments("create_research_report requires request")
            })?;
            let requested_title = optional_string(&args, "title");
            let run = tutor_agent::research::run_research_workflow_with_runtime(
                &self.router,
                tutor_agent::research::ResearchWorkflowInput {
                    request: request.clone(),
                },
                None,
                Some(ctx.abort.clone()),
                ctx.run.extension::<KnowledgeAccessContext>().cloned(),
            )
            .await
            .map_err(|err| tool_execution_failure(err.to_string()))?;
            let title = requested_title.unwrap_or_else(|| {
                report_title_from_markdown(&run.markdown)
                    .unwrap_or_else(|| report_title_from_request(&request))
            });

            let content = vec![DataBlock::text(format!(
                "Research report \"{title}\" is ready with {} source(s). The product UI will render the report from tool metadata.",
                run.sources.len()
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "title": title,
                    "request": request,
                    "markdown": run.markdown,
                    "sources": run.sources,
                }),
                false,
            ))
        })
    }
}

fn optional_string(args: &serde_json::Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn report_title_from_markdown(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim();
        let heading = trimmed.trim_start_matches('#');
        if heading.len() == trimmed.len()
            || !heading.chars().next().is_some_and(char::is_whitespace)
        {
            return None;
        }
        let title = heading.trim();
        (!title.is_empty()).then(|| title.chars().take(120).collect())
    })
}

fn report_title_from_request(request: &str) -> String {
    let title = request
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Research Report")
        .trim()
        .trim_end_matches(['。', '.', '!', '！', '?', '？'])
        .chars()
        .take(120)
        .collect::<String>();
    if title.is_empty() {
        "Research Report".into()
    } else {
        title
    }
}

fn string_array(args: &serde_json::Value, key: &str) -> Vec<String> {
    args[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn tool_execution_failure(message: impl Into<String>) -> ToolFailure {
    ToolFailure::new("research_tool_failed", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_harness_runtime_sandbox_os::OsEnv;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn make_ctx() -> ToolContext {
        let (tx, _rx) = mpsc::channel(1);
        ToolContext {
            env: Arc::new(OsEnv::new(std::env::temp_dir())),
            run: Arc::new(llm_harness_types::RunContext::new(
                llm_harness_types::RunRequest::default(),
            )),
            abort: CancellationToken::new(),
            tool_use_id: "test-id".into(),
            turn_index: 0,
            assistant_message: Arc::new(llm_harness_types::AssistantMessage {
                kind: llm_harness_types::AssistantMessageKind::FinalAnswer,
                message_id: "test-message".into(),
                turn_id: "test-turn".into(),
                content: vec![],
                usage: None,
                stop_reason: None,
                timestamp: chrono::Utc::now(),
                provider: None,
                api: None,
                model: None,
                error_message: None,
            }),
            update_tx: tx,
        }
    }

    #[tokio::test]
    async fn propose_research_plan_returns_structured_details() {
        let tool = ProposeResearchPlanTool;
        let ctx = make_ctx();
        let result = tool
            .execute(
                json!({
                    "title": "Runtime research",
                    "topic": "llm-harness-runtime",
                    "scope": "workflow APIs",
                    "output_format": "Markdown report",
                    "depth": "standard",
                    "time_range": "2026",
                    "source_preferences": ["official docs"],
                    "use_notebook": true,
                    "use_knowledge_base": false,
                    "steps": ["Search", "Read", "Synthesize"],
                    "questions": ["Confirm source scope"]
                }),
                &ctx,
            )
            .await
            .unwrap();

        assert_eq!(result.details["title"], "Runtime research");
        assert_eq!(result.details["topic"], "llm-harness-runtime");
        assert_eq!(result.details["source_preferences"][0], "official docs");
        assert_eq!(result.details["use_notebook"], true);
        assert!(!result.terminate);
    }

    #[test]
    fn create_research_report_declares_tool_boundary() {
        let router = tutor_agent::CapabilityRouter::new(
            Arc::new(OsEnv::new(std::env::temp_dir())),
            tutor_agent::LlmConfig::anthropic("test-model", "test-key"),
            tutor_agent::governance::GovernanceConfig::new(1.0, None, false),
        );
        let tool = CreateResearchReportTool::new(router);
        let schema = tool.parameters_schema();

        assert_eq!(tool.name(), "create_research_report");
        assert!(tool.description().contains("detailed Research workflow"));
        assert_eq!(schema["required"][0], "request");
        assert_eq!(schema["properties"]["request"]["type"], "string");
    }

    #[test]
    fn report_title_prefers_first_markdown_heading() {
        assert_eq!(
            report_title_from_markdown("Intro paragraph.\n\n## Transformer architecture\nBody"),
            Some("Transformer architecture".into())
        );
        assert_eq!(report_title_from_markdown("#Missing space\nBody"), None);
    }

    #[test]
    fn report_title_falls_back_to_trimmed_request() {
        assert_eq!(
            report_title_from_request("  调研 Transformer 架构。\n补充要求"),
            "调研 Transformer 架构"
        );
        assert_eq!(report_title_from_request("\n\n"), "Research Report");
        assert_eq!(report_title_from_request("。"), "Research Report");
    }
}
