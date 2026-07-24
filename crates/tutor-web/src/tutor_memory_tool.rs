#![allow(clippy::result_large_err)] // Tool helpers must return the runtime-owned ToolFailure value.

use std::sync::{Arc, OnceLock};

use futures::future::BoxFuture;
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde_json::json;

use crate::tutor_memory_store::{
    CreateTutorMemoryEntry, TutorMemoryKind, TutorMemoryStatus, TutorMemoryStore,
};

static READ_SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
static REMEMBER_SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();
static RESOLVE_SCHEMA: OnceLock<serde_json::Value> = OnceLock::new();

pub struct ReadTutorMemoryTool {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
}

pub struct RememberForLaterTool {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
    session_id: String,
}

pub struct ResolveTutorMemoryTool {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
}

impl ReadTutorMemoryTool {
    pub fn new(store: Arc<TutorMemoryStore>, tutor_id: impl Into<String>) -> Self {
        Self {
            store,
            tutor_id: tutor_id.into(),
        }
    }
}

impl RememberForLaterTool {
    pub fn new(
        store: Arc<TutorMemoryStore>,
        tutor_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            tutor_id: tutor_id.into(),
            session_id: session_id.into(),
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

impl Tool for ReadTutorMemoryTool {
    fn name(&self) -> &str {
        "read_tutor_memory"
    }

    fn description(&self) -> &str {
        "Read this tutor's private continuity memory: its commitments, unresolved follow-ups, lesson plans, reflections, and teaching strategies. The tool is already bound to the current tutor. Do not use it for general facts about the learner."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        READ_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "include_resolved": { "type": "boolean", "description": "Include closed items. Defaults to false." },
                    "kind": { "type": "string", "enum": ["commitment", "open_loop", "lesson_plan", "reflection", "strategy"] },
                    "query": { "type": "string", "description": "Optional text filter." }
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
            let include_resolved = args["include_resolved"].as_bool().unwrap_or(false);
            let kind = optional_kind(&args, "kind")?;
            let query = args["query"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_lowercase());
            let mut entries = self
                .store
                .list(&self.tutor_id, include_resolved)
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            entries.retain(|entry| kind.is_none_or(|kind| entry.kind == kind));
            if let Some(query) = query {
                entries.retain(|entry| {
                    entry.text.to_lowercase().contains(&query)
                        || entry
                            .next_action
                            .as_deref()
                            .is_some_and(|value| value.to_lowercase().contains(&query))
                });
            }
            let text = if entries.is_empty() {
                "No private continuity memory is recorded for this tutor.".into()
            } else {
                entries
                    .iter()
                    .map(|entry| {
                        format!(
                            "- [{}] {}{} (id: {})",
                            kind_name(entry.kind),
                            entry.text,
                            entry
                                .next_action
                                .as_deref()
                                .map(|action| format!("; next: {action}"))
                                .unwrap_or_default(),
                            entry.id
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Ok(ToolResult::ephemeral(
                vec![DataBlock::text(text)],
                format!("Read {} private tutor memory item(s).", entries.len()),
                json!({ "tutor_id": self.tutor_id, "entries": entries }),
                false,
            ))
        })
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
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let kind = required_kind(&args, "kind")?;
            let text = required_string(&args, "text")?;
            let entry = self
                .store
                .create(
                    &self.tutor_id,
                    CreateTutorMemoryEntry {
                        kind,
                        text,
                        next_action: optional_string(&args, "next_action"),
                        due_at: None,
                        source_session_id: Some(self.session_id.clone()),
                        source_message_id: optional_string(&args, "source_message_id"),
                    },
                )
                .map_err(|error| tool_execution_failure(error.to_string()))?;
            let content = vec![DataBlock::text(format!(
                "Saved private tutor memory: {}",
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

fn kind_name(kind: TutorMemoryKind) -> &'static str {
    match kind {
        TutorMemoryKind::Commitment => "commitment",
        TutorMemoryKind::OpenLoop => "open_loop",
        TutorMemoryKind::LessonPlan => "lesson_plan",
        TutorMemoryKind::Reflection => "reflection",
        TutorMemoryKind::Strategy => "strategy",
    }
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
    async fn tools_are_hard_bound_to_one_tutor() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(TutorMemoryStore::new_with_root(dir.path()));
        let remember = RememberForLaterTool::new(store.clone(), "tutor-a", "session-a");
        let created = remember
            .execute(
                json!({
                    "kind": "open_loop",
                    "text": "Continue the attention exercise",
                    "next_action": "Review question 3"
                }),
                &make_ctx(),
            )
            .await
            .unwrap();
        let entry_id = created.details["entry"]["id"].as_str().unwrap();

        let read_other = ReadTutorMemoryTool::new(store.clone(), "tutor-b")
            .execute(json!({}), &make_ctx())
            .await
            .unwrap();
        assert!(read_other.details["entries"].as_array().unwrap().is_empty());

        let resolve_other = ResolveTutorMemoryTool::new(store.clone(), "tutor-b")
            .execute(json!({ "entry_id": entry_id }), &make_ctx())
            .await;
        assert!(resolve_other.is_err());

        let own_entries = store.list("tutor-a", false).unwrap();
        assert_eq!(own_entries.len(), 1);
        assert_eq!(
            own_entries[0].source_session_id.as_deref(),
            Some("session-a")
        );
    }
}
