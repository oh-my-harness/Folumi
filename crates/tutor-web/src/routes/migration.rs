use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Json, Router, extract::State, routing::get};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::memory_store::{DurableMemoryWrite, FileMemoryBackend};

#[derive(Clone)]
struct MigrationState {
    data_dir: PathBuf,
    memory: Arc<FileMemoryBackend>,
}

#[derive(Clone, Deserialize, Serialize)]
struct LegacyContinuityEntry {
    id: String,
    tutor_id: String,
    kind: String,
    text: String,
    status: String,
    #[serde(default)]
    next_action: Option<String>,
    #[serde(default)]
    due_at: Option<DateTime<Utc>>,
    #[serde(default)]
    source_session_id: Option<String>,
    #[serde(default)]
    source_message_id: Option<String>,
}

#[derive(Deserialize)]
struct LegacyContinuityFile {
    #[serde(default)]
    entries: Vec<LegacyContinuityEntry>,
}

#[derive(Deserialize)]
struct ImportContinuityRequest {
    entry_ids: Vec<String>,
}

async fn legacy_preview(State(state): State<MigrationState>) -> impl IntoResponse {
    match read_legacy_continuity(&state.data_dir) {
        Ok(entries) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "continuity": entries,
                "quiz_export_available": legacy_quiz_export_available(&state.data_dir),
                "tutor_export_available": state.data_dir.join("tutors").is_dir(),
            })),
        )
            .into_response(),
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn legacy_quiz_export_available(data_dir: &Path) -> bool {
    data_dir.join("quizzes.json").is_file()
        || data_dir.join("memory/L2/quiz.md").is_file()
        || data_dir.join("memory/L1/quiz_events.jsonl").is_file()
}

async fn import_continuity(
    State(state): State<MigrationState>,
    Json(request): Json<ImportContinuityRequest>,
) -> impl IntoResponse {
    let entries = match read_legacy_continuity(&state.data_dir) {
        Ok(entries) => entries,
        Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let selected = request
        .entry_ids
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>();
    let available = entries
        .iter()
        .map(legacy_entry_key)
        .collect::<std::collections::BTreeSet<_>>();
    if !selected.is_subset(&available) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "one or more selected legacy continuity entries do not exist".into(),
        );
    }

    let mut imported = Vec::new();
    for entry in entries
        .into_iter()
        .filter(|entry| selected.contains(&legacy_entry_key(entry)))
    {
        let kind = match entry.kind.as_str() {
            "commitment" => "commitment",
            "open_loop" => "open_loop",
            "lesson_plan" | "reflection" | "strategy" => "strategy",
            _ => continue,
        };
        let content = match entry
            .next_action
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            Some(next_action) if next_action != entry.text => {
                format!("{} Next action: {}", entry.text.trim(), next_action)
            }
            _ => entry.text.trim().to_string(),
        };
        if content.is_empty() {
            continue;
        }
        let memory = match state.memory.upsert_durable_memory(DurableMemoryWrite {
            content,
            kind: kind.into(),
            provenance: serde_json::json!({
                "origin": "legacy_tutor_continuity_migration",
                "legacy_tutor_id": entry.tutor_id,
                "legacy_entry_id": entry.id,
                "source_session_id": entry.source_session_id,
                "source_message_id": entry.source_message_id,
                "due_at": entry.due_at,
            }),
            idempotency_key: format!("legacy-tutor-continuity:{}:{}", entry.tutor_id, entry.id),
            // A legacy due date says when a commitment needs attention; it is not an expiry.
            expires_at: None,
        }) {
            Ok(memory) => memory,
            Err(error) => return error_response(StatusCode::BAD_REQUEST, error.to_string()),
        };
        imported.push(memory.marker);
    }

    let count = imported.len();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "imported": imported,
            "count": count,
        })),
    )
        .into_response()
}

fn legacy_entry_key(entry: &LegacyContinuityEntry) -> String {
    format!("{}:{}", entry.tutor_id, entry.id)
}

async fn export_legacy(State(state): State<MigrationState>) -> impl IntoResponse {
    match build_legacy_export(&state.data_dir) {
        Ok(bytes) => {
            let mut response = Response::new(Body::from(bytes));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/zip"),
            );
            response.headers_mut().insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_static("attachment; filename=folumi-legacy-export.zip"),
            );
            response
        }
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

pub fn migration_router(data_dir: PathBuf, memory: Arc<FileMemoryBackend>) -> Router {
    Router::new()
        .route("/api/migration/legacy", get(legacy_preview))
        .route(
            "/api/migration/legacy/continuity",
            axum::routing::post(import_continuity),
        )
        .route("/api/migration/legacy/export.zip", get(export_legacy))
        .with_state(MigrationState { data_dir, memory })
}

fn read_legacy_continuity(data_dir: &Path) -> anyhow::Result<Vec<LegacyContinuityEntry>> {
    let tutors_root = data_dir.join("tutors");
    let mut entries = Vec::new();
    let Ok(children) = std::fs::read_dir(tutors_root) else {
        return Ok(entries);
    };
    for child in children {
        let child = child?;
        if !child.file_type()?.is_dir() {
            continue;
        }
        let path = child.path().join("memory.json");
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let file: LegacyContinuityFile = serde_json::from_slice(&bytes)?;
        entries.extend(
            file.entries
                .into_iter()
                .filter(|entry| entry.status == "active"),
        );
    }
    entries.sort_by(|left, right| {
        left.tutor_id
            .cmp(&right.tutor_id)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(entries)
}

fn build_legacy_export(data_dir: &Path) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut archive = zip::ZipWriter::new(cursor);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let quiz_path = data_dir.join("quizzes.json");
    if quiz_path.is_file() {
        archive.start_file("quizzes.json", options)?;
        archive.write_all(&std::fs::read(quiz_path)?)?;
    }
    append_file_if_exists(
        &mut archive,
        &data_dir.join("memory").join("L2").join("quiz.md"),
        "memory/L2/quiz.md",
        options,
    )?;
    append_file_if_exists(
        &mut archive,
        &data_dir.join("memory").join("L1").join("quiz_events.jsonl"),
        "memory/L1/quiz_events.jsonl",
        options,
    )?;
    append_directory(&mut archive, &data_dir.join("tutors"), "tutors", options)?;
    Ok(archive.finish()?.into_inner())
}

fn append_file_if_exists(
    archive: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    path: &Path,
    archive_path: &str,
    options: SimpleFileOptions,
) -> anyhow::Result<()> {
    if path.is_file() {
        archive.start_file(archive_path, options)?;
        archive.write_all(&std::fs::read(path)?)?;
    }
    Ok(())
}

fn append_directory(
    archive: &mut zip::ZipWriter<Cursor<Vec<u8>>>,
    root: &Path,
    archive_root: &str,
    options: SimpleFileOptions,
) -> anyhow::Result<()> {
    let Ok(children) = std::fs::read_dir(root) else {
        return Ok(());
    };
    for child in children {
        let child = child?;
        let path = child.path();
        let name = format!("{archive_root}/{}", child.file_name().to_string_lossy());
        if child.file_type()?.is_dir() {
            append_directory(archive, &path, &name, options)?;
        } else if child.file_type()?.is_file() && !name.ends_with(".tmp") {
            archive.start_file(name.replace('\\', "/"), options)?;
            archive.write_all(&std::fs::read(path)?)?;
        }
    }
    Ok(())
}

fn error_response(status: StatusCode, message: String) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;
    use tower::ServiceExt;

    fn legacy_app(root: &Path) -> (Router, Arc<FileMemoryBackend>) {
        let tutor_dir = root.join("tutors").join("legacy-a");
        std::fs::create_dir_all(&tutor_dir).unwrap();
        std::fs::write(
            tutor_dir.join("memory.json"),
            serde_json::to_vec(&serde_json::json!({
                "entries": [{
                    "id": "open-1",
                    "tutor_id": "legacy-a",
                    "kind": "open_loop",
                    "text": "Send the project summary.",
                    "status": "active",
                    "next_action": "Draft the summary tomorrow.",
                    "due_at": "2030-01-02T03:04:05Z"
                }, {
                    "id": "done-1",
                    "tutor_id": "legacy-a",
                    "kind": "commitment",
                    "text": "Already completed.",
                    "status": "resolved"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        std::fs::write(root.join("quizzes.json"), b"{\"quizzes\":[]}").unwrap();
        std::fs::create_dir_all(root.join("memory").join("L2")).unwrap();
        std::fs::create_dir_all(root.join("memory").join("L1")).unwrap();
        std::fs::write(
            root.join("memory").join("L2").join("quiz.md"),
            b"# Legacy quiz memory",
        )
        .unwrap();
        std::fs::write(
            root.join("memory").join("L1").join("quiz_events.jsonl"),
            b"{\"category\":\"quiz\"}\n",
        )
        .unwrap();
        let memory = Arc::new(FileMemoryBackend::new_with_root(root.join("memory")));
        (migration_router(root.to_path_buf(), memory.clone()), memory)
    }

    #[tokio::test]
    async fn previews_active_continuity_and_imports_selected_item_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let (app, memory) = legacy_app(dir.path());
        let preview = app
            .clone()
            .oneshot(
                Request::get("/api/migration/legacy")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(preview.status(), StatusCode::OK);
        let body = axum::body::to_bytes(preview.into_body(), usize::MAX)
            .await
            .unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(body["continuity"].as_array().unwrap().len(), 1);
        assert_eq!(body["quiz_export_available"], true);

        for _ in 0..2 {
            let imported = app
                .clone()
                .oneshot(
                    Request::post("/api/migration/legacy/continuity")
                        .header("content-type", "application/json")
                        .body(Body::from(r#"{"entry_ids":["legacy-a:open-1"]}"#))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(imported.status(), StatusCode::OK);
        }

        let entries = crate::memory_store::try_parse_memory_entries(
            &memory.read("L3/continuity.md").unwrap().markdown,
        )
        .unwrap();
        assert_eq!(entries.len(), 1);
        let metadata = entries[0].metadata.as_ref().unwrap();
        assert_eq!(metadata.kind, "open_loop");
        assert_eq!(metadata.expires_at, None);
        assert_eq!(metadata.provenance["due_at"], "2030-01-02T03:04:05Z");
    }

    #[tokio::test]
    async fn exports_legacy_data_without_reactivating_it() {
        let dir = tempfile::tempdir().unwrap();
        let (app, _) = legacy_app(dir.path());
        let response = app
            .oneshot(
                Request::get("/api/migration/legacy/export.zip")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/zip"
        );
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(body)).unwrap();
        assert!(archive.by_name("quizzes.json").is_ok());
        assert!(archive.by_name("tutors/legacy-a/memory.json").is_ok());
        assert!(archive.by_name("memory/L2/quiz.md").is_ok());
        assert!(archive.by_name("memory/L1/quiz_events.jsonl").is_ok());
    }
}
