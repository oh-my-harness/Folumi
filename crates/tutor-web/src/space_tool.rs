#![allow(clippy::result_large_err)] // Tool helpers must return the runtime-owned ToolFailure value.

use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_types::{DataBlock, Tool, ToolContext, ToolFailure, ToolResult};
use serde_json::json;

use crate::notebook_store::{NotebookEntrySummary, NotebookStore};
use crate::quiz_store::QuizStore;
use crate::routes::space::{SpaceMention, SpaceMentionType, resolve_space_mention_markdown};

static SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

pub struct ReadSpaceItemTool {
    notebook: Arc<NotebookStore>,
    quizzes: Arc<QuizStore>,
}

pub struct ProposeNotebookEditTool {
    notebook: Arc<NotebookStore>,
}

pub struct ListNotebookTreeTool {
    notebook: Arc<NotebookStore>,
}

pub struct SearchNotebookTool {
    notebook: Arc<NotebookStore>,
}

impl ReadSpaceItemTool {
    pub fn new(notebook: Arc<NotebookStore>, quizzes: Arc<QuizStore>) -> Self {
        Self { notebook, quizzes }
    }
}

impl ProposeNotebookEditTool {
    pub fn new(notebook: Arc<NotebookStore>) -> Self {
        Self { notebook }
    }
}

impl ListNotebookTreeTool {
    pub fn new(notebook: Arc<NotebookStore>) -> Self {
        Self { notebook }
    }
}

impl SearchNotebookTool {
    pub fn new(notebook: Arc<NotebookStore>) -> Self {
        Self { notebook }
    }
}

impl Tool for ReadSpaceItemTool {
    fn name(&self) -> &str {
        "read_space_item"
    }

    fn description(&self) -> &str {
        "Read a user-mentioned Space artifact, such as a Notebook entry, Quiz session, or Quiz question. Use this when the user references @Space content and you need its exact content."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "item_type": {
                        "type": "string",
                        "enum": ["notebook_entry", "quiz_session", "quiz_question"],
                        "description": "Type of Space artifact to read."
                    },
                    "target_id": {
                        "type": "string",
                        "description": "Notebook entry id or Quiz session id."
                    },
                    "question_id": {
                        "type": "string",
                        "description": "Required when item_type is quiz_question."
                    },
                    "mention_id": {
                        "type": "string",
                        "description": "Optional full mention id, e.g. notebook_entry:<id> or quiz_question:<quiz_id>:<question_id>."
                    }
                },
                "required": ["item_type", "target_id"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let mention = mention_from_args(args)?;
            let Some((resolved_id, markdown)) =
                resolve_space_mention_markdown(&self.notebook, &self.quizzes, &mention)
            else {
                let content = vec![DataBlock::text(
                    "Space item not found. Ask the user to choose the item again from the Space picker.",
                )];
                return Ok(ToolResult::projected(
                    content.clone(),
                    content,
                    json!({
                        "found": false,
                        "requested": mention,
                    }),
                    false,
                ));
            };

            Ok(ToolResult::ephemeral(
                vec![DataBlock::text(markdown.clone())],
                format!("Read Space item `{resolved_id}`."),
                json!({
                    "found": true,
                    "id": resolved_id,
                    "item_type": mention.mention_type,
                    "target_id": mention.target_id,
                    "question_id": mention.question_id,
                    "title": mention.title,
                    "markdown": markdown,
                }),
                false,
            ))
        })
    }
}

static SEARCH_NOTEBOOK_SCHEMA: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();

impl Tool for SearchNotebookTool {
    fn name(&self) -> &str {
        "search_notebook"
    }

    fn description(&self) -> &str {
        "Search the user's associated Notebook/Vault as plain Markdown text. Use this only when Notebook is associated. This searches paths, titles, tags, links, metadata, and Markdown. This does not use embeddings or RAG."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        SEARCH_NOTEBOOK_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Plain-text query to search in Notebook paths, titles, tags, links, metadata, and Markdown."
                    },
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 10,
                        "description": "Maximum number of Notebook entries to return."
                    }
                },
                "required": ["query"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let query = args["query"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ToolFailure::invalid_arguments("query is required"))?;
            let limit = args["limit"]
                .as_u64()
                .map(|value| value.clamp(1, 10) as usize)
                .unwrap_or(5);
            let summaries = self.notebook.list_summaries(None);
            let hits = search_notebook_entries(&summaries, query, limit);
            let text = if hits.is_empty() {
                format!("No Notebook entries matched query: {query}")
            } else {
                hits.iter()
                    .enumerate()
                    .map(|(index, hit)| {
                        format!(
                            "{}. {} ({})\nID: {}\nPath: {}\nTags: {}\nLinks: {}\nBacklinks: {}\nScore: {}\nSnippet: {}",
                            index + 1,
                            hit.title,
                            hit.entry_type,
                            hit.id,
                            hit.path.as_deref().unwrap_or(""),
                            if hit.tags.is_empty() {
                                "none".into()
                            } else {
                                hit.tags.join(", ")
                            },
                            hit.links.len(),
                            hit.backlinks.len(),
                            hit.score,
                            hit.snippet
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n\n")
            };

            let content = vec![DataBlock::text(text)];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "query": query,
                    "hits": hits,
                }),
                false,
            ))
        })
    }
}

static LIST_NOTEBOOK_TREE_SCHEMA: std::sync::OnceLock<serde_json::Value> =
    std::sync::OnceLock::new();

impl Tool for ListNotebookTreeTool {
    fn name(&self) -> &str {
        "list_notebook_tree"
    }

    fn description(&self) -> &str {
        "List the associated Notebook/Vault folder tree without reading full note bodies. Use this only when Notebook is associated and you need to explore available notes or folders."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        LIST_NOTEBOOK_TREE_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 200,
                        "description": "Maximum number of note entries to return."
                    }
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
            let limit = args["limit"]
                .as_u64()
                .map(|value| value.clamp(1, 200) as usize)
                .unwrap_or(100);
            let folders = self.notebook.list_folders();
            let entries = self
                .notebook
                .list_summaries(None)
                .into_iter()
                .take(limit)
                .map(|summary| {
                    json!({
                        "id": summary.entry.id,
                        "title": summary.entry.title,
                        "path": summary.entry.path,
                        "entry_type": format!("{:?}", summary.entry.entry_type).to_ascii_lowercase(),
                        "tags": summary.tags,
                        "links_count": summary.links.len(),
                        "backlinks_count": summary.backlinks.len(),
                        "updated_at": summary.entry.updated_at,
                    })
                })
                .collect::<Vec<_>>();
            let text = if entries.is_empty() && folders.is_empty() {
                "Notebook/Vault is empty.".to_string()
            } else {
                let folder_text = if folders.is_empty() {
                    "Folders: none".to_string()
                } else {
                    format!(
                        "Folders:\n{}",
                        folders
                            .iter()
                            .map(|folder| format!("- {folder}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    )
                };
                let entry_text = if entries.is_empty() {
                    "Notes: none".to_string()
                } else {
                    let lines = entries
                        .iter()
                        .enumerate()
                        .map(|(index, entry)| {
                            format!(
                                "{}. {} | {} | {}",
                                index + 1,
                                entry["path"].as_str().unwrap_or(""),
                                entry["title"].as_str().unwrap_or("Untitled"),
                                entry["id"].as_str().unwrap_or("")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    format!("Notes:\n{lines}")
                };
                format!("{folder_text}\n\n{entry_text}")
            };

            let content = vec![DataBlock::text(text)];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "folders": folders,
                    "entries": entries,
                }),
                false,
            ))
        })
    }
}

static PROPOSE_NOTEBOOK_EDIT_SCHEMA: std::sync::OnceLock<serde_json::Value> =
    std::sync::OnceLock::new();

impl Tool for ProposeNotebookEditTool {
    fn name(&self) -> &str {
        "propose_notebook_edit"
    }

    fn description(&self) -> &str {
        "Create a preview-only Notebook edit proposal for a user-mentioned Notebook entry. This tool never writes data. The user must explicitly confirm the proposal before the product updates the Notebook entry."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        PROPOSE_NOTEBOOK_EDIT_SCHEMA.get_or_init(|| {
            json!({
                "type": "object",
                "properties": {
                    "entry_id": {
                        "type": "string",
                        "description": "Notebook entry id to revise."
                    },
                    "proposed_title": {
                        "type": "string",
                        "description": "Optional replacement title. Omit to keep the current title."
                    },
                    "proposed_markdown": {
                        "type": "string",
                        "description": "Complete replacement Markdown for the Notebook entry."
                    },
                    "summary": {
                        "type": "string",
                        "description": "Short user-facing summary of the proposed change."
                    },
                    "proposal_kind": {
                        "type": "string",
                        "enum": ["edit", "links", "tags", "merge"],
                        "description": "Organization proposal type. Use links for wiki-link suggestions, tags for tag cleanup, merge for duplicate-note consolidation, and edit for general rewrites."
                    },
                    "suggested_links": {
                        "type": "array",
                        "description": "Optional wiki-link suggestions included in this replacement.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "text": { "type": "string" },
                                "target": { "type": "string" },
                                "reason": { "type": "string" }
                            },
                            "required": ["text", "target"]
                        }
                    },
                    "suggested_tags": {
                        "type": "array",
                        "description": "Optional tags to add, keep, or remove.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "tag": { "type": "string" },
                                "action": { "type": "string", "enum": ["add", "keep", "remove"] },
                                "reason": { "type": "string" }
                            },
                            "required": ["tag", "action"]
                        }
                    },
                    "merge_source_entry_ids": {
                        "type": "array",
                        "description": "For merge proposals, duplicate/source Notebook entry ids that were considered. Applying this proposal updates only entry_id; it does not delete these source entries.",
                        "items": { "type": "string" }
                    }
                },
                "required": ["entry_id", "proposed_markdown", "summary"]
            })
        })
    }

    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        _ctx: &'a ToolContext,
    ) -> BoxFuture<'a, Result<ToolResult, ToolFailure>> {
        Box::pin(async move {
            let entry_id = args["entry_id"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ToolFailure::invalid_arguments("entry_id is required"))?;
            let Some(entry) = self.notebook.get(entry_id) else {
                let content = vec![DataBlock::text(
                    "Notebook entry not found. Ask the user to choose the entry again from the Space picker.",
                )];
                return Ok(ToolResult::projected(
                    content.clone(),
                    content,
                    json!({
                        "found": false,
                        "entry_id": entry_id,
                    }),
                    false,
                ));
            };
            let proposed_markdown = args["proposed_markdown"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| ToolFailure::invalid_arguments("proposed_markdown is required"))?;
            let proposed_title = args["proposed_title"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or(&entry.title);
            let summary = args["summary"]
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("Proposed Notebook update");
            let proposal_kind = args["proposal_kind"]
                .as_str()
                .map(str::trim)
                .filter(|value| matches!(*value, "edit" | "links" | "tags" | "merge"))
                .unwrap_or("edit");
            let suggested_links = args["suggested_links"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let suggested_tags = args["suggested_tags"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let merge_source_entry_ids = args["merge_source_entry_ids"]
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
                "Notebook {proposal_kind} proposal is ready for user review. It has not been applied. Entry: {}.",
                entry.title
            ))];
            Ok(ToolResult::projected(
                content.clone(),
                content,
                json!({
                    "found": true,
                    "proposal_kind": proposal_kind,
                    "entry_id": entry.id,
                    "entry_title": entry.title,
                    "current_markdown": entry.markdown,
                    "proposed_title": proposed_title,
                    "proposed_markdown": proposed_markdown,
                    "summary": summary,
                    "suggested_links": suggested_links,
                    "suggested_tags": suggested_tags,
                    "merge_source_entry_ids": merge_source_entry_ids,
                    "requires_confirmation": true,
                }),
                false,
            ))
        })
    }
}

#[derive(Debug, serde::Serialize)]
struct NotebookSearchHit {
    id: String,
    title: String,
    path: Option<String>,
    entry_type: String,
    tags: Vec<String>,
    links: Vec<String>,
    backlinks: Vec<String>,
    snippet: String,
    score: usize,
}

fn search_notebook_entries(
    entries: &[NotebookEntrySummary],
    query: &str,
    limit: usize,
) -> Vec<NotebookSearchHit> {
    let terms = query
        .split_whitespace()
        .map(|term| term.trim().to_ascii_lowercase())
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return vec![];
    }

    let mut hits = entries
        .iter()
        .filter_map(|summary| {
            let entry = &summary.entry;
            let metadata = entry
                .metadata
                .as_ref()
                .map(|value| value.to_string())
                .unwrap_or_default();
            let links = summary
                .links
                .iter()
                .map(|link| {
                    link.alias
                        .as_ref()
                        .map(|alias| format!("{} {}", link.target, alias))
                        .unwrap_or_else(|| link.target.clone())
                })
                .collect::<Vec<_>>();
            let backlinks = summary
                .backlinks
                .iter()
                .map(|backlink| backlink.source_title.clone())
                .collect::<Vec<_>>();
            let haystack = format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n{}",
                entry.path.as_deref().unwrap_or_default(),
                entry.title,
                summary.tags.join(" "),
                links.join(" "),
                backlinks.join(" "),
                metadata,
                entry.markdown
            )
            .to_ascii_lowercase();
            let score = terms
                .iter()
                .map(|term| haystack.matches(term).count())
                .sum::<usize>();
            if score == 0 {
                return None;
            }
            Some(NotebookSearchHit {
                id: entry.id.clone(),
                title: entry.title.clone(),
                path: entry.path.clone(),
                entry_type: format!("{:?}", entry.entry_type).to_ascii_lowercase(),
                tags: summary.tags.clone(),
                links,
                backlinks,
                snippet: notebook_snippet(&entry.markdown, &terms),
                score,
            })
        })
        .collect::<Vec<_>>();

    hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.title.cmp(&b.title)));
    hits.truncate(limit);
    hits
}

fn notebook_snippet(markdown: &str, terms: &[String]) -> String {
    let normalized = markdown.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    let start = terms
        .iter()
        .filter_map(|term| lower.find(term))
        .min()
        .unwrap_or(0);
    let start = normalized
        .char_indices()
        .map(|(index, _)| index)
        .take_while(|index| *index <= start)
        .last()
        .unwrap_or(0)
        .saturating_sub(80);
    let snippet = normalized.chars().skip(start).take(240).collect::<String>();
    if normalized.chars().count() > snippet.chars().count() {
        format!("{snippet}...")
    } else {
        snippet
    }
}

fn mention_from_args(args: serde_json::Value) -> Result<SpaceMention, ToolFailure> {
    let item_type = args["item_type"]
        .as_str()
        .or_else(|| args["type"].as_str())
        .ok_or_else(|| ToolFailure::invalid_arguments("item_type is required"))?;
    let mention_type = match item_type {
        "notebook_entry" => SpaceMentionType::NotebookEntry,
        "quiz_session" => SpaceMentionType::QuizSession,
        "quiz_question" => SpaceMentionType::QuizQuestion,
        other => {
            return Err(ToolFailure::invalid_arguments(format!(
                "unsupported space item type `{other}`"
            )));
        }
    };
    let target_id = args["target_id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string());
    let question_id = args["question_id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string());
    let mention_id = args["mention_id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().to_string())
        .or_else(|| {
            let target_id = target_id.as_deref()?;
            Some(match mention_type {
                SpaceMentionType::NotebookEntry => format!("notebook_entry:{target_id}"),
                SpaceMentionType::QuizSession => format!("quiz_session:{target_id}"),
                SpaceMentionType::QuizQuestion => {
                    format!(
                        "quiz_question:{}:{}",
                        target_id,
                        question_id.as_deref().unwrap_or_default()
                    )
                }
            })
        })
        .ok_or_else(|| ToolFailure::invalid_arguments("target_id is required"))?;

    Ok(SpaceMention {
        id: mention_id,
        mention_type,
        target_id,
        question_id,
        title: args["title"]
            .as_str()
            .unwrap_or("Space item")
            .trim()
            .to_string(),
        preview: None,
        metadata: json!({}),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook_store::{NotebookEntryInput, NotebookEntryType};
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
                message_id: "test-message".into(),
                turn_id: "test-turn".into(),
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
    async fn read_space_item_returns_notebook_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let quizzes = Arc::new(QuizStore::new_with_path(dir.path().join("quizzes.json")));
        let entry = notebook
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: None,
                title: "Mask notes".into(),
                markdown: "Alignment marks matter.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let tool = ReadSpaceItemTool::new(notebook, quizzes);

        let result = tool
            .execute(
                json!({
                    "item_type": "notebook_entry",
                    "target_id": entry.id,
                }),
                &make_ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result.details["found"], true);
        match &result.model_content[0] {
            DataBlock::Text { text, .. } => assert!(text.contains("Alignment marks")),
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn propose_notebook_edit_returns_preview_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let entry = notebook
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: None,
                title: "Mask notes".into(),
                markdown: "Original notes.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let tool = ProposeNotebookEditTool::new(notebook.clone());

        let result = tool
            .execute(
                json!({
                    "entry_id": entry.id,
                    "proposed_title": "Updated mask notes",
                    "proposed_markdown": "# Updated\n\nBetter notes.",
                    "summary": "Rewrite as structured notes.",
                    "proposal_kind": "links",
                    "suggested_links": [
                        { "text": "mask alignment", "target": "Mask Alignment", "reason": "Connect related concept" }
                    ],
                    "suggested_tags": [
                        { "tag": "semiconductor", "action": "add", "reason": "Topic tag" }
                    ]
                }),
                &make_ctx(),
            )
            .await
            .unwrap();

        assert_eq!(result.details["requires_confirmation"], true);
        assert_eq!(result.details["proposal_kind"], "links");
        assert_eq!(
            result.details["suggested_links"][0]["target"],
            "Mask Alignment"
        );
        assert_eq!(result.details["suggested_tags"][0]["tag"], "semiconductor");
        assert_eq!(result.details["proposed_title"], "Updated mask notes");
        assert_eq!(notebook.get(&entry.id).unwrap().markdown, "Original notes.");
    }

    #[tokio::test]
    async fn search_notebook_returns_plain_text_hits() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let entry = notebook
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: Some("concepts/Lithography.md".into()),
                title: "Lithography notes".into(),
                markdown: "OPC and mask alignment. #semiconductor\n\nSee [[Mask Alignment]]."
                    .into(),
                metadata: Some(json!({
                    "aliases": ["optical lithography"]
                })),
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let tool = SearchNotebookTool::new(notebook);

        let result = tool
            .execute(json!({ "query": "mask", "limit": 3 }), &make_ctx())
            .await
            .unwrap();

        assert_eq!(result.details["hits"][0]["id"], entry.id);
        assert_eq!(result.details["hits"][0]["path"], "concepts/Lithography.md");
        match &result.model_content[0] {
            DataBlock::Text { text, .. } => {
                assert!(text.contains("concepts/Lithography.md"));
                assert!(text.contains("mask alignment"));
            }
            _ => panic!("expected text content"),
        }
    }

    #[tokio::test]
    async fn list_notebook_tree_returns_folders_and_entry_paths() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        notebook.create_folder("concepts/lithography").unwrap();
        let entry = notebook
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: Some("concepts/lithography/TCC.md".into()),
                title: "TCC".into(),
                markdown: "Transmission cross coefficient.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let tool = ListNotebookTreeTool::new(notebook);

        let result = tool
            .execute(json!({ "limit": 20 }), &make_ctx())
            .await
            .unwrap();

        assert!(
            result.details["folders"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item == "concepts/lithography")
        );
        assert_eq!(result.details["entries"][0]["id"], entry.id);
        assert_eq!(
            result.details["entries"][0]["path"],
            "concepts/lithography/TCC.md"
        );
        match &result.model_content[0] {
            DataBlock::Text { text, .. } => {
                assert!(text.contains("concepts/lithography/TCC.md"))
            }
            _ => panic!("expected text content"),
        }
    }
}
