use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Multipart, Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};

use crate::memory_store::{FileMemoryBackend, MemoryEventCategory};
use crate::notebook_store::{
    NotebookEntryInput, NotebookEntryType, NotebookEntryUpdate, NotebookStore,
};

#[derive(Clone)]
struct NotebookState {
    store: Arc<NotebookStore>,
    memory: Arc<FileMemoryBackend>,
}

#[derive(Deserialize)]
struct ListNotebookQuery {
    space_id: Option<String>,
}

#[derive(Deserialize)]
struct ExportNotebookQuery {
    space_id: Option<String>,
    entry_ids: Option<String>,
}

#[derive(Deserialize)]
struct CreateNotebookEntryRequest {
    space_id: Option<String>,
    entry_type: Option<NotebookEntryType>,
    title: String,
    path: Option<String>,
    markdown: String,
    metadata: Option<serde_json::Value>,
    source_session_id: Option<String>,
    source_message_id: Option<String>,
}

#[derive(Deserialize)]
struct UpdateNotebookEntryRequest {
    title: Option<String>,
    markdown: Option<String>,
    metadata: Option<serde_json::Value>,
    source_session_id: Option<String>,
    source_message_id: Option<String>,
}

#[derive(Deserialize)]
struct ImportFolderRequest {
    space_id: Option<String>,
    path: String,
}

#[derive(Deserialize)]
struct BindVaultRequest {
    path: String,
}

#[derive(Deserialize)]
struct CreateNotebookFolderRequest {
    path: String,
}

async fn list_entries(
    State(state): State<NotebookState>,
    Query(query): Query<ListNotebookQuery>,
) -> impl IntoResponse {
    let space_id = query.space_id.as_deref();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "entries": state.store.list_summaries(space_id),
            "folders": state.store.list_folders(),
            "vault": state.store.vault_info(),
            "watch": state.store.watch_info(),
        })),
    )
}

async fn list_tree(
    State(state): State<NotebookState>,
    Query(query): Query<ListNotebookQuery>,
) -> impl IntoResponse {
    let space_id = query.space_id.as_deref();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "entries": state.store.list_items(space_id),
            "folders": state.store.list_folders(),
            "vault": state.store.vault_info(),
            "watch": state.store.watch_info(),
        })),
    )
}

async fn refresh_vault(State(state): State<NotebookState>) -> impl IntoResponse {
    match state.store.refresh_from_vault() {
        Ok(result) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "refresh": result,
                "entries": state.store.list_items(Some("default")),
                "folders": state.store.list_folders(),
                "vault": state.store.vault_info(),
                "watch": state.store.watch_info(),
            })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn get_vault(State(state): State<NotebookState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "vault": state.store.vault_info(),
            "watch": state.store.watch_info(),
        })),
    )
}

async fn bind_vault(
    State(state): State<NotebookState>,
    Json(req): Json<BindVaultRequest>,
) -> impl IntoResponse {
    match state.store.set_vault_root(PathBuf::from(req.path)) {
        Ok(mount) => {
            let watch = state.store.start_watcher().ok();
            let _ = state.memory.record_event(
                MemoryEventCategory::Notebook,
                "bound_vault",
                format!("Bound notebook vault: {}", mount.vault.root),
                None,
                serde_json::json!({
                    "root": mount.vault.root,
                    "entries": mount.entries.len(),
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "vault": mount.vault,
                    "entries": mount.entries,
                    "folders": mount.folders,
                    "watch": watch.unwrap_or_else(|| state.store.watch_info()),
                })),
            )
                .into_response()
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn get_entry(
    State(state): State<NotebookState>,
    AxumPath(entry_id): AxumPath<String>,
) -> impl IntoResponse {
    match state.store.get_view(&entry_id) {
        Some(entry) => {
            (StatusCode::OK, Json(serde_json::json!({ "entry": entry }))).into_response()
        }
        None => error_response(StatusCode::NOT_FOUND, "notebook entry not found".into()),
    }
}

async fn create_entry(
    State(state): State<NotebookState>,
    Json(req): Json<CreateNotebookEntryRequest>,
) -> impl IntoResponse {
    match state.store.create(NotebookEntryInput {
        space_id: req.space_id,
        entry_type: req.entry_type.unwrap_or(NotebookEntryType::Note),
        title: req.title,
        path: req.path,
        markdown: req.markdown,
        metadata: req.metadata,
        source_session_id: req.source_session_id,
        source_message_id: req.source_message_id,
    }) {
        Ok(entry) => {
            let _ = state.memory.record_event(
                MemoryEventCategory::Notebook,
                "created",
                format!("Created notebook entry: {}", entry.title),
                Some(entry.id.clone()),
                serde_json::json!({
                    "entry_type": entry.entry_type,
                    "space_id": entry.space_id,
                    "source_session_id": entry.source_session_id,
                }),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "entry": entry })),
            )
                .into_response()
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn create_folder(
    State(state): State<NotebookState>,
    Json(req): Json<CreateNotebookFolderRequest>,
) -> impl IntoResponse {
    match state.store.create_folder(&req.path) {
        Ok(path) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "folder": { "path": path },
                "folders": state.store.list_folders(),
            })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn update_entry(
    State(state): State<NotebookState>,
    AxumPath(entry_id): AxumPath<String>,
    Json(req): Json<UpdateNotebookEntryRequest>,
) -> impl IntoResponse {
    match state.store.update(
        &entry_id,
        NotebookEntryUpdate {
            title: req.title,
            markdown: req.markdown,
            metadata: req.metadata,
            source_session_id: req.source_session_id,
            source_message_id: req.source_message_id,
        },
    ) {
        Ok(entry) => {
            let _ = state.memory.record_event(
                MemoryEventCategory::Notebook,
                "updated",
                format!("Updated notebook entry: {}", entry.title),
                Some(entry.id.clone()),
                serde_json::json!({
                    "entry_type": entry.entry_type,
                    "space_id": entry.space_id,
                    "metadata": entry.metadata,
                }),
            );
            (StatusCode::OK, Json(serde_json::json!({ "entry": entry }))).into_response()
        }
        Err(err) if err.to_string().contains("not found") => {
            error_response(StatusCode::NOT_FOUND, err.to_string())
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err.to_string()),
    }
}

async fn delete_entry(
    State(state): State<NotebookState>,
    AxumPath(entry_id): AxumPath<String>,
) -> impl IntoResponse {
    let existing = state.store.get(&entry_id);
    if state.store.delete(&entry_id) {
        let _ = state.memory.record_event(
            MemoryEventCategory::Notebook,
            "deleted",
            format!(
                "Deleted notebook entry: {}",
                existing
                    .as_ref()
                    .map(|entry| entry.title.as_str())
                    .unwrap_or(&entry_id)
            ),
            Some(entry_id),
            serde_json::json!({}),
        );
        StatusCode::NO_CONTENT.into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "notebook entry not found".into())
    }
}

async fn import_entries(
    State(state): State<NotebookState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let import = match parse_import_multipart(&mut multipart).await {
        Ok(import) => import,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, err.to_string()),
    };

    import_parsed_entries(state, import)
}

async fn import_folder(
    State(state): State<NotebookState>,
    Json(req): Json<ImportFolderRequest>,
) -> impl IntoResponse {
    let root = PathBuf::from(req.path);
    let import = match import_payloads_from_folder(&root) {
        Ok(upload) => ParsedImport {
            space_id: req.space_id,
            payloads: upload.payloads,
            skipped: upload.skipped,
        },
        Err(err) => return error_response(StatusCode::BAD_REQUEST, err.to_string()),
    };

    import_parsed_entries(state, import)
}

fn import_parsed_entries(state: NotebookState, import: ParsedImport) -> axum::response::Response {
    if import.payloads.is_empty() && import.skipped.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "missing file field".into());
    }

    let mut skipped = import.skipped;
    let inputs = import
        .payloads
        .into_iter()
        .map(|payload| {
            let source_path = payload.source_path.clone();
            (
                source_path.clone(),
                NotebookEntryInput {
                    space_id: import.space_id.clone(),
                    entry_type: NotebookEntryType::Note,
                    title: payload.title,
                    path: Some(source_path),
                    markdown: payload.markdown,
                    metadata: Some(payload.metadata),
                    source_session_id: None,
                    source_message_id: None,
                },
            )
        })
        .collect::<Vec<_>>();
    let source_paths = inputs
        .iter()
        .map(|(source_path, _)| source_path.clone())
        .collect::<Vec<_>>();
    let imported = match state
        .store
        .create_many(inputs.into_iter().map(|(_, input)| input).collect())
    {
        Ok(entries) => entries,
        Err(err) => {
            let reason = err.to_string();
            for file_name in source_paths {
                skipped.push(ImportSkipped {
                    file_name,
                    reason: reason.clone(),
                });
            }
            Vec::new()
        }
    };

    if imported.is_empty() && skipped.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "missing file field".into());
    }

    if !imported.is_empty() {
        let _ = state.memory.record_event(
            MemoryEventCategory::Notebook,
            "imported",
            format!("Imported {} notebook entries", imported.len()),
            None,
            serde_json::json!({
                "entry_ids": imported.iter().map(|entry| entry.id.as_str()).collect::<Vec<_>>(),
                "skipped_count": skipped.len(),
            }),
        );
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "entries": imported,
            "skipped": skipped,
        })),
    )
        .into_response()
}

async fn preview_import_entries(
    State(state): State<NotebookState>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let import = match parse_import_multipart(&mut multipart).await {
        Ok(import) => import,
        Err(err) => return error_response(StatusCode::BAD_REQUEST, err.to_string()),
    };

    if import.payloads.is_empty() && import.skipped.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "missing file field".into());
    }

    let existing = state.store.list(import.space_id.as_deref());
    let items = import
        .payloads
        .iter()
        .map(|payload| {
            let duplicate_title_entry = existing
                .iter()
                .find(|entry| normalized_key(&entry.title) == normalized_key(&payload.title));
            ImportPreviewItem {
                source_path: payload.source_path.clone(),
                title: payload.title.clone(),
                markdown_chars: payload.markdown.chars().count(),
                tags: crate::notebook_store::metadata_tags(Some(&payload.metadata)),
                duplicate_title_entry_id: duplicate_title_entry.map(|entry| entry.id.clone()),
                duplicate_title: duplicate_title_entry.map(|entry| entry.title.clone()),
            }
        })
        .collect::<Vec<_>>();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "items": items,
            "skipped": import.skipped,
            "conflict_count": items.iter().filter(|item| item.duplicate_title_entry_id.is_some()).count(),
        })),
    )
        .into_response()
}

async fn export_entry(
    State(state): State<NotebookState>,
    AxumPath(entry_id): AxumPath<String>,
) -> impl IntoResponse {
    let Some(entry) = state.store.get_view(&entry_id) else {
        return error_response(StatusCode::NOT_FOUND, "notebook entry not found".into());
    };
    let markdown = export_markdown(&entry);
    markdown_response(single_export_file_name(&entry.entry), markdown)
}

async fn export_entries_zip(
    State(state): State<NotebookState>,
    Query(query): Query<ExportNotebookQuery>,
) -> impl IntoResponse {
    let mut entries = state.store.list_views(query.space_id.as_deref());
    if let Some(ids) = query.entry_ids {
        let requested = ids
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        entries.retain(|entry| requested.iter().any(|id| id == &entry.entry.id));
    }
    if entries.is_empty() {
        return error_response(
            StatusCode::NOT_FOUND,
            "no notebook entries to export".into(),
        );
    }
    match export_zip(&entries) {
        Ok(bytes) => bytes_response("notebook-export.zip", "application/zip", bytes),
        Err(err) => error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

async fn export_obsidian_vault_zip(
    State(state): State<NotebookState>,
    Query(query): Query<ExportNotebookQuery>,
) -> impl IntoResponse {
    let mut entries = state.store.list_views(query.space_id.as_deref());
    if let Some(ids) = query.entry_ids {
        let requested = ids
            .split(',')
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        entries.retain(|entry| requested.iter().any(|id| id == &entry.entry.id));
    }
    if entries.is_empty() {
        return error_response(
            StatusCode::NOT_FOUND,
            "no notebook entries to export".into(),
        );
    }
    match export_obsidian_vault(&entries) {
        Ok(bytes) => bytes_response("notebook-vault.zip", "application/zip", bytes),
        Err(err) => error_response(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    }
}

pub fn notebook_router(store: Arc<NotebookStore>, memory: Arc<FileMemoryBackend>) -> Router {
    let state = NotebookState { store, memory };
    Router::new()
        .route("/api/notebook/entries", get(list_tree).post(create_entry))
        .route("/api/notebook/entries/full", get(list_entries))
        .route("/api/notebook/refresh", axum::routing::post(refresh_vault))
        .route(
            "/api/notebook/entries/{entry_id}",
            get(get_entry).patch(update_entry).delete(delete_entry),
        )
        .route(
            "/api/notebook/entries/{entry_id}/export.md",
            get(export_entry),
        )
        .route("/api/notebook/export.zip", get(export_entries_zip))
        .route(
            "/api/notebook/export-vault.zip",
            get(export_obsidian_vault_zip),
        )
        .route("/api/notebook/vault", get(get_vault).put(bind_vault))
        .route(
            "/api/notebook/import/preview",
            axum::routing::post(preview_import_entries),
        )
        .route("/api/notebook/import", axum::routing::post(import_entries))
        .route("/api/notebook/folders", axum::routing::post(create_folder))
        .route(
            "/api/notebook/import/folder",
            axum::routing::post(import_folder),
        )
        .with_state(state)
}

fn error_response(status: StatusCode, message: String) -> axum::response::Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[derive(Debug)]
struct ImportPayload {
    source_path: String,
    title: String,
    markdown: String,
    metadata: serde_json::Value,
}

#[derive(Debug)]
struct ParsedImport {
    space_id: Option<String>,
    payloads: Vec<ImportPayload>,
    skipped: Vec<ImportSkipped>,
}

#[derive(Debug)]
struct ParsedUpload {
    payloads: Vec<ImportPayload>,
    skipped: Vec<ImportSkipped>,
}

#[derive(Debug, Serialize)]
struct ImportSkipped {
    file_name: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct ImportPreviewItem {
    source_path: String,
    title: String,
    markdown_chars: usize,
    tags: Vec<String>,
    duplicate_title_entry_id: Option<String>,
    duplicate_title: Option<String>,
}

async fn parse_import_multipart(multipart: &mut Multipart) -> anyhow::Result<ParsedImport> {
    let mut space_id = None;
    let mut payloads = Vec::new();
    let mut skipped = Vec::new();

    loop {
        let Some(field) = multipart.next_field().await? else {
            break;
        };

        match field.name() {
            Some("space_id") => {
                let value = field.text().await?;
                if !value.trim().is_empty() {
                    space_id = Some(value.trim().to_string());
                }
            }
            Some("file") | Some("files") => {
                let file_name = field.file_name().unwrap_or("notebook.md").to_string();
                let bytes = field.bytes().await?;
                match import_payloads_from_upload(&file_name, bytes.as_ref()) {
                    Ok(upload) if upload.payloads.is_empty() && upload.skipped.is_empty() => {
                        skipped.push(ImportSkipped {
                            file_name,
                            reason: "no markdown files found".into(),
                        })
                    }
                    Ok(upload) => {
                        payloads.extend(upload.payloads);
                        skipped.extend(upload.skipped);
                    }
                    Err(err) => skipped.push(ImportSkipped {
                        file_name,
                        reason: err.to_string(),
                    }),
                }
            }
            _ => {}
        }
    }

    Ok(ParsedImport {
        space_id,
        payloads,
        skipped,
    })
}

fn import_payloads_from_upload(file_name: &str, bytes: &[u8]) -> anyhow::Result<ParsedUpload> {
    if file_name.to_lowercase().ends_with(".zip") {
        return import_payloads_from_zip(file_name, bytes);
    }
    if !file_name.to_lowercase().ends_with(".md")
        && !file_name.to_lowercase().ends_with(".markdown")
    {
        anyhow::bail!("unsupported notebook import file type");
    }
    let markdown = decode_utf8(bytes)?;
    let skipped = skipped_asset_references(file_name, &markdown);
    Ok(ParsedUpload {
        payloads: vec![payload_from_markdown(file_name, markdown)?],
        skipped,
    })
}

fn import_payloads_from_zip(file_name: &str, bytes: &[u8]) -> anyhow::Result<ParsedUpload> {
    let reader = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)?;
    let mut payloads = Vec::new();
    let mut skipped = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().replace('\\', "/");
        if !name.to_lowercase().ends_with(".md") && !name.to_lowercase().ends_with(".markdown") {
            if should_report_unsupported_asset(&name) {
                skipped.push(ImportSkipped {
                    file_name: name,
                    reason: "Obsidian attachment/assets are not imported yet".into(),
                });
            }
            continue;
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let markdown = decode_utf8(&bytes)?;
        skipped.extend(skipped_asset_references(&name, &markdown));
        payloads.push(payload_from_markdown(&name, markdown)?);
    }
    payloads.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    if payloads.is_empty() {
        anyhow::bail!("{file_name} contains no markdown files");
    }
    skipped.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    skipped.dedup_by(|a, b| a.file_name == b.file_name && a.reason == b.reason);
    Ok(ParsedUpload { payloads, skipped })
}

fn import_payloads_from_folder(root: &Path) -> anyhow::Result<ParsedUpload> {
    if !root.is_dir() {
        anyhow::bail!("notebook import folder does not exist or is not a directory");
    }
    let mut payloads = Vec::new();
    let mut skipped = Vec::new();
    collect_folder_import(root, root, &mut payloads, &mut skipped)?;
    payloads.sort_by(|a, b| a.source_path.cmp(&b.source_path));
    skipped.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    skipped.dedup_by(|a, b| a.file_name == b.file_name && a.reason == b.reason);
    if payloads.is_empty() {
        anyhow::bail!("folder contains no markdown files");
    }
    Ok(ParsedUpload { payloads, skipped })
}

fn collect_folder_import(
    root: &Path,
    dir: &Path,
    payloads: &mut Vec<ImportPayload>,
    skipped: &mut Vec<ImportSkipped>,
) -> anyhow::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_folder_import(root, &path, payloads, skipped)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        let source_path = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let lower = source_path.to_lowercase();
        if !lower.ends_with(".md") && !lower.ends_with(".markdown") {
            if should_report_unsupported_asset(&source_path) {
                skipped.push(ImportSkipped {
                    file_name: source_path,
                    reason: "Obsidian attachment/assets are not imported yet".into(),
                });
            }
            continue;
        }
        match std::fs::read(&path)
            .map_err(anyhow::Error::from)
            .and_then(|bytes| decode_utf8(&bytes))
            .and_then(|markdown| {
                skipped.extend(skipped_asset_references(&source_path, &markdown));
                payload_from_markdown(&source_path, markdown)
            }) {
            Ok(payload) => payloads.push(payload),
            Err(err) => skipped.push(ImportSkipped {
                file_name: source_path,
                reason: err.to_string(),
            }),
        }
    }
    Ok(())
}

fn payload_from_markdown(source_path: &str, markdown: String) -> anyhow::Result<ImportPayload> {
    if markdown.trim().is_empty() {
        anyhow::bail!("markdown file is empty");
    }
    let frontmatter = parse_frontmatter(&markdown);
    let title = title_from_path(source_path);
    let mut metadata = serde_json::json!({
        "import": {
            "source_path": source_path,
            "format": "markdown",
        },
        "frontmatter": frontmatter.unwrap_or_else(|| serde_json::json!({})),
    });
    if let Some(tags) = metadata["frontmatter"].get("tags").cloned() {
        metadata["tags"] = tags;
    }
    if let Some(tag) = metadata["frontmatter"].get("tag").cloned() {
        metadata["tag"] = tag;
    }
    Ok(ImportPayload {
        source_path: source_path.to_string(),
        title,
        markdown,
        metadata,
    })
}

fn skipped_asset_references(source_path: &str, markdown: &str) -> Vec<ImportSkipped> {
    let mut skipped = Vec::new();
    for asset in markdown_asset_references(markdown) {
        skipped.push(ImportSkipped {
            file_name: format!("{source_path} -> {asset}"),
            reason: "Referenced Obsidian attachment/assets are not imported yet".into(),
        });
    }
    skipped
}

fn markdown_asset_references(markdown: &str) -> Vec<String> {
    let mut assets = Vec::new();
    let mut index = 0;
    while let Some(start) = markdown[index..].find("](") {
        let open = index + start + 2;
        let Some(close_offset) = markdown[open..].find(')') else {
            break;
        };
        let target = markdown[open..open + close_offset]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches('"')
            .trim_matches('\'');
        if should_report_unsupported_asset(target) {
            assets.push(target.to_string());
        }
        index = open + close_offset + 1;
    }

    index = 0;
    while let Some(start) = markdown[index..].find("![[") {
        let open = index + start + 3;
        let Some(close_offset) = markdown[open..].find("]]") else {
            break;
        };
        let target = markdown[open..open + close_offset]
            .split('|')
            .next()
            .unwrap_or("")
            .trim();
        if should_report_unsupported_asset(target) {
            assets.push(target.to_string());
        }
        index = open + close_offset + 2;
    }

    assets.sort();
    assets.dedup();
    assets
}

fn should_report_unsupported_asset(path: &str) -> bool {
    let normalized = path.trim().trim_start_matches("./").to_lowercase();
    if normalized.is_empty()
        || normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with('#')
        || normalized.starts_with(".obsidian/")
        || normalized.ends_with(".json")
        || normalized.ends_with(".css")
    {
        return false;
    }
    matches!(
        Path::new(&normalized)
            .extension()
            .and_then(|extension| extension.to_str()),
        Some(
            "png"
                | "jpg"
                | "jpeg"
                | "gif"
                | "webp"
                | "svg"
                | "bmp"
                | "pdf"
                | "mp3"
                | "mp4"
                | "mov"
                | "wav"
                | "ogg"
                | "canvas"
        )
    )
}

fn parse_frontmatter(markdown: &str) -> Option<serde_json::Value> {
    let normalized = markdown
        .strip_prefix("---\r\n")
        .or_else(|| markdown.strip_prefix("---\n"))?;
    let frontmatter_start = markdown.len() - normalized.len();
    let mut offset = frontmatter_start;
    for line in normalized.split_inclusive('\n') {
        let line_text = line.trim_end_matches(['\r', '\n']);
        if line_text.trim() == "---" {
            let yaml = &markdown[frontmatter_start..offset];
            return serde_yaml::from_str::<serde_yaml::Value>(yaml)
                .ok()
                .and_then(|value| serde_json::to_value(value).ok());
        }
        offset += line.len();
    }
    None
}

fn title_from_path(source_path: &str) -> String {
    let normalized = source_path.replace('\\', "/");
    let file_name = normalized.rsplit('/').next().unwrap_or(source_path);
    Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Imported note")
        .to_string()
}

fn decode_utf8(bytes: &[u8]) -> anyhow::Result<String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| anyhow::anyhow!("file is not valid UTF-8"))
}

fn export_markdown(entry: &crate::notebook_store::NotebookEntryView) -> String {
    let tags = if entry.tags.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(entry.tags)
    };
    let frontmatter = serde_json::json!({
        "id": entry.entry.id,
        "title": entry.entry.title,
        "type": entry.entry.entry_type,
        "tags": tags,
        "source_session_id": entry.entry.source_session_id,
        "source_message_id": entry.entry.source_message_id,
        "created_at": entry.entry.created_at,
        "updated_at": entry.entry.updated_at,
    });
    let yaml = serde_yaml::to_string(&frontmatter).unwrap_or_default();
    format!(
        "---\n{}---\n\n{}",
        yaml.trim_start_matches("---\n"),
        strip_frontmatter(&entry.entry.markdown).trim_start()
    )
}

fn strip_frontmatter(markdown: &str) -> &str {
    let Some(normalized) = markdown
        .strip_prefix("---\r\n")
        .or_else(|| markdown.strip_prefix("---\n"))
    else {
        return markdown;
    };
    let mut consumed = markdown.len() - normalized.len();
    for line in normalized.split_inclusive('\n') {
        consumed += line.len();
        if line.trim_end_matches(['\r', '\n']).trim() == "---" {
            return &markdown[consumed..];
        }
    }
    markdown
}

fn export_zip(entries: &[crate::notebook_store::NotebookEntryView]) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut used_names = Vec::new();
    for entry in entries {
        let file_name = unique_file_name(&entry_export_path(&entry.entry), &mut used_names);
        writer.start_file(file_name, options)?;
        std::io::Write::write_all(&mut writer, export_markdown(entry).as_bytes())?;
    }
    Ok(writer.finish()?.into_inner())
}

fn export_obsidian_vault(
    entries: &[crate::notebook_store::NotebookEntryView],
) -> anyhow::Result<Vec<u8>> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut used_names = Vec::new();

    writer.start_file(".obsidian/app.json", options)?;
    std::io::Write::write_all(
        &mut writer,
        br#"{"alwaysUpdateLinks":true,"newFileLocation":"root","attachmentFolderPath":"assets"}"#,
    )?;
    writer.start_file(".obsidian/appearance.json", options)?;
    std::io::Write::write_all(&mut writer, br#"{"theme":"obsidian"}"#)?;
    writer.start_file("README.md", options)?;
    std::io::Write::write_all(
        &mut writer,
        b"# Notebook Vault\n\nExported from Tutor Agent. Notes keep their YAML frontmatter and wiki links.\n",
    )?;

    for entry in entries {
        let file_name = unique_file_name(&entry_export_path(&entry.entry), &mut used_names);
        writer.start_file(file_name, options)?;
        std::io::Write::write_all(&mut writer, export_markdown(entry).as_bytes())?;
    }
    Ok(writer.finish()?.into_inner())
}

fn markdown_response(file_name: String, markdown: String) -> axum::response::Response {
    bytes_response(
        file_name,
        "text/markdown; charset=utf-8",
        markdown.into_bytes(),
    )
}

fn bytes_response(
    file_name: impl AsRef<str>,
    content_type: &str,
    bytes: Vec<u8>,
) -> axum::response::Response {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!(
            "attachment; filename=\"{}\"",
            file_name.as_ref().replace('"', "")
        ))
        .unwrap_or(HeaderValue::from_static("attachment")),
    );
    (StatusCode::OK, headers, bytes).into_response()
}

fn safe_markdown_file_name(title: &str) -> String {
    format!("{}.md", safe_file_stem(title))
}

fn entry_export_path(entry: &crate::notebook_store::NotebookEntry) -> String {
    entry
        .path
        .as_deref()
        .and_then(normalize_export_path)
        .unwrap_or_else(|| safe_markdown_file_name(&entry.title))
}

fn single_export_file_name(entry: &crate::notebook_store::NotebookEntry) -> String {
    entry_export_path(entry)
        .rsplit('/')
        .next()
        .map(ToOwned::to_owned)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| safe_markdown_file_name(&entry.title))
}

fn normalize_export_path(path: &str) -> Option<String> {
    let parts = path
        .replace('\\', "/")
        .split('/')
        .map(safe_file_stem)
        .filter(|part| !part.is_empty() && part != "." && part != "..")
        .collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let mut normalized = parts.join("/");
    if !normalized.to_lowercase().ends_with(".md")
        && !normalized.to_lowercase().ends_with(".markdown")
    {
        normalized.push_str(".md");
    }
    Some(normalized)
}

fn safe_file_stem(title: &str) -> String {
    let name = title
        .chars()
        .map(|ch| match ch {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            ch if ch.is_control() => '-',
            ch => ch,
        })
        .collect::<String>()
        .trim()
        .trim_matches('.')
        .to_string();
    if name.is_empty() { "note".into() } else { name }
}

fn unique_file_name(base: &str, used_names: &mut Vec<String>) -> String {
    if !used_names
        .iter()
        .any(|name| name.eq_ignore_ascii_case(base))
    {
        used_names.push(base.to_string());
        return base.to_string();
    }
    let stem = base.strip_suffix(".md").unwrap_or(base);
    for index in 2.. {
        let candidate = format!("{stem}-{index}.md");
        if !used_names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&candidate))
        {
            used_names.push(candidate.clone());
            return candidate;
        }
    }
    unreachable!()
}

fn normalized_key(value: &str) -> String {
    value.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    #[tokio::test]
    async fn creates_lists_and_deletes_notebook_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let app = notebook_router(store, memory.clone());
        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/api/notebook/folders",
                serde_json::json!({ "path": "concepts/lithography" }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        assert_eq!(body["folder"]["path"], "concepts/lithography");

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/api/notebook/entries",
                serde_json::json!({
                    "title": "Report",
                    "path": "concepts/lithography/Report.md",
                    "entry_type": "research_report",
                    "markdown": "# Report"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        let entry_id = body["entry"]["id"].as_str().unwrap();

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/notebook/entries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        assert_eq!(body["entries"][0]["title"], "Report");
        assert!(body["entries"][0].get("markdown").is_none());
        assert_eq!(body["folders"][0], "concepts");
        assert_eq!(body["folders"][1], "concepts/lithography");

        let response = app
            .clone()
            .oneshot(json_request(
                Method::PATCH,
                &format!("/api/notebook/entries/{entry_id}"),
                serde_json::json!({
                    "title": "Updated report",
                    "markdown": "# Updated report"
                }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["entry"]["title"], "Updated report");
        assert_eq!(body["entry"]["markdown"], "# Updated report");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::DELETE)
                    .uri(format!("/api/notebook/entries/{entry_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert!(!memory.recent_events(10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn imports_markdown_entries_from_multipart() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let app = notebook_router(store, memory.clone());
        let markdown = "---\ntitle: Imported OPC\ntags:\n  - optics\n  - opc\n---\n# Imported OPC\n\nSee [[Lithography]].\n";
        let body = multipart_body("BOUNDARY", "opc.md", "text/markdown", markdown.as_bytes());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/notebook/import")
                    .header("content-type", "multipart/form-data; boundary=BOUNDARY")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        assert_eq!(body["entries"].as_array().unwrap().len(), 1);
        assert_eq!(body["entries"][0]["title"], "opc");
        assert_eq!(body["entries"][0]["path"], "opc.md");
        assert_eq!(body["skipped"].as_array().unwrap().len(), 0);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/notebook/entries")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = response_json(response).await;
        assert_eq!(body["entries"][0]["tags"][0], "opc");
        assert_eq!(body["entries"][0]["tags"][1], "optics");
        assert!(body["entries"][0].get("markdown").is_none());

        let entry_id = body["entries"][0]["id"].as_str().unwrap();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/notebook/entries/{entry_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert!(
            body["entry"]["markdown"]
                .as_str()
                .unwrap()
                .contains("[[Lithography]]")
        );
        assert_eq!(body["entry"]["links"][0]["target"], "Lithography");
        assert!(!memory.recent_events(10).unwrap().is_empty());
    }

    #[tokio::test]
    async fn previews_import_conflicts_and_exports_notebook() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let existing = store
            .create(NotebookEntryInput {
                space_id: Some("default".into()),
                entry_type: NotebookEntryType::Note,
                path: None,
                title: "Imported OPC".into(),
                markdown: "# Imported OPC\n\nExisting.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let app = notebook_router(store, memory);
        let markdown = "---\ntitle: Imported OPC\ntags: [opc]\n---\n# Imported OPC\n\nNew.";
        let body = multipart_body("BOUNDARY", "opc.md", "text/markdown", markdown.as_bytes());

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/notebook/import/preview")
                    .header("content-type", "multipart/form-data; boundary=BOUNDARY")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["conflict_count"], 0);
        assert_eq!(body["items"][0]["title"], "opc");
        assert_eq!(
            body["items"][0]["duplicate_title_entry_id"],
            serde_json::Value::Null
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/notebook/entries/{}/export.md", existing.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(text.contains("title: Imported OPC"));
        assert!(text.contains("# Imported OPC"));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/notebook/export.zip?space_id=default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert_eq!(archive.len(), 1);
        assert_eq!(archive.by_index(0).unwrap().name(), "Imported OPC.md");

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/notebook/export-vault.zip?space_id=default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        assert!(archive.by_name(".obsidian/app.json").is_ok());
        assert!(archive.by_name(".obsidian/appearance.json").is_ok());
        assert!(archive.by_name("README.md").is_ok());
        assert!(archive.by_name("Imported OPC.md").is_ok());
    }

    #[test]
    fn parses_markdown_frontmatter_and_zip_imports() {
        let markdown = "---\r\ntitle: Process Notes\ntags: lithography opc\r\n---\r\n# Body";
        let payload = payload_from_markdown("folder/process.md", markdown.into()).unwrap();
        assert_eq!(payload.title, "process");
        assert_eq!(payload.metadata["frontmatter"]["tags"], "lithography opc");

        let mut zip_bytes = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut zip_bytes);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            writer.start_file("notes/a.md", options).unwrap();
            std::io::Write::write_all(&mut writer, b"# A").unwrap();
            writer.start_file("assets/image.png", options).unwrap();
            std::io::Write::write_all(&mut writer, b"not markdown").unwrap();
            writer.finish().unwrap();
        }
        let payloads = import_payloads_from_upload("vault.zip", zip_bytes.get_ref()).unwrap();
        assert_eq!(payloads.payloads.len(), 1);
        assert_eq!(payloads.payloads[0].title, "a");
        assert_eq!(payloads.payloads[0].source_path, "notes/a.md");
        assert_eq!(payloads.skipped.len(), 1);
        assert_eq!(payloads.skipped[0].file_name, "assets/image.png");
    }

    #[test]
    fn imports_markdown_entries_from_folder_and_reports_assets() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("notes").join("week1");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            nested.join("topic.md"),
            "# Topic\n\nSee ![[diagram.png]] and [PDF](../assets/ref.pdf).",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("assets").join("diagram.png"), b"png").unwrap();
        std::fs::write(dir.path().join("assets").join("ref.pdf"), b"pdf").unwrap();

        let upload = import_payloads_from_folder(dir.path()).unwrap();
        assert_eq!(upload.payloads.len(), 1);
        assert_eq!(upload.payloads[0].source_path, "notes/week1/topic.md");
        assert!(
            upload
                .skipped
                .iter()
                .any(|item| item.file_name == "assets/diagram.png")
        );
        assert!(
            upload
                .skipped
                .iter()
                .any(|item| item.file_name == "assets/ref.pdf")
        );
        assert!(
            upload
                .skipped
                .iter()
                .any(|item| item.file_name == "notes/week1/topic.md -> diagram.png")
        );
    }

    fn json_request(method: Method, uri: &str, value: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap()
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn multipart_body(
        boundary: &str,
        file_name: &str,
        content_type: &str,
        bytes: &[u8],
    ) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
        body.extend_from_slice(
            format!("Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n")
                .as_bytes(),
        );
        body.extend_from_slice(format!("Content-Type: {content_type}\r\n\r\n").as_bytes());
        body.extend_from_slice(bytes);
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }
}
