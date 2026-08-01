use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::knowledge_store::KnowledgeStore;
use crate::notebook_store::NotebookStore;

#[derive(Clone)]
struct LibraryState {
    knowledge: Arc<KnowledgeStore>,
    notebook: Arc<NotebookStore>,
}

#[derive(Debug, Deserialize)]
struct LibrarySearchQuery {
    q: String,
    #[serde(rename = "type")]
    item_type: Option<LibraryItemType>,
    limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum LibraryItemType {
    Source,
    Note,
}

#[derive(Debug, Serialize)]
struct LibrarySearchHit {
    id: String,
    #[serde(rename = "type")]
    item_type: LibraryItemType,
    title: String,
    snippet: String,
    location: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    knowledge_base_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    note_id: Option<String>,
}

pub fn library_router(knowledge: Arc<KnowledgeStore>, notebook: Arc<NotebookStore>) -> Router {
    Router::new()
        .route("/api/library/search", get(search_library))
        .with_state(LibraryState {
            knowledge,
            notebook,
        })
}

async fn search_library(
    State(state): State<LibraryState>,
    Query(query): Query<LibrarySearchQuery>,
) -> impl IntoResponse {
    let needle = query.q.trim();
    if needle.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "query is empty" })),
        )
            .into_response();
    }

    let limit = query.limit.unwrap_or(30).clamp(1, 100);
    let mut hits = Vec::new();
    let mut warnings = Vec::new();

    if query.item_type.is_none() || query.item_type == Some(LibraryItemType::Source) {
        'knowledge_bases: for kb in state.knowledge.list() {
            for document in kb.documents {
                let text = match state.knowledge.document_text(&kb.id, &document.id) {
                    Ok(Some(text)) => text,
                    Ok(None) => {
                        warnings.push(format!("Stored content is missing for {}", document.name));
                        continue;
                    }
                    Err(error) => {
                        warnings.push(format!("Could not read {}: {error}", document.name));
                        continue;
                    }
                };
                if let Some(found) = find_text_match(&text, needle) {
                    hits.push(LibrarySearchHit {
                        id: format!("source:{}:{}", kb.id, document.id),
                        item_type: LibraryItemType::Source,
                        title: document.name,
                        snippet: found.snippet,
                        location: format!("{} · line {}", kb.name, found.line),
                        knowledge_base_id: Some(kb.id.clone()),
                        document_id: Some(document.id),
                        note_id: None,
                    });
                    if hits.len() >= limit {
                        break 'knowledge_bases;
                    }
                }
            }
        }
    }

    if hits.len() < limit
        && (query.item_type.is_none() || query.item_type == Some(LibraryItemType::Note))
    {
        for note in state.notebook.list(Some("default")) {
            let searchable = format!("{}\n{}", note.title, note.markdown);
            if let Some(found) = find_text_match(&searchable, needle) {
                hits.push(LibrarySearchHit {
                    id: format!("note:{}", note.id),
                    item_type: LibraryItemType::Note,
                    title: note.title,
                    snippet: found.snippet,
                    location: note.path.unwrap_or_else(|| format!("line {}", found.line)),
                    knowledge_base_id: None,
                    document_id: None,
                    note_id: Some(note.id),
                });
                if hits.len() >= limit {
                    break;
                }
            }
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "query": needle,
            "hits": hits,
            "warnings": warnings,
        })),
    )
        .into_response()
}

struct TextMatch {
    snippet: String,
    line: usize,
}

fn find_text_match(text: &str, query: &str) -> Option<TextMatch> {
    let lower_text = text.to_lowercase();
    let terms = query
        .split_whitespace()
        .map(str::to_lowercase)
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() || terms.iter().any(|term| !lower_text.contains(term)) {
        return None;
    }
    let first_term = &terms[0];
    text.lines().enumerate().find_map(|(index, line)| {
        line.to_lowercase().contains(first_term).then(|| {
            let length = line.chars().count();
            let body = line.chars().take(240).collect::<String>();
            TextMatch {
                snippet: format!("{}{}", body.trim(), if length > 240 { "…" } else { "" }),
                line: index + 1,
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_is_case_insensitive_and_reports_source_line() {
        let found = find_text_match("First line\nKnowledge Boundary\nLast line", "knowledge")
            .expect("query should match");
        assert_eq!(found.line, 2);
        assert!(found.snippet.contains("Knowledge Boundary"));
    }

    #[test]
    fn missing_query_returns_no_match() {
        assert!(find_text_match("personal notes", "citations").is_none());
    }

    #[test]
    fn multiple_terms_can_match_across_lines() {
        let found = find_text_match(
            "Runtime architecture\nTrusted citations",
            "runtime citations",
        )
        .expect("all terms exist in the document");
        assert_eq!(found.line, 1);
    }
}
