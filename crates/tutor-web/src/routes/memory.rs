use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};

use crate::memory_store::{
    CreateMemoryItem, MemoryItem, MemoryKind, MemoryListFilter, MemoryOrigin, MemorySettings,
    MemoryStatus, MemoryStore, MemoryStoreError, UpdateMemoryItem,
};
use crate::session::SessionPool;

#[derive(Clone)]
struct MemoryState {
    store: Arc<MemoryStore>,
    sessions: Arc<SessionPool>,
}

pub fn memory_router(store: Arc<MemoryStore>, sessions: Arc<SessionPool>) -> Router {
    Router::new()
        .route("/api/memory/items", get(list_items).post(create_item))
        .route(
            "/api/memory/items/{id}",
            patch(update_item).delete(forget_item),
        )
        .route(
            "/api/memory/settings",
            get(get_settings).patch(update_settings),
        )
        .route("/api/memory/export.json", get(export_memory))
        .with_state(MemoryState { store, sessions })
}

#[derive(Debug, Deserialize)]
struct ListItemsQuery {
    kind: Option<MemoryKind>,
    status: Option<MemoryStatus>,
    #[serde(default)]
    include_expired: bool,
    query: Option<String>,
}

#[derive(Debug, Serialize)]
struct ItemsResponse {
    items: Vec<MemoryItem>,
}

async fn list_items(
    State(state): State<MemoryState>,
    Query(query): Query<ListItemsQuery>,
) -> Response {
    match state.store.list(&MemoryListFilter {
        kind: query.kind,
        status: query.status,
        include_expired: query.include_expired,
        query: query.query,
    }) {
        Ok(items) => (StatusCode::OK, Json(ItemsResponse { items })).into_response(),
        Err(error) => error_response(error),
    }
}

async fn create_item(
    State(state): State<MemoryState>,
    Json(mut input): Json<CreateMemoryItem>,
) -> Response {
    input.origin = MemoryOrigin::UserExplicit;
    input.provenance = serde_json::json!({ "origin": "memory_ui" });
    input.idempotency_key = None;
    match state.store.create(input) {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn update_item(
    State(state): State<MemoryState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateMemoryItem>,
) -> Response {
    match state.store.update(&id, input, MemoryOrigin::UserExplicit) {
        Ok(item) => (StatusCode::OK, Json(item)).into_response(),
        Err(error) => error_response(error),
    }
}

#[derive(Debug, Deserialize)]
struct ForgetRequest {
    revision: String,
}

async fn forget_item(
    State(state): State<MemoryState>,
    Path(id): Path<String>,
    Json(input): Json<ForgetRequest>,
) -> Response {
    match state.store.forget(&id, &input.revision) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => error_response(error),
    }
}

async fn get_settings(State(state): State<MemoryState>) -> Response {
    match state.store.settings() {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => error_response(error),
    }
}

#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    enabled: Option<bool>,
    history_recall_enabled: Option<bool>,
    assistant_write_without_approval: Option<bool>,
}

async fn update_settings(
    State(state): State<MemoryState>,
    Json(input): Json<UpdateSettingsRequest>,
) -> Response {
    let current = match state.store.settings() {
        Ok(settings) => settings,
        Err(error) => return error_response(error),
    };
    let enabled = input.enabled.unwrap_or(current.enabled);
    let history_recall_enabled = if enabled {
        input
            .history_recall_enabled
            .unwrap_or(current.history_recall_enabled)
    } else {
        false
    };
    let assistant_write_without_approval = if enabled {
        input
            .assistant_write_without_approval
            .unwrap_or(current.assistant_write_without_approval)
    } else {
        false
    };
    if history_recall_enabled != current.history_recall_enabled
        && let Err(error) = state
            .sessions
            .synchronize_history_recall(history_recall_enabled)
            .await
    {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                code: "history_recall_sync",
                error: format!("failed to update runtime Session recall: {error}"),
                latest: None,
                existing: None,
            }),
        )
            .into_response();
    }
    match state.store.update_settings(
        enabled,
        history_recall_enabled,
        assistant_write_without_approval,
    ) {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => {
            if history_recall_enabled != current.history_recall_enabled {
                let _ = state
                    .sessions
                    .synchronize_history_recall(current.history_recall_enabled)
                    .await;
            }
            error_response(error)
        }
    }
}

#[derive(Debug, Serialize)]
struct MemoryExport {
    schema_version: u32,
    exported_at: chrono::DateTime<chrono::Utc>,
    settings: MemorySettings,
    items: Vec<MemoryItem>,
}

async fn export_memory(State(state): State<MemoryState>) -> Response {
    let settings = match state.store.settings() {
        Ok(settings) => settings,
        Err(error) => return error_response(error),
    };
    let items = match state.store.list(&MemoryListFilter {
        include_expired: true,
        ..Default::default()
    }) {
        Ok(items) => items,
        Err(error) => return error_response(error),
    };
    let payload = MemoryExport {
        schema_version: 1,
        exported_at: chrono::Utc::now(),
        settings,
        items,
    };
    match serde_json::to_vec_pretty(&payload) {
        Ok(body) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
                (
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=folumi-memory-export.json",
                ),
            ],
            body,
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                code: "serialization",
                error: format!("memory export failed: {error}"),
                latest: None,
                existing: None,
            }),
        )
            .into_response(),
    }
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    code: &'static str,
    error: String,
    latest: Option<MemoryItem>,
    existing: Option<MemoryItem>,
}

fn error_response(error: MemoryStoreError) -> Response {
    let (status, code, latest, existing) = match &error {
        MemoryStoreError::Validation(_) => (StatusCode::BAD_REQUEST, "validation", None, None),
        MemoryStoreError::NotFound => (StatusCode::NOT_FOUND, "not_found", None, None),
        MemoryStoreError::Stale { latest } => (
            StatusCode::CONFLICT,
            "stale_revision",
            Some((**latest).clone()),
            None,
        ),
        MemoryStoreError::Conflict { existing } => (
            StatusCode::CONFLICT,
            "memory_conflict",
            None,
            Some((**existing).clone()),
        ),
        MemoryStoreError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "database", None, None)
        }
    };
    (
        status,
        Json(ErrorResponse {
            code,
            error: error.to_string(),
            latest,
            existing,
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;

    fn app() -> (tempfile::TempDir, Router) {
        let directory = tempfile::tempdir().unwrap();
        let store =
            Arc::new(MemoryStore::new_with_path(directory.path().join("memory.sqlite3")).unwrap());
        let sessions = SessionPool::new_with_root(directory.path().join("sessions"));
        (directory, memory_router(store, sessions))
    }

    async fn response_json(response: Response) -> serde_json::Value {
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[tokio::test]
    async fn memory_crud_uses_revision_and_hard_forget() {
        let (_directory, app) = app();
        let create = app
            .clone()
            .oneshot(
                Request::post("/api/memory/items")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "kind": "preference",
                            "content": "请使用简洁的中文回答。",
                            "topic_key": "response_language"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let item = response_json(create).await;
        let id = item["id"].as_str().unwrap();
        let revision = item["revision"].as_str().unwrap();

        let list = app
            .clone()
            .oneshot(
                Request::get("/api/memory/items")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response_json(list).await["items"].as_array().unwrap().len(),
            1
        );

        let forget = app
            .oneshot(
                Request::delete(format!("/api/memory/items/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "revision": revision }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forget.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn export_omits_forgotten_content_and_internal_history() {
        let (_directory, app) = app();
        let forgotten = app
            .clone()
            .oneshot(
                Request::post("/api/memory/items")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "kind": "fact",
                            "content": "export-secret-that-must-disappear"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        let forgotten = response_json(forgotten).await;
        let id = forgotten["id"].as_str().unwrap();
        let revision = forgotten["revision"].as_str().unwrap();
        app.clone()
            .oneshot(
                Request::delete(format!("/api/memory/items/{id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(json!({ "revision": revision }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        app.clone()
            .oneshot(
                Request::post("/api/memory/items")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "kind": "preference", "content": "Keep this export item." })
                            .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        let response = app
            .oneshot(
                Request::get("/api/memory/export.json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=folumi-memory-export.json"
        );
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let export = response_json(response).await;
        assert_eq!(export["schema_version"], 1);
        assert_eq!(export["items"].as_array().unwrap().len(), 1);
        assert_eq!(export["items"][0]["content"], "Keep this export item.");
        let object = export.as_object().unwrap();
        assert_eq!(object.len(), 4);
        assert!(
            ["exported_at", "items", "schema_version", "settings"]
                .into_iter()
                .all(|key| object.contains_key(key))
        );
        let serialized = serde_json::to_string(&export).unwrap();
        assert!(!serialized.contains("export-secret-that-must-disappear"));
        assert!(!serialized.contains("tombstone"));
        assert!(!serialized.contains("memory_history"));
        assert!(!serialized.contains("policy_secret"));
    }

    #[tokio::test]
    async fn history_recall_setting_updates_runtime_sessions() {
        let (_directory, app) = app();
        let response = app
            .oneshot(
                Request::patch("/api/memory/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "enabled": true,
                            "history_recall_enabled": true,
                            "assistant_write_without_approval": true
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        assert_eq!(payload["history_recall_enabled"], true);
        assert_eq!(payload["assistant_write_without_approval"], true);
    }
}
