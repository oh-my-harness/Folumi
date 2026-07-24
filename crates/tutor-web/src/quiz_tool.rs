#![allow(clippy::result_large_err)] // Tool helpers must return the runtime-owned ToolFailure value.

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::KnowledgeAccessContext;
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde_json::json;

use crate::quiz_store::QuizDifficulty;
use crate::routes::quiz::{CreateLlmConfig, CreateQuizRequest, QuizState, create_quiz_for_request};

static CREATE_QUIZ_SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
static PROPOSE_QUIZ_PLAN_SCHEMA: std::sync::OnceLock<serde_json::Value> =
    std::sync::OnceLock::new();

pub(crate) struct CreateQuizTool {
    state: QuizState,
    default_kb_id: Option<String>,
    llm: Option<CreateLlmConfig>,
    allowed_kb_ids: Option<Vec<String>>,
    notebook_allowed: bool,
}

impl CreateQuizTool {
    pub(crate) fn new(
        state: QuizState,
        default_kb_id: Option<String>,
        llm: Option<CreateLlmConfig>,
    ) -> Self {
        Self {
            state,
            default_kb_id,
            llm,
            allowed_kb_ids: None,
            notebook_allowed: true,
        }
    }

    pub(crate) fn with_resource_policy(
        mut self,
        allowed_kb_ids: Vec<String>,
        notebook_allowed: bool,
    ) -> Self {
        self.allowed_kb_ids = Some(allowed_kb_ids);
        self.notebook_allowed = notebook_allowed;
        self
    }
}

pub(crate) struct ProposeQuizPlanTool;

impl Tool for ProposeQuizPlanTool {
    fn name(&self) -> &str {
        "propose_quiz_plan"
    }

    fn description(&self) -> &str {
        "Show a quiz generation plan for user confirmation. Use this before create_quiz when the user wants to discuss scope, source, difficulty, or question count. This tool does not create a quiz."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        PROPOSE_QUIZ_PLAN_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Proposed quiz title." },
                    "topic": { "type": "string", "description": "Main topic or learning objective." },
                    "source": { "type": "string", "description": "Planned source material, such as conversation, attachment, Notebook note, Space item, or knowledge base." },
                    "difficulty": { "type": "string", "enum": ["easy", "medium", "hard"], "description": "Planned difficulty." },
                    "question_count": { "type": "integer", "minimum": 1, "maximum": 10, "description": "Planned number of questions." },
                    "notes": { "type": "array", "items": { "type": "string" }, "description": "Important constraints or confirmation questions." }
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
            let title = optional_string(&args, "title").unwrap_or_else(|| "Quiz plan".into());
            let topic =
                optional_string(&args, "topic").unwrap_or_else(|| "selected material".into());
            let source =
                optional_string(&args, "source").unwrap_or_else(|| "current conversation".into());
            let difficulty =
                optional_string(&args, "difficulty").unwrap_or_else(|| "medium".into());
            let question_count = args["question_count"].as_u64().unwrap_or(5).clamp(1, 10);
            let notes = args["notes"]
                .as_array()
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| item.as_str())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let content = vec![DataBlock::text(format!(
                "Proposed quiz plan: {title}, topic {topic}, {question_count} {difficulty} questions from {source}. Ask the user to confirm before creating it."
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "title": title,
                    "topic": topic,
                    "source": source,
                    "difficulty": difficulty,
                    "question_count": question_count,
                    "notes": notes,
                }),
                false,
            ))
        })
    }
}

impl Tool for CreateQuizTool {
    fn name(&self) -> &str {
        "create_quiz"
    }

    fn description(&self) -> &str {
        "Create an interactive Quiz only after the user explicitly asks to generate one or confirms a quiz plan. Use source_text for conversation, attachments, or read_space_item results; use kb_id for a knowledge base; use notebook_entry_id for a specific note. Do not call this just to discuss quiz scope."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        CREATE_QUIZ_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Optional quiz title."
                    },
                    "topic": {
                        "type": "string",
                        "description": "Quiz focus or learning objective."
                    },
                    "difficulty": {
                        "type": "string",
                        "enum": ["easy", "medium", "hard"],
                        "description": "Target difficulty."
                    },
                    "question_count": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "Number of questions to generate."
                    },
                    "kb_id": {
                        "type": "string",
                        "description": "Knowledge base id. Omit to use the associated knowledge base if one is selected."
                    },
                    "notebook_entry_id": {
                        "type": "string",
                        "description": "Notebook entry id to use as source material."
                    },
                    "source_text": {
                        "type": "string",
                        "description": "Explicit source material from the conversation, attachments, or Space references. Do not put mere instructions here."
                    },
                    "source_label": {
                        "type": "string",
                        "description": "Human-readable label for source_text."
                    }
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
            let requested_kb =
                optional_string(&args, "kb_id").or_else(|| self.default_kb_id.clone());
            let notebook_entry_id = optional_string(&args, "notebook_entry_id");
            validate_quiz_resource_policy(
                self.allowed_kb_ids.as_deref(),
                self.notebook_allowed,
                requested_kb.as_deref(),
                notebook_entry_id.as_deref(),
            )?;
            let request = CreateQuizRequest {
                title: optional_string(&args, "title"),
                kb_id: requested_kb,
                notebook_entry_id,
                source_text: optional_string(&args, "source_text"),
                source_label: optional_string(&args, "source_label"),
                topic: optional_string(&args, "topic"),
                difficulty: optional_string(&args, "difficulty")
                    .as_deref()
                    .map(parse_difficulty)
                    .transpose()?,
                question_count: args["question_count"].as_u64().map(|value| value as usize),
                llm: self.llm.clone(),
                knowledge_access: ctx.run.extension::<KnowledgeAccessContext>().cloned(),
            };
            let state = QuizState {
                store: self.state.store.clone(),
                knowledge: self.state.knowledge.clone(),
                notebook: self.state.notebook.clone(),
                memory: self.state.memory.clone(),
                evidence_authority: self.state.evidence_authority.clone(),
                rag_root: self.state.rag_root.clone(),
                workflow_root: self.state.workflow_root.clone(),
            };
            let quiz = create_quiz_for_request(&state, request)
                .await
                .map_err(|err| tool_execution_failure(err.to_string()))?;
            let content = vec![DataBlock::text(format!(
                "Created Quiz \"{}\" with {} questions. The product UI will render the interactive quiz card.",
                quiz.title,
                quiz.questions.len()
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "quiz_id": quiz.id,
                    "title": quiz.title,
                    "question_count": quiz.questions.len(),
                    "quiz": quiz,
                }),
                false,
            ))
        })
    }
}

fn validate_quiz_resource_policy(
    allowed_kb_ids: Option<&[String]>,
    notebook_allowed: bool,
    requested_kb_id: Option<&str>,
    notebook_entry_id: Option<&str>,
) -> Result<(), ToolFailure> {
    if let (Some(allowed), Some(kb_id)) = (allowed_kb_ids, requested_kb_id)
        && !allowed.iter().any(|id| id == kb_id)
    {
        return Err(tool_execution_failure(
            "Tutor is not allowed to use this Knowledge Base",
        ));
    }
    if notebook_entry_id.is_some() && !notebook_allowed {
        return Err(tool_execution_failure(
            "Tutor is not allowed to use Notebook entries",
        ));
    }
    Ok(())
}

fn optional_string(args: &serde_json::Value, key: &str) -> Option<String> {
    args[key]
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_difficulty(value: &str) -> Result<QuizDifficulty, ToolFailure> {
    match value.trim().to_ascii_lowercase().as_str() {
        "easy" => Ok(QuizDifficulty::Easy),
        "medium" => Ok(QuizDifficulty::Medium),
        "hard" => Ok(QuizDifficulty::Hard),
        other => Err(ToolFailure::invalid_arguments(format!(
            "unsupported difficulty `{other}`"
        ))),
    }
}

fn tool_execution_failure(message: impl Into<String>) -> ToolFailure {
    ToolFailure::new("quiz_tool_failed", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiz_resource_policy_rejects_direct_tool_bypass() {
        let allowed = vec!["approved-kb".to_string()];

        assert!(
            validate_quiz_resource_policy(Some(&allowed), true, Some("other-kb"), None,).is_err()
        );
        assert!(
            validate_quiz_resource_policy(
                Some(&allowed),
                false,
                Some("approved-kb"),
                Some("note-1"),
            )
            .is_err()
        );
        assert!(
            validate_quiz_resource_policy(Some(&allowed), true, Some("approved-kb"), None,).is_ok()
        );
    }
}
