use std::collections::HashMap;
use std::sync::Arc;

use llm_harness_agent::{AgentHarness, AgentHarnessEvent, Session};
use llm_harness_loop::FinalAnswerMode;
use llm_harness_types::{
    AgentEvent, AgentMessage, AssistantMessage, AssistantMessageKind, CompactionError,
    ContentBlock, HarnessError, RunRequest, StopReason, UserMessage,
};
use tokio_util::sync::CancellationToken;
use tutor_tools::{CodeExecTool, WebFetchTool, WebSearchTool};

use crate::capability::CapabilityRouter;
use crate::error::{Result, TutorError};
use crate::event_sink::{emit_content, emit_trace};
use crate::runtime_harness::{RuntimeHarnessConfig, build_runtime_harness};

/// Run a single Chat turn through runtime-owned Knowledge and web tools.
/// Creates a fresh in-memory harness per call (stateless in v0.1).
pub async fn run_chat(router: &CapabilityRouter, question: &str) -> Result<String> {
    run_chat_with_messages(router, vec![user_message(question)]).await
}

pub async fn run_chat_with_messages(
    router: &CapabilityRouter,
    messages: Vec<AgentMessage>,
) -> Result<String> {
    run_conversation_with_request(router, "chat", RunRequest::new(messages), None, None).await
}

pub async fn run_chat_with_session(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
) -> Result<String> {
    run_chat_with_session_cancel(router, session, question, None).await
}

pub async fn run_chat_with_session_cancel(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    run_conversation_with_request(
        router,
        "chat",
        RunRequest::from_text(question),
        Some(session),
        abort_token,
    )
    .await
}

pub async fn run_research_with_messages(
    router: &CapabilityRouter,
    messages: Vec<AgentMessage>,
) -> Result<String> {
    run_conversation_with_request(router, "research", RunRequest::new(messages), None, None).await
}

pub async fn run_research_with_session(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
) -> Result<String> {
    run_research_with_session_cancel(router, session, question, None).await
}

pub async fn run_research_with_session_cancel(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    run_conversation_with_request(
        router,
        "research",
        RunRequest::from_text(question),
        Some(session),
        abort_token,
    )
    .await
}

pub async fn run_organize_with_messages(
    router: &CapabilityRouter,
    messages: Vec<AgentMessage>,
) -> Result<String> {
    run_conversation_with_request(router, "organize", RunRequest::new(messages), None, None).await
}

pub async fn run_organize_with_session(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
) -> Result<String> {
    run_organize_with_session_cancel(router, session, question, None).await
}

pub async fn run_organize_with_session_cancel(
    router: &CapabilityRouter,
    session: Session,
    question: &str,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    run_conversation_with_request(
        router,
        "organize",
        RunRequest::from_text(question),
        Some(session),
        abort_token,
    )
    .await
}

pub(crate) async fn run_conversation_with_request(
    router: &CapabilityRouter,
    capability: &'static str,
    request: RunRequest,
    session: Option<Session>,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    let system_prompt = match capability {
        "chat" => chat_system_prompt(),
        "research" => research_system_prompt(),
        "organize" => organize_system_prompt(),
        other => {
            return Err(TutorError::Internal(format!(
                "unsupported conversational capability: {other}"
            )));
        }
    };
    let system_prompt = router.apply_runtime_instructions(&system_prompt);
    emit_trace(
        &router.event_sink,
        "phase_start",
        serde_json::json!({ "capability": capability, "phase": "respond" }),
    )
    .await;
    if capability == "research" {
        emit_trace(
            &router.event_sink,
            "research_stage_start",
            serde_json::json!({
                "capability": "research",
                "stage": "plan",
                "title": "Plan research"
            }),
        )
        .await;
    }

    let mut tools: Vec<Arc<dyn llm_harness_types::Tool>> = vec![
        Arc::new(match router.web_search.clone() {
            Some(config) => WebSearchTool::with_config(config),
            None => WebSearchTool::new(),
        }),
        Arc::new(match router.web_search.clone() {
            Some(config) => WebFetchTool::with_config(config),
            None => WebFetchTool::new(),
        }),
        Arc::new(CodeExecTool::new()),
    ];
    let mut plugins = Vec::new();
    if conversation_uses_runtime_knowledge(capability)
        && let Some(knowledge_runtime) = &router.knowledge_runtime
    {
        plugins.push(knowledge_runtime.plugin());
    }
    if conversation_uses_runtime_knowledge(capability)
        && let Some(memory_service) = &router.memory_service
    {
        plugins.push(Arc::new(llm_harness_runtime_memory::MemoryPlugin::new(
            memory_service.clone(),
        )));
    }
    tools.extend(router.product_tools.iter().cloned());

    let client = router.make_client();

    let has_session = session.is_some();
    let harness = Arc::new(
        build_runtime_harness(
            client,
            router.env.clone(),
            session,
            RuntimeHarnessConfig {
                model: router.llm.model.clone(),
                model_info: router.llm.model_info(8192),
                tools,
                plugins,
                system_prompt,
                final_answer_mode: final_answer_mode_for_capability(capability),
                before_tool_call: vec![],
                prepare_next_turn: vec![],
            },
        )
        .await?,
    );
    if has_session {
        try_auto_compact(&harness, router, capability).await;
    }
    if let Some(token) = abort_token {
        harness.set_abort_token(token);
    }
    let mut rx = harness.subscribe();
    let prompt_harness = harness.clone();
    let prompt_task = tokio::spawn(async move { prompt_harness.run(request).await });

    // Collect the last complete assistant message.
    let mut last_text = String::new();
    let mut fallback_text = String::new();
    let mut last_error: Option<String> = None;
    let mut tool_names: HashMap<String, String> = HashMap::new();
    let mut saw_tool_execution = false;
    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                emit_trace(
                    &router.event_sink,
                    "event_lagged",
                    serde_json::json!({ "capability": capability, "skipped": skipped }),
                )
                .await;
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        };

        if let AgentHarnessEvent::Agent(agent_event) = event.as_ref() {
            if let Some((message_id, turn_id, text)) = agent_event.as_final_answer() {
                last_text = text.clone();
                emit_trace(
                    &router.event_sink,
                    "final_answer",
                    serde_json::json!({
                        "capability": capability,
                        "message_id": message_id,
                        "turn_id": turn_id,
                    }),
                )
                .await;
                continue;
            }

            if let Some((message_id, turn_id, text)) = agent_event.as_progress() {
                emit_trace(
                    &router.event_sink,
                    "assistant_progress",
                    serde_json::json!({
                        "capability": capability,
                        "message_id": message_id,
                        "turn_id": turn_id,
                        "summary": text.chars().take(240).collect::<String>(),
                    }),
                )
                .await;
                continue;
            }
        }

        match event.as_ref() {
            AgentHarnessEvent::Agent(AgentEvent::TextDelta { text, .. }) => {
                let TextDeltaRoute::FinalAnswer = text_delta_route_for_capability(capability);
                emit_content(&router.event_sink, text.clone(), true).await;
                fallback_text.push_str(text);
            }
            AgentHarnessEvent::Agent(AgentEvent::ToolExecutionStart {
                tool_use_id,
                tool_name,
                args,
            }) => {
                saw_tool_execution = true;
                tool_names.insert(tool_use_id.clone(), tool_name.clone());
                emit_trace(
                    &router.event_sink,
                    "tool_call",
                    serde_json::json!({
                        "capability": capability,
                        "tool_use_id": tool_use_id,
                        "tool": tool_name,
                        "args": args,
                    }),
                )
                .await;
                if capability == "research" && tool_name == "web_search" {
                    emit_trace(
                        &router.event_sink,
                        "research_search",
                        serde_json::json!({
                            "capability": "research",
                            "stage": "search",
                            "title": "Search web",
                            "payload": { "args": args },
                        }),
                    )
                    .await;
                } else if capability == "research" && tool_name == "web_fetch" {
                    emit_trace(
                        &router.event_sink,
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
            }
            AgentHarnessEvent::Agent(AgentEvent::ToolExecutionEnd {
                tool_use_id,
                result,
            }) => {
                let tool_name = tool_names
                    .get(tool_use_id)
                    .cloned()
                    .unwrap_or_else(|| "tool".into());
                let details = result.as_ref().ok().map(|result| result.details.clone());
                emit_trace(
                    &router.event_sink,
                    "tool_result",
                    serde_json::json!({
                        "capability": capability,
                        "tool_use_id": tool_use_id,
                        "tool": tool_name,
                        "ok": result.is_ok(),
                        "details": details,
                    }),
                )
                .await;
            }
            AgentHarnessEvent::Agent(AgentEvent::Error(err)) => {
                last_error = Some(err.to_string());
            }
            AgentHarnessEvent::Agent(AgentEvent::AgentEnd { new_messages })
                if last_text.is_empty() =>
            {
                last_text = last_assistant_text(new_messages).unwrap_or_default();
            }
            AgentHarnessEvent::Settled | AgentHarnessEvent::Aborted => break,
            _ => {}
        }
    }
    prompt_task
        .await
        .map_err(|err| TutorError::Internal(format!("agent prompt task failed: {err}")))??;

    emit_trace(
        &router.event_sink,
        "phase_end",
        serde_json::json!({ "capability": capability, "phase": "respond" }),
    )
    .await;
    emit_runtime_usage(&harness, router, capability).await;
    if capability == "research" && looks_like_research_report(&last_text) {
        emit_trace(
            &router.event_sink,
            "research_report_done",
            serde_json::json!({
                "capability": "research",
                "stage": "synthesize",
                "title": "Research report ready",
                "summary": last_text.chars().take(240).collect::<String>(),
            }),
        )
        .await;
    }

    if let Some(error) = last_error {
        return Err(TutorError::Internal(error));
    }

    if last_text.is_empty() {
        let fallback_text = fallback_text.trim();
        if !saw_tool_execution && !fallback_text.is_empty() {
            return Ok(fallback_text.to_string());
        }
        return Err(TutorError::Internal(
            "agent settled without assistant text".into(),
        ));
    }

    Ok(last_text)
}

fn final_answer_mode_for_capability(capability: &str) -> FinalAnswerMode {
    let _ = capability;
    FinalAnswerMode::tool_with_text_fallback()
}

fn conversation_uses_runtime_knowledge(capability: &str) -> bool {
    matches!(capability, "chat" | "research")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextDeltaRoute {
    FinalAnswer,
}

fn text_delta_route_for_capability(capability: &str) -> TextDeltaRoute {
    let _ = capability;
    TextDeltaRoute::FinalAnswer
}

fn looks_like_research_report(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("## summary")
        && normalized.contains("## sources")
        && (normalized.contains("## key findings") || normalized.contains("## analysis"))
}

pub(crate) async fn emit_runtime_usage(
    harness: &AgentHarness,
    router: &CapabilityRouter,
    capability: &str,
) {
    let usage = harness.usage();
    emit_trace(
        &router.event_sink,
        "runtime_usage",
        serde_json::json!({
            "capability": capability,
            "input_tokens": usage.total_input_tokens,
            "output_tokens": usage.total_output_tokens,
            "cache_read_tokens": usage.total_cache_read_tokens,
            "cache_write_tokens": usage.total_cache_write_tokens,
        }),
    )
    .await;
}

pub(crate) async fn try_auto_compact(
    harness: &AgentHarness,
    router: &CapabilityRouter,
    capability: &str,
) {
    match harness.compact().await {
        Ok(stats) => {
            emit_trace(
                &router.event_sink,
                "context_compacted",
                serde_json::json!({
                    "capability": capability,
                    "tokens_before": stats.tokens_before,
                    "tokens_after": stats.tokens_after,
                    "compressed_entries": stats.compressed_entries,
                }),
            )
            .await;
        }
        Err(HarnessError::Compaction(CompactionError::InsufficientTokens)) => {}
        Err(err) => {
            emit_trace(
                &router.event_sink,
                "context_compaction_skipped",
                serde_json::json!({
                    "capability": capability,
                    "reason": err.to_string(),
                }),
            )
            .await;
        }
    }
}

pub fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        timestamp: chrono::Utc::now(),
    })
}

pub fn assistant_message(text: &str) -> AgentMessage {
    AgentMessage::Assistant(AssistantMessage {
        kind: AssistantMessageKind::FinalAnswer,
        message_id: "manual_assistant_message".into(),
        turn_id: "manual_turn".into(),
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        stop_reason: Some(StopReason::EndTurn),
        timestamp: chrono::Utc::now(),
        provider: None,
        api: None,
        model: None,
        usage: None,
        error_message: None,
    })
}

fn last_assistant_text(messages: &[AgentMessage]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        let AgentMessage::Assistant(message) = message else {
            return None;
        };
        if message.kind != AssistantMessageKind::FinalAnswer {
            return None;
        }

        let text = message.text_content();
        (!text.is_empty()).then_some(text)
    })
}

fn chat_system_prompt() -> String {
    "Use the product-provided Assistant Profile, when present, for identity and communication style. When knowledge_search and knowledge_read are available, \
     use them for facts from the selected Knowledge Base. For a selected Knowledge Base search, set source_id to exactly `course_knowledge`. Search first, then read only the exact \
     opaque references returned by knowledge_search. Base Knowledge Base claims on content returned \
     by knowledge_read, and cite only the citation handles returned by that read. Never cite a \
     search hit that you did not read, never invent a citation handle, and never ask the user for \
     a source ID, item ID, revision, or authorization scope. \
     Use search_notebook when Notebook is associated and saved Markdown notes may be relevant. \
     When the user references a Notebook entry, read the exact entry before relying on its content. \
     When the user explicitly asks you to create a Notebook item, use create_notebook_item. When the user explicitly asks you to modify, rename, or move an existing Notebook item, call read_notebook_item first and then use update_notebook_item or move_notebook_item with the exact returned revision. Use propose_notebook_edit only for self-initiated suggestions that the user did not explicitly request. Never delete Notebook content. \
     When product-provided Saved Memory or History Recall instructions are present, follow their source routing, mandatory search-before-unknown, retry, and exact-read requirements before making claims about what the user previously shared. \
     When memory_write and memory_forget are available, follow the product's Saved Memory permission instruction. You may use memory_write when the user explicitly asks, or when they directly state clearly durable, personally useful context such as a preferred name, stable preference, goal, or continuity item. Do not infer unstated facts or capture transient or sensitive details merely because they appear useful. Use memory_forget only for the exact item the user asks to forget. The product instruction states whether writes require separate confirmation; never claim success before the tool result. \
     Web verification rules are strict: when the user asks you to collect facts, trivia, \
     current information, latest information, sources, external references, or information \
     about real-world/public entities, products, games, communities, papers, libraries, \
     events, or online content, you must call web_search before answering. After web_search, \
     use web_fetch to read important source pages before making citation-backed or factual \
     claims. If web_search or web_fetch fails, say what could not be verified instead of \
     inventing facts from memory. Use code_exec when the user asks to run or verify code. \
     For non-trivial numeric calculations, approximations, transcendental functions, \
     statistics, simulations, or any answer where exact arithmetic matters, call code_exec \
     with Python to compute or verify the result before answering."
        .into()
}

fn research_system_prompt() -> String {
    "You are a research tutor. Your job is to help the user clarify research needs and, when appropriate, turn a confirmed topic into a sourced, reusable research report. \
     Research findings belong in reports, not memory. \
     Research has two modes: Research Chat and Detailed Research Workflow. \
     When knowledge_search and knowledge_read are available in Research Chat, use them only when selected course material is relevant: set source_id to exactly `course_knowledge`, search first, read exact returned references, and cite only handles returned by knowledge_read. Never invent a Knowledge citation or ask the user to provide opaque identifiers or authorization scope. \
     In Research Chat, discuss the topic, ask focused clarification questions, and help define goal, scope, source preferences, output format, depth, time range, and whether Notebook or Knowledge Base context should be used. \
     Do not call web_search, web_fetch, or produce a full report when the user's request is ambiguous or they are only discussing scope. \
     When the research need is mostly clear but not confirmed, call propose_research_plan with the proposed topic, scope, output format, depth, time range, sources, and workflow steps, then ask the user to confirm or revise it. \
     Call create_research_report only when the user explicitly asks to begin, confirms a proposed plan, or gives an unambiguous instruction to produce the report now. \
     Do not start the Detailed Research Workflow through free-form chat text; create_research_report is the workflow boundary. \
     For the Detailed Research Workflow: (1) identify the confirmed research question and scope, \
     (2) call web_search for external facts, (3) call web_fetch on the most relevant sources before relying on them, \
     (4) read exact referenced Notebook entries, (5) optionally call search_notebook when Notebook is associated, (6) carry any confirmed Knowledge Base source preference into create_research_report, \
     (7) synthesize a Markdown report. Do not answer detailed research requests from memory when external verification is needed. \
     If the user explicitly asks to create, update, rename, or move Notebook content, use the bounded Notebook mutation tools; read an existing item first and pass its exact revision. Use propose_notebook_edit only for self-initiated suggestions. Never delete Notebook content. \
     If search or fetch fails, clearly state what failed and what remains unverified. \
     When create_research_report completes, briefly tell the user the report is ready; the product UI renders the full report from tool metadata. The report must be Markdown with these sections: Title, Summary, Key Findings, Analysis, Limitations, Follow-up Questions, Sources. \
     Cite factual claims using numbered source references that match the Sources section. \
     Keep workflow progress brief; the final report is the main deliverable."
        .into()
}

fn organize_system_prompt() -> String {
    "You are a Notebook organization assistant. Your job is to help the user search, \
     inspect, clean up, link, tag, deduplicate, and revise saved Notebook content. Notebook is a \
     plain-text Markdown workspace, not a vector knowledge base. Prefer search_notebook when the \
     user asks about saved notes, prior notes, Notebook contents, organization, tags, links, or \
     duplicates. When the user explicitly requests a create, update, rename, or move, use create_notebook_item, update_notebook_item, or move_notebook_item. Read an existing item with read_notebook_item first and pass its exact revision. For self-initiated organization suggestions, use propose_notebook_edit with complete replacement Markdown; set proposal_kind to links, tags, merge, or edit, and include suggested_links, suggested_tags, or merge_source_entry_ids when relevant. Never delete Notebook content. Only claim a write succeeded after its tool result confirms it. You may use code_exec for parsing or verification if it \
     helps, and web_search only when the user explicitly asks for external/current facts. Keep \
     organization suggestions concrete and cite the Notebook entries you used."
        .into()
}

#[cfg(test)]
mod tests {
    use super::{
        TextDeltaRoute, chat_system_prompt, conversation_uses_runtime_knowledge,
        final_answer_mode_for_capability, looks_like_research_report, organize_system_prompt,
        research_system_prompt, text_delta_route_for_capability,
    };
    use llm_harness_loop::{FinalAnswerMissingBehavior, FinalAnswerMode};

    #[test]
    fn chat_prompt_requires_web_search_for_fact_collection() {
        let prompt = chat_system_prompt();
        assert!(!prompt.contains("rag_search"));
        assert!(prompt.contains("knowledge_search"));
        assert!(prompt.contains("knowledge_read"));
        assert!(prompt.contains("source_id to exactly `course_knowledge`"));
        assert!(prompt.contains("Search first"));
        assert!(prompt.contains("cite only the citation handles"));
        assert!(prompt.contains("never invent a citation handle"));
        assert!(prompt.contains("read the exact entry"));
        assert!(prompt.contains("propose_notebook_edit"));
        assert!(prompt.contains("mandatory search-before-unknown"));
        assert!(prompt.contains("exact-read requirements"));
        assert!(prompt.contains("directly state clearly durable"));
        assert!(prompt.contains("Do not infer unstated facts"));
        assert!(prompt.contains("whether writes require separate confirmation"));
        assert!(prompt.contains("collect facts"));
        assert!(prompt.contains("trivia"));
        assert!(prompt.contains("must call web_search before answering"));
        assert!(prompt.contains("If web_search or web_fetch fails"));
    }

    #[test]
    fn research_prompt_requires_search_fetch_and_report() {
        let prompt = research_system_prompt();
        assert!(!prompt.contains("rag_search"));
        assert!(prompt.contains("knowledge_search"));
        assert!(prompt.contains("knowledge_read"));
        assert!(prompt.contains("source_id to exactly `course_knowledge`"));
        assert!(prompt.contains("search first"));
        assert!(prompt.contains("cite only handles"));
        assert!(prompt.contains("Research findings belong in reports"));
        assert!(prompt.contains("Research Chat and Detailed Research Workflow"));
        assert!(prompt.contains("Do not call web_search"));
        assert!(prompt.contains("propose_research_plan"));
        assert!(prompt.contains("create_research_report"));
        assert!(prompt.contains("workflow boundary"));
        assert!(prompt.contains("explicitly asks to begin"));
        assert!(prompt.contains("call web_search"));
        assert!(prompt.contains("call web_fetch"));
        assert!(prompt.contains("read exact referenced Notebook entries"));
        assert!(prompt.contains("propose_notebook_edit"));
        assert!(prompt.contains("Markdown report"));
        assert!(prompt.contains("Sources"));
        assert!(!prompt.contains("final_answer"));
    }

    #[test]
    fn only_migrated_conversations_install_runtime_knowledge() {
        assert!(conversation_uses_runtime_knowledge("chat"));
        assert!(conversation_uses_runtime_knowledge("research"));
        assert!(!conversation_uses_runtime_knowledge("quiz"));
        assert!(!conversation_uses_runtime_knowledge("organize"));
    }

    #[test]
    fn research_allows_chat_fallback_before_workflow() {
        match final_answer_mode_for_capability("chat") {
            FinalAnswerMode::Tool(config) => {
                assert_eq!(
                    config.missing_behavior,
                    FinalAnswerMissingBehavior::FallbackToText
                );
            }
            other => panic!("expected final answer tool fallback, got {other:?}"),
        }
        match final_answer_mode_for_capability("research") {
            FinalAnswerMode::Tool(config) => {
                assert_eq!(
                    config.missing_behavior,
                    FinalAnswerMissingBehavior::FallbackToText
                );
            }
            other => panic!("expected final answer tool fallback, got {other:?}"),
        }
    }

    #[test]
    fn research_chat_routes_text_delta_to_final_answer_channel() {
        assert_eq!(
            text_delta_route_for_capability("research"),
            TextDeltaRoute::FinalAnswer
        );
        assert_eq!(
            text_delta_route_for_capability("chat"),
            TextDeltaRoute::FinalAnswer
        );
    }

    #[test]
    fn research_report_detection_requires_report_sections() {
        assert!(!looks_like_research_report(
            "Sure. What scope and output format should I use?"
        ));
        assert!(looks_like_research_report(
            "# Topic\n\n## Summary\n\nBrief.\n\n## Key Findings\n\n- One.\n\n## Sources\n\n[1] Source"
        ));
    }

    #[test]
    fn organize_prompt_separates_explicit_writes_from_self_initiated_proposals() {
        let prompt = organize_system_prompt();
        assert!(prompt.contains("search_notebook"));
        assert!(prompt.contains("plain-text Markdown workspace"));
        assert!(prompt.contains("propose_notebook_edit"));
        assert!(prompt.contains("create_notebook_item"));
        assert!(prompt.contains("update_notebook_item"));
        assert!(prompt.contains("move_notebook_item"));
        assert!(prompt.contains("exact revision"));
        assert!(prompt.contains("proposal_kind"));
        assert!(prompt.contains("suggested_links"));
        assert!(prompt.contains("suggested_tags"));
        assert!(prompt.contains("merge_source_entry_ids"));
        assert!(prompt.contains("self-initiated organization suggestions"));
        assert!(prompt.contains("Never delete Notebook content"));
    }
}
