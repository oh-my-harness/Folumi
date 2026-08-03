use std::sync::Arc;

use llm_harness_agent::{AgentHarnessEvent, Session};
use llm_harness_loop::FinalAnswerMode;
use llm_harness_types::{
    AgentEvent, AgentMessage, AssistantMessageKind, BeforeToolCallHook, RunRequest,
};
use tokio_util::sync::CancellationToken;
use tutor_tools::CodeExecTool;

use crate::capability::CapabilityRouter;
use crate::error::{Result, TutorError};
use crate::event_sink::{emit_content, emit_trace};
use crate::runtime_harness::{RuntimeHarnessConfig, build_runtime_harness};

/// Run code execution as an agent turn: the model calls `code_exec`, then explains the result.
pub async fn run_code_exec(router: &CapabilityRouter, request: &str) -> Result<String> {
    run_code_exec_with_messages(router, vec![crate::chat::user_message(request)]).await
}

pub async fn run_code_exec_with_messages(
    router: &CapabilityRouter,
    messages: Vec<AgentMessage>,
) -> Result<String> {
    run_code_exec_with_request(router, RunRequest::new(messages), None, None).await
}

pub async fn run_code_exec_with_session(
    router: &CapabilityRouter,
    session: Session,
    request: &str,
) -> Result<String> {
    run_code_exec_with_session_cancel(router, session, request, None).await
}

pub async fn run_code_exec_with_session_cancel(
    router: &CapabilityRouter,
    session: Session,
    request: &str,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    run_code_exec_with_request(
        router,
        RunRequest::from_text(request),
        Some(session),
        abort_token,
    )
    .await
}

pub(crate) async fn run_code_exec_with_request(
    router: &CapabilityRouter,
    request: RunRequest,
    session: Option<Session>,
    abort_token: Option<CancellationToken>,
) -> Result<String> {
    emit_trace(
        &router.event_sink,
        "phase_start",
        serde_json::json!({ "capability": "code_exec", "phase": "execute" }),
    )
    .await;

    let tools: Vec<Arc<dyn llm_harness_types::Tool>> = vec![Arc::new(CodeExecTool::new())];
    let before_tool_call: Vec<Arc<dyn BeforeToolCallHook>> =
        if let Some(approval) = &router.governance.approval {
            vec![approval.clone()]
        } else {
            vec![]
        };

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
                plugins: router.runtime_plugins.clone(),
                system_prompt: router.apply_product_instruction(
                    "You are a code execution assistant. When the user asks to run code, \
             call code_exec with the correct language and code, then explain stdout, stderr, \
             and exit code clearly. For non-trivial numeric calculations or approximations, \
             call code_exec with Python to compute or verify the result before answering. If no \
             runnable code or computable task is provided, ask for the missing details.",
                ),
                final_answer_mode: FinalAnswerMode::tool_with_text_fallback(),
                before_tool_call,
                prepare_next_turn: vec![],
            },
        )
        .await?,
    );
    if has_session {
        crate::chat::try_auto_compact(&harness, router, "code_exec").await;
    }
    if let Some(token) = abort_token {
        harness.set_abort_token(token);
    }
    let mut rx = harness.subscribe();
    let prompt_harness = harness.clone();
    let prompt_task = tokio::spawn(async move { prompt_harness.run(request).await });

    let mut last_text = String::new();
    let mut last_error: Option<String> = None;
    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                emit_trace(
                    &router.event_sink,
                    "event_lagged",
                    serde_json::json!({ "capability": "code_exec", "skipped": skipped }),
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
                        "capability": "code_exec",
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
                        "capability": "code_exec",
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
                emit_content(&router.event_sink, text.clone(), true).await;
            }
            AgentHarnessEvent::Agent(AgentEvent::ToolExecutionStart {
                tool_use_id,
                tool_name,
                args,
            }) => {
                emit_trace(
                    &router.event_sink,
                    "tool_call",
                    serde_json::json!({
                        "capability": "code_exec",
                        "tool_use_id": tool_use_id,
                        "tool": tool_name,
                        "args": args,
                    }),
                )
                .await;
            }
            AgentHarnessEvent::Agent(AgentEvent::ToolExecutionEnd {
                tool_use_id,
                result,
            }) => {
                emit_trace(
                    &router.event_sink,
                    "tool_result",
                    serde_json::json!({
                        "capability": "code_exec",
                        "tool_use_id": tool_use_id,
                        "ok": result.is_ok(),
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
        serde_json::json!({ "capability": "code_exec", "phase": "execute" }),
    )
    .await;
    crate::chat::emit_runtime_usage(&harness, router, "code_exec").await;

    if let Some(error) = last_error {
        return Err(TutorError::Internal(error));
    }

    if last_text.is_empty() {
        return Err(TutorError::Internal(
            "agent settled without assistant text".into(),
        ));
    }

    Ok(last_text)
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
