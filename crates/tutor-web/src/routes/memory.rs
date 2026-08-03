use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch},
};
use serde::{Deserialize, Serialize};

use crate::memory_store::{
    CreateMemoryItem, MemoryItem, MemoryKind, MemoryListFilter, MemoryOrigin, MemoryStatus,
    MemoryStore, MemoryStoreError, UpdateMemoryItem,
};

#[derive(Clone)]
struct MemoryState {
    store: Arc<MemoryStore>,
}

pub fn memory_router(store: Arc<MemoryStore>) -> Router {
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
        .with_state(MemoryState { store })
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
}

async fn update_settings(
    State(state): State<MemoryState>,
    Json(input): Json<UpdateSettingsRequest>,
) -> Response {
    let current = match state.store.settings() {
        Ok(settings) => settings,
        Err(error) => return error_response(error),
    };
    match state.store.update_settings(
        input.enabled.unwrap_or(current.enabled),
        input
            .history_recall_enabled
            .unwrap_or(current.history_recall_enabled),
    ) {
        Ok(settings) => (StatusCode::OK, Json(settings)).into_response(),
        Err(error) => error_response(error),
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
        (directory, memory_router(store))
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
    async fn history_recall_setting_fails_closed() {
        let (_directory, app) = app();
        let response = app
            .oneshot(
                Request::patch("/api/memory/settings")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({ "enabled": true, "history_recall_enabled": true }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["code"], "validation");
    }
}
