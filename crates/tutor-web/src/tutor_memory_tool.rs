#![allow(clippy::result_large_err)] // Tool helpers must return the runtime-owned ToolFailure value.

use std::sync::{Arc, OnceLock};

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::KnowledgeRequestContext;
use llm_harness_runtime_memory::{MemoryMutationOrigin, MemoryService};
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde_json::json;

use crate::tutor_memory_store::{TutorMemoryKind, TutorMemoryStatus, TutorMemoryStore};
use crate::tutor_memory_write::tutor_memory_write_intent;

static REMEMBER_SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
static RESOLVE_SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();

pub struct RememberForLaterTool {
    service: Arc<MemoryService>,
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
}

pub struct ResolveTutorMemoryTool {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
}

impl RememberForLaterTool {
    pub fn new(
        service: Arc<MemoryService>,
        store: Arc<TutorMemoryStore>,
        tutor_id: impl Into<String>,
    ) -> Self {
        Self {
            service,
            store,
            tutor_id: tutor_id.into(),
        }
    }
}

impl ResolveTutorMemoryTool {
    pub fn new(store: Arc<TutorMemoryStore>, tutor_id: impl Into<String>) -> Self {
        Self {
            store,
            tutor_id: tutor_id.into(),
        }
    }
}

impl Tool for RememberForLaterTool {
    fn name(&self) -> &str {
        "remember_for_later"
    }

    fn description(&self) -> &str {
        "Save low-risk private continuity memory for this tutor: a promise the tutor made, an unresolved follow-up, a lesson plan, a reflection on teaching, or a concrete future teaching strategy. Never store learner profile facts, credentials, sensitive personal data, external factual claims, or unsupported judgments here."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        REMEMBER_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "required": ["kind", "text"],
                "properties": {
                    "kind": { "type": "string", "enum": ["commitment", "open_loop", "lesson_plan", "reflection", "strategy"] },
                    "text": { "type": "string", "description": "Concise relationship-specific item." },
                    "next_action": { "type": "string", "description": "Optional concrete next action." },
                    "source_message_id": { "type": "string", "description": "Optional originating runtime message id." }
                }
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let kind = required_kind(&args, "kind")?;
            let text = required_string(&args, "text")?;
            let intent = tutor_memory_write_intent(
                kind,
                text,
                optional_string(&args, "next_action"),
                optional_string(&args, "source_message_id"),
            )
            .map_err(|error| tool_execution_failure(error.to_string()))?;
            let request_context = KnowledgeRequestContext::from_run(&ctx.run)
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            let receipt = self
                .service
                .write(
                    request_context,
                    intent,
                    MemoryMutationOrigin::ExplicitTool {
                        tool_use_id: ctx.tool_use_id.clone(),
                    },
                    ctx.abort.clone(),
                )
                .await
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            let entry_id = receipt
                .reference
                .item_id
                .strip_prefix("entry/")
                .ok_or_else(|| tool_execution_failure("Tutor Memory returned an invalid item"))?;
            let entry = self
                .store
                .get(&self.tutor_id, entry_id)
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            let content = vec![DataBlock::text(format!(
                "Saved private tutor memory: {}",
                entry.text
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({ "tutor_id": self.tutor_id, "entry": entry, "receipt": receipt }),
                false,
            ))
        })
    }
}

impl Tool for ResolveTutorMemoryTool {
    fn name(&self) -> &str {
        "resolve_tutor_memory"
    }

    fn description(&self) -> &str {
        "Close one active private memory item belonging to this tutor after its commitment, follow-up, or plan has been completed. The tool cannot access another tutor's memory."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        RESOLVE_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "required": ["entry_id"],
                "properties": {
                    "entry_id": { "type": "string" },
                    "resolution_note": { "type": "string" }
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
            let entry_id = required_string(&args, "entry_id")?;
            let entry = self
                .store
                .resolve(
                    &self.tutor_id,
                    &entry_id,
                    optional_string(&args, "resolution_note"),
                )
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            debug_assert_eq!(entry.status, TutorMemoryStatus::Resolved);
            let content = vec![DataBlock::text(format!(
                "Closed private tutor memory: {}",
                entry.text
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({ "tutor_id": self.tutor_id, "entry": entry }),
                false,
            ))
        })
    }
}

fn required_kind(args: &serde_json::Value, key: &str) -> Result<TutorMemoryKind, ToolFailure> {
    optional_kind(args, key)?
        .ok_or_else(|| ToolFailure::invalid_arguments(format!("{key} is required")))
}

fn optional_kind(
    args: &serde_json::Value,
    key: &str,
) -> Result<Option<TutorMemoryKind>, ToolFailure> {
    let Some(value) = args[key].as_str() else {
        return Ok(None);
    };
    let kind = match value.trim() {
        "commitment" => TutorMemoryKind::Commitment,
        "open_loop" => TutorMemoryKind::OpenLoop,
        "lesson_plan" => TutorMemoryKind::LessonPlan,
        "reflection" => TutorMemoryKind::Reflection,
        "strategy" => TutorMemoryKind::Strategy,
        other => {
            return Err(ToolFailure::invalid_arguments(format!(
                "unsupported tutor memory kind `{other}`"
            )));
        }
    };
    Ok(Some(kind))
}

fn required_string(args: &serde_json::Value, key: &str) -> Result<String, ToolFailure> {
    optional_string(args, key)
        .ok_or_else(|| ToolFailure::invalid_arguments(format!("{key} is required")))
}

fn optional_string(args: &serde_json::Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn tool_execution_failure(message: impl Into<String>) -> ToolFailure {
    ToolFailure::new("tutor_memory_failed", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use llm_harness_types::UnsupportedEnv;
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;

    fn make_ctx() -> ToolContext {
        let (tx, _rx) = mpsc::channel(1);
        ToolContext {
            env: Arc::new(UnsupportedEnv::new()),
            run: Arc::new(llm_harness_types::RunContext::new(
                llm_harness_types::RunRequest::default(),
            )),
            abort: CancellationToken::new(),
            tool_use_id: "test-id".into(),
            turn_index: 0,
            assistant_message: Arc::new(llm_harness_types::AssistantMessage {
                kind: llm_harness_types::AssistantMessageKind::FinalAnswer,
                message_id: "message-1".into(),
                turn_id: "turn-1".into(),
                content: vec![],
                usage: None,
                stop_reason: None,
                timestamp: Utc::now(),
                provider: None,
                api: None,
                model: None,
                error_message: None,
            }),
            update_tx: tx,
        }
    }

    #[tokio::test]
    async fn resolve_tool_is_hard_bound_to_one_tutor() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(TutorMemoryStore::new_with_root(dir.path()));
        let created = store
            .create(
                "tutor-a",
                crate::tutor_memory_store::CreateTutorMemoryEntry {
                    kind: TutorMemoryKind::OpenLoop,
                    text: "Continue the attention exercise".into(),
                    next_action: Some("Review question 3".into()),
                    due_at: None,
                    source_session_id: Some("session-a".into()),
                    source_message_id: None,
                },
            )
            .unwrap();

        let resolve_other = ResolveTutorMemoryTool::new(store.clone(), "tutor-b")
            .execute(json!({ "entry_id": created.id }), &make_ctx())
            .await;
        assert!(resolve_other.is_err());

        ResolveTutorMemoryTool::new(store.clone(), "tutor-a")
            .execute(json!({ "entry_id": created.id }), &make_ctx())
            .await
            .unwrap();
        assert!(store.list("tutor-a", false).unwrap().is_empty());
    }
}
