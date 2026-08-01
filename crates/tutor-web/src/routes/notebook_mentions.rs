use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::notebook_store::{NotebookEntry, NotebookStore};

#[derive(Clone)]
struct NotebookMentionState {
    notebook: Arc<NotebookStore>,
}

#[derive(Debug, Deserialize)]
struct MentionQuery {
    q: Option<String>,
    limit: Option<usize>,
    space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotebookMention {
    pub id: String,
    #[serde(rename = "type")]
    pub mention_type: String,
    pub target_id: Option<String>,
    pub title: String,
    pub preview: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

async fn list_mentions(
    State(state): State<NotebookMentionState>,
    Query(query): Query<MentionQuery>,
) -> impl IntoResponse {
    let query_text = query.q.as_deref().unwrap_or_default().trim();
    let limit = query.limit.unwrap_or(20).clamp(1, 50);
    let space_id = query
        .space_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mentions = state
        .notebook
        .list(space_id)
        .into_iter()
        .filter(|entry| {
            matches_query(
                query_text,
                &[&entry.title, entry_type_label(entry), &entry.markdown],
            )
        })
        .take(limit)
        .map(|entry| mention_for_entry(&entry))
        .collect::<Vec<_>>();
    (
        StatusCode::OK,
        Json(serde_json::json!({ "mentions": mentions })),
    )
        .into_response()
}

pub fn resolve_notebook_mention(
    notebook: &NotebookStore,
    mention: &NotebookMention,
) -> Option<(String, String)> {
    if mention.mention_type != "notebook_entry" {
        return None;
    }
    let target_id = mention
        .target_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| mention.id.strip_prefix("notebook_entry:"))?;
    let entry = notebook.get(target_id)?;
    Some((mention_for_entry(&entry).id, notebook_markdown(&entry)))
}

pub fn notebook_mentions_router(notebook: Arc<NotebookStore>) -> Router {
    Router::new()
        .route("/api/notebook/mentions", get(list_mentions))
        .with_state(NotebookMentionState { notebook })
}

fn mention_for_entry(entry: &NotebookEntry) -> NotebookMention {
    NotebookMention {
        id: format!("notebook_entry:{}", entry.id),
        mention_type: "notebook_entry".into(),
        target_id: Some(entry.id.clone()),
        title: entry.title.clone(),
        preview: first_text_line(&entry.markdown),
        metadata: serde_json::json!({
            "entry_type": entry.entry_type,
            "path": entry.path,
            "space_id": entry.space_id,
            "updated_at": entry.updated_at,
            "source_session_id": entry.source_session_id,
            "source_message_id": entry.source_message_id,
        }),
    }
}

fn notebook_markdown(entry: &NotebookEntry) -> String {
    let path = entry.path.as_deref().unwrap_or("");
    let path_line = if path.is_empty() {
        String::new()
    } else {
        format!("Path: {path}\n")
    };
    format!(
        "# {}\n\nType: {}\n{}ID: {}\n\n{}",
        entry.title,
        entry_type_label(entry),
        path_line,
        entry.id,
        entry.markdown
    )
}

fn matches_query(query: &str, values: &[&str]) -> bool {
    if query.is_empty() {
        return true;
    }
    let query = query.to_lowercase();
    values
        .iter()
        .any(|value| value.to_lowercase().contains(&query))
}

fn first_text_line(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.chars().take(160).collect())
}

fn entry_type_label(entry: &NotebookEntry) -> &'static str {
    match entry.entry_type {
        crate::notebook_store::NotebookEntryType::ResearchReport => "research_report",
        crate::notebook_store::NotebookEntryType::Note => "note",
        crate::notebook_store::NotebookEntryType::ChatAnswer => "chat_answer",
        crate::notebook_store::NotebookEntryType::SourceSnippet => "source_snippet",
        crate::notebook_store::NotebookEntryType::QuizSummary => "quiz_summary",
        crate::notebook_store::NotebookEntryType::DeepSolveResult => "deep_solve_result",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook_store::{NotebookEntryInput, NotebookEntryType};
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn lists_only_matching_notebook_mentions() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        notebook
            .create(NotebookEntryInput {
                space_id: Some("default".into()),
                entry_type: NotebookEntryType::Note,
                title: "Lithography notes".into(),
                markdown: "Photoresist process".into(),
                path: None,
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let response = notebook_mentions_router(notebook)
            .oneshot(
                Request::get("/api/notebook/mentions?q=photoresist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["mentions"].as_array().unwrap().len(), 1);
        assert_eq!(body["mentions"][0]["type"], "notebook_entry");
    }
}
