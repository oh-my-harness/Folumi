use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router,
    body::Body,
    extract::{Multipart, Path, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{delete, get, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::knowledge_store::{
    KnowledgeBaseView, KnowledgeDocument, KnowledgeStore, normalize_embedding_config,
};
use crate::memory_store::{MemoryEventCategory, MemoryStore};

#[derive(Clone)]
struct KnowledgeState {
    store: Arc<KnowledgeStore>,
    jobs: Arc<IngestionJobs>,
    memory: Arc<MemoryStore>,
    rag_root: PathBuf,
}

#[derive(Default)]
struct IngestionJobs {
    items: Mutex<HashMap<String, IngestionJob>>,
}

#[derive(Clone, Serialize)]
struct IngestionJob {
    id: String,
    status: IngestionJobStatus,
    stage: String,
    message: String,
    progress: u8,
    chunks: Option<usize>,
    knowledge_base: Option<KnowledgeBaseView>,
    error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "snake_case")]
enum IngestionJobStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Deserialize)]
struct CreateKnowledgeBaseRequest {
    name: String,
    embedding: tutor_rag::EmbeddingConfig,
}

#[derive(Deserialize)]
struct IngestDocumentRequest {
    source: Option<String>,
    text: String,
}

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
    top_k: Option<usize>,
}

const CHAT_ATTACHMENT_MAX_CHARS: usize = 20_000;

#[derive(Serialize)]
struct ParsedAttachment {
    name: String,
    size: usize,
    mime_type: Option<String>,
    text: String,
    truncated: bool,
}

async fn list_knowledge_bases(State(state): State<KnowledgeState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "knowledge_bases": state.store.list() })),
    )
}

async fn create_knowledge_base(
    State(state): State<KnowledgeState>,
    Json(req): Json<CreateKnowledgeBaseRequest>,
) -> impl IntoResponse {
    match state
        .store
        .create(req.name, normalize_embedding_config(req.embedding))
    {
        Ok(item) => {
            record_knowledge_event(
                &state,
                "create_kb",
                format!("Created knowledge base `{}`.", item.name),
                Some(item.id.clone()),
                serde_json::json!({
                    "kb": item.id,
                    "name": item.name,
                    "embedding": item.embedding,
                }),
            );
            (
                StatusCode::CREATED,
                Json(serde_json::json!({ "knowledge_base": item })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn delete_knowledge_base(
    State(state): State<KnowledgeState>,
    Path(kb): Path<String>,
) -> impl IntoResponse {
    if let Some(item) = state.store.get(&kb) {
        let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
        if let Err(err) = rag.delete_kb(&kb).await {
            return error_response(err);
        }
    }

    match state.store.delete(&kb) {
        Ok(true) => {
            record_knowledge_event(
                &state,
                "delete_kb",
                format!("Deleted knowledge base `{kb}`."),
                Some(kb),
                serde_json::json!({ "kb_deleted": true }),
            );
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn ingest_document(
    State(state): State<KnowledgeState>,
    Path(kb): Path<String>,
    Json(req): Json<IngestDocumentRequest>,
) -> impl IntoResponse {
    if req.text.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "document text is empty" })),
        )
            .into_response();
    }

    if state.store.get(&kb).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    }

    let source = req.source.unwrap_or_else(|| "pasted-text".to_string());
    let size_bytes = req.text.len();
    let job = state.jobs.create();
    let job_id = job.id.clone();
    let task_state = state.clone();
    tokio::spawn(async move {
        update_job(
            &task_state.jobs,
            &job_id,
            "parse",
            "Preparing pasted text",
            12,
            None,
        );
        let result =
            ingest_text_document(&task_state, &job_id, &kb, &source, &req.text, size_bytes).await;
        finish_job(&task_state.jobs, &job_id, result);
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "job": job })),
    )
        .into_response()
}

async fn upload_document(
    State(state): State<KnowledgeState>,
    Path(kb): Path<String>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if state.store.get(&kb).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    }

    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return error_response(err),
        };

        if field.name() != Some("file") {
            continue;
        }

        let file_name = field.file_name().unwrap_or("uploaded.txt").to_string();
        let content_type = field.content_type().map(str::to_string);
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => return error_response(err),
        };
        let size_bytes = bytes.len();
        let normalized_content_type = upload_mime_type(&file_name, content_type.as_deref());
        let job = state.jobs.create();
        let job_id = job.id.clone();
        let task_state = state.clone();
        tokio::spawn(async move {
            let result = ingest_upload_document(
                &task_state,
                UploadDocumentInput {
                    job_id: &job_id,
                    kb: &kb,
                    source: &file_name,
                    size_bytes,
                    original_bytes: bytes.to_vec(),
                    content_type,
                    mime_type: normalized_content_type,
                },
            )
            .await;
            finish_job(&task_state.jobs, &job_id, result);
        });
        return (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({ "job": job })),
        )
            .into_response();
    }

    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "missing file field" })),
    )
        .into_response()
}

async fn parse_chat_attachment(mut multipart: Multipart) -> impl IntoResponse {
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(err) => return error_response(err),
        };

        if field.name() != Some("file") {
            continue;
        }

        let file_name = field.file_name().unwrap_or("attachment.txt").to_string();
        let content_type = field.content_type().map(str::to_string);
        let bytes = match field.bytes().await {
            Ok(bytes) => bytes,
            Err(err) => return error_response(err),
        };
        let size = bytes.len();
        let text = match extract_upload_text(&file_name, content_type.as_deref(), bytes.as_ref()) {
            Ok(text) => text,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": err.to_string() })),
                )
                    .into_response();
            }
        };
        let truncated = text.chars().count() > CHAT_ATTACHMENT_MAX_CHARS;
        let text = if truncated {
            text.chars().take(CHAT_ATTACHMENT_MAX_CHARS).collect()
        } else {
            text
        };
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "attachment": ParsedAttachment {
                    name: file_name.clone(),
                    size,
                    mime_type: upload_mime_type(&file_name, content_type.as_deref()),
                    text,
                    truncated,
                }
            })),
        )
            .into_response();
    }

    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "error": "missing file field" })),
    )
        .into_response()
}

fn extract_upload_text(
    file_name: &str,
    content_type: Option<&str>,
    bytes: &[u8],
) -> anyhow::Result<String> {
    if is_pdf_upload(file_name, content_type) {
        let text = pdf_extract::extract_text_from_mem(bytes)?;
        if text.trim().is_empty() {
            anyhow::bail!("PDF text is empty or could not be extracted");
        }
        return Ok(text);
    }

    match String::from_utf8(bytes.to_vec()) {
        Ok(text) => Ok(text),
        Err(_) => {
            anyhow::bail!("only UTF-8 text attachments and PDF files are supported for now")
        }
    }
}

fn is_pdf_upload(file_name: &str, content_type: Option<&str>) -> bool {
    file_name
        .rsplit('.')
        .next()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        || content_type.is_some_and(|value| value.eq_ignore_ascii_case("application/pdf"))
}

fn upload_mime_type(file_name: &str, content_type: Option<&str>) -> Option<String> {
    if is_pdf_upload(file_name, content_type) {
        return Some("application/pdf".to_string());
    }
    content_type.map(str::to_string)
}

async fn ingest_text_document(
    state: &KnowledgeState,
    job_id: &str,
    kb: &str,
    source: &str,
    text: &str,
    size_bytes: usize,
) -> anyhow::Result<(KnowledgeBaseView, usize)> {
    let Some(item) = state.store.get(kb) else {
        anyhow::bail!("knowledge base not found");
    };

    let document_id = uuid::Uuid::new_v4().to_string();
    let index_source = document_index_source(&document_id, source);
    let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
    let jobs = state.jobs.clone();
    let job_id_for_progress = job_id.to_string();
    let chunks = rag
        .ingest_text_with_progress(kb, &index_source, text, move |progress| {
            update_job(
                &jobs,
                &job_id_for_progress,
                &format!("{:?}", progress.stage).to_ascii_lowercase(),
                &progress.message,
                progress.progress,
                progress.chunks,
            );
        })
        .await?;
    update_job(
        &state.jobs,
        job_id,
        "store",
        "Saving document metadata",
        96,
        Some(chunks),
    );
    let content_path = state.store.store_document_text(kb, &document_id, text)?;
    let document = KnowledgeDocument {
        id: document_id,
        name: source.to_string(),
        source: source.to_string(),
        index_source: Some(index_source),
        size_bytes,
        chunks,
        mime_type: Some("text/plain; charset=utf-8".to_string()),
        content_path: Some(content_path),
        file_path: None,
        created_at: Utc::now(),
    };
    let view = state.store.add_document(kb, document.clone())?;
    record_knowledge_event(
        state,
        "ingest_document",
        format!(
            "Added document `{}` to knowledge base `{}` and indexed {} chunks.",
            document.name, kb, chunks
        ),
        Some(document.id.clone()),
        serde_json::json!({
            "kb": kb,
            "document_id": document.id,
            "document": document.name,
            "source": document.source,
            "chunks": chunks,
            "size_bytes": size_bytes,
        }),
    );
    Ok((view, chunks))
}

struct UploadDocumentInput<'a> {
    job_id: &'a str,
    kb: &'a str,
    source: &'a str,
    size_bytes: usize,
    original_bytes: Vec<u8>,
    content_type: Option<String>,
    mime_type: Option<String>,
}

async fn ingest_upload_document(
    state: &KnowledgeState,
    input: UploadDocumentInput<'_>,
) -> anyhow::Result<(KnowledgeBaseView, usize)> {
    let UploadDocumentInput {
        job_id,
        kb,
        source,
        size_bytes,
        original_bytes,
        content_type,
        mime_type,
    } = input;
    update_job(
        state.jobs.as_ref(),
        job_id,
        "parse",
        "Parsing uploaded file",
        18,
        None,
    );
    let text = extract_upload_text(source, content_type.as_deref(), original_bytes.as_slice())?;
    if text.trim().is_empty() {
        anyhow::bail!("document text is empty");
    }

    let (updated, chunks) =
        ingest_text_document(state, job_id, kb, source, &text, size_bytes).await?;
    let document_id = updated
        .documents
        .first()
        .map(|doc| doc.id.clone())
        .ok_or_else(|| anyhow::anyhow!("ingested document is missing"))?;
    let file_path =
        state
            .store
            .store_document_file(kb, &document_id, source, original_bytes.as_slice())?;
    let updated =
        state
            .store
            .update_document_file_metadata(kb, &document_id, file_path, mime_type)?;
    Ok((updated, chunks))
}

fn error_response(err: impl std::fmt::Display) -> axum::response::Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}

async fn get_document_file(
    State(state): State<KnowledgeState>,
    Path((kb, document_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };
    let Some(document) = item.documents.iter().find(|doc| doc.id == document_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document not found" })),
        )
            .into_response();
    };

    match state.store.document_file(&kb, &document_id) {
        Ok(Some(bytes)) => {
            let content_type = document
                .mime_type
                .as_deref()
                .unwrap_or("application/octet-stream");
            let mut response = Body::from(bytes).into_response();
            let headers = response.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                content_type
                    .parse()
                    .unwrap_or(header::HeaderValue::from_static("application/octet-stream")),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                format!("inline; filename=\"{}\"", document.name.replace('"', ""))
                    .parse()
                    .unwrap_or(header::HeaderValue::from_static("inline")),
            );
            response
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document file not found" })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_document_content(
    State(state): State<KnowledgeState>,
    Path((kb, document_id)): Path<(String, String)>,
) -> impl IntoResponse {
    match state.store.document_text(&kb, &document_id) {
        Ok(Some(text)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "kb": kb, "document_id": document_id, "text": text })),
        )
            .into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document content not found" })),
        )
            .into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn search_knowledge(
    State(state): State<KnowledgeState>,
    Path(kb): Path<String>,
    Json(req): Json<SearchRequest>,
) -> impl IntoResponse {
    if req.query.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "query is empty" })),
        )
            .into_response();
    }

    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };

    let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
    match rag
        .search_for_management(&kb, &req.query, req.top_k.unwrap_or(5))
        .await
    {
        Ok(hits) => {
            record_knowledge_event(
                &state,
                "search",
                format!(
                    "Searched knowledge base `{}` for `{}` and got {} hits.",
                    kb,
                    req.query.trim(),
                    hits.len()
                ),
                Some(kb.clone()),
                serde_json::json!({
                    "kb": kb,
                    "query": req.query,
                    "top_k": req.top_k.unwrap_or(5),
                    "hits": hits.len(),
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({ "kb": kb, "hits": hits })),
            )
                .into_response()
        }
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn get_ingest_job(
    State(state): State<KnowledgeState>,
    Path(job_id): Path<String>,
) -> impl IntoResponse {
    match state.jobs.get(&job_id) {
        Some(job) => (StatusCode::OK, Json(serde_json::json!({ "job": job }))).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "ingestion job not found" })),
        )
            .into_response(),
    }
}

async fn delete_document(
    State(state): State<KnowledgeState>,
    Path((kb, document_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };
    let Some(document) = item
        .documents
        .iter()
        .find(|doc| doc.id == document_id)
        .cloned()
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document not found" })),
        )
            .into_response();
    };

    let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
    if let Err(err) = rag.delete_source(&kb, document.index_source()).await {
        return error_response(err);
    }

    match state.store.delete_document(&kb, &document_id) {
        Ok(Some(_)) => {
            record_knowledge_event(
                &state,
                "delete_document",
                format!(
                    "Deleted document `{}` from knowledge base `{}`.",
                    document.name, kb
                ),
                Some(document_id),
                serde_json::json!({
                    "kb": kb,
                    "document_id": document.id,
                    "document": document.name,
                    "source": document.source,
                }),
            );
            (
                StatusCode::OK,
                Json(serde_json::json!({ "knowledge_base": state.store.get(&kb).map(KnowledgeBaseView::from) })),
            )
                .into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document not found" })),
        )
            .into_response(),
        Err(err) => error_response(err),
    }
}

async fn reindex_document(
    State(state): State<KnowledgeState>,
    Path((kb, document_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };
    let Some(document) = item
        .documents
        .iter()
        .find(|doc| doc.id == document_id)
        .cloned()
    else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document not found" })),
        )
            .into_response();
    };
    let text = match state.store.document_text(&kb, &document_id) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "document content not found" })),
            )
                .into_response();
        }
        Err(err) => return error_response(err),
    };

    let job = state.jobs.create();
    let job_id = job.id.clone();
    let task_state = state.clone();
    tokio::spawn(async move {
        let result = async {
            let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
            let jobs = task_state.jobs.clone();
            let job_id_for_progress = job_id.clone();
            let chunks = rag
                .ingest_text_with_progress(&kb, document.index_source(), &text, move |progress| {
                    update_job(
                        &jobs,
                        &job_id_for_progress,
                        &format!("{:?}", progress.stage).to_ascii_lowercase(),
                        &progress.message,
                        progress.progress,
                        progress.chunks,
                    );
                })
                .await?;
            update_job(
                &task_state.jobs,
                &job_id,
                "store",
                "Updating document metadata",
                96,
                Some(chunks),
            );
            let view = task_state
                .store
                .update_document_chunks(&kb, &document_id, chunks)?;
            record_knowledge_event(
                &task_state,
                "reindex_document",
                format!(
                    "Reindexed document `{}` in knowledge base `{}` into {} chunks.",
                    document.name, kb, chunks
                ),
                Some(document_id.clone()),
                serde_json::json!({
                    "kb": kb,
                    "document_id": document.id,
                    "document": document.name,
                    "chunks": chunks,
                }),
            );
            Ok((view, chunks))
        }
        .await;
        finish_job(&task_state.jobs, &job_id, result);
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "job": job })),
    )
        .into_response()
}

async fn reindex_knowledge_base(
    State(state): State<KnowledgeState>,
    Path(kb): Path<String>,
) -> impl IntoResponse {
    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };

    let job = state.jobs.create();
    let job_id = job.id.clone();
    let task_state = state.clone();
    tokio::spawn(async move {
        let result = async {
            let document_count = item.documents.len();
            let rag = tutor_rag::LanceDbRag::new(task_state.rag_root.clone(), item.embedding);
            let mut total_chunks = 0;

            for (index, document) in item.documents.iter().enumerate() {
                let text = task_state
                    .store
                    .document_text(&kb, &document.id)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "stored content is missing for document `{}`",
                            document.name
                        )
                    })?;
                let jobs = task_state.jobs.clone();
                let job_id_for_progress = job_id.clone();
                let document_name = document.name.clone();
                let progress_base = 10 + (index * 80 / document_count) as u8;
                let progress_span = (80 / document_count).max(1) as u8;
                let chunks = rag
                    .ingest_text_with_progress(
                        &kb,
                        document.index_source(),
                        &text,
                        move |progress| {
                            let mapped = progress_base
                                .saturating_add(
                                    progress_span.saturating_mul(progress.progress) / 100,
                                )
                                .min(95);
                            update_job(
                                &jobs,
                                &job_id_for_progress,
                                &format!("{:?}", progress.stage).to_ascii_lowercase(),
                                &format!("{}: {}", document_name, progress.message),
                                mapped,
                                Some(total_chunks + progress.chunks.unwrap_or(0)),
                            );
                        },
                    )
                    .await?;
                total_chunks += chunks;
                task_state
                    .store
                    .update_document_chunks(&kb, &document.id, chunks)?;
            }

            let view = task_state
                .store
                .get(&kb)
                .map(KnowledgeBaseView::from)
                .ok_or_else(|| anyhow::anyhow!("knowledge base was removed during reindex"))?;
            record_knowledge_event(
                &task_state,
                "reindex_knowledge_base",
                format!(
                    "Reindexed {} document(s) in knowledge base `{}` into {} chunks.",
                    document_count, kb, total_chunks
                ),
                Some(kb.clone()),
                serde_json::json!({
                    "kb": kb,
                    "documents": document_count,
                    "chunks": total_chunks,
                }),
            );
            Ok((view, total_chunks))
        }
        .await;
        finish_job(&task_state.jobs, &job_id, result);
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({ "job": job })),
    )
        .into_response()
}

async fn get_document_chunks(
    State(state): State<KnowledgeState>,
    Path((kb, document_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let Some(item) = state.store.get(&kb) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "knowledge base not found" })),
        )
            .into_response();
    };
    let Some(document) = item.documents.iter().find(|doc| doc.id == document_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "document not found" })),
        )
            .into_response();
    };

    let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), item.embedding);
    match rag
        .chunks_for_source(&kb, document.index_source(), 200)
        .await
    {
        Ok(chunks) => (
            StatusCode::OK,
            Json(serde_json::json!({ "kb": kb, "document_id": document_id, "chunks": chunks })),
        )
            .into_response(),
        Err(err) => error_response(err),
    }
}

impl IngestionJobs {
    fn create(&self) -> IngestionJob {
        let job = IngestionJob {
            id: uuid::Uuid::new_v4().to_string(),
            status: IngestionJobStatus::Queued,
            stage: "queued".into(),
            message: "Queued".into(),
            progress: 0,
            chunks: None,
            knowledge_base: None,
            error: None,
        };
        self.items
            .lock()
            .unwrap()
            .insert(job.id.clone(), job.clone());
        job
    }

    fn get(&self, id: &str) -> Option<IngestionJob> {
        self.items.lock().unwrap().get(id).cloned()
    }
}

fn update_job(
    jobs: &IngestionJobs,
    id: &str,
    stage: &str,
    message: &str,
    progress: u8,
    chunks: Option<usize>,
) {
    if let Some(job) = jobs.items.lock().unwrap().get_mut(id) {
        job.status = IngestionJobStatus::Running;
        job.stage = stage.to_string();
        job.message = message.to_string();
        job.progress = progress.min(99);
        job.chunks = chunks.or(job.chunks);
    }
}

fn finish_job(jobs: &IngestionJobs, id: &str, result: anyhow::Result<(KnowledgeBaseView, usize)>) {
    if let Some(job) = jobs.items.lock().unwrap().get_mut(id) {
        match result {
            Ok((knowledge_base, chunks)) => {
                job.status = IngestionJobStatus::Done;
                job.stage = "done".into();
                job.message = "Done".into();
                job.progress = 100;
                job.chunks = Some(chunks);
                job.knowledge_base = Some(knowledge_base);
                job.error = None;
            }
            Err(err) => {
                job.status = IngestionJobStatus::Error;
                job.stage = "error".into();
                job.message = err.to_string();
                job.error = Some(err.to_string());
            }
        }
    }
}

fn document_index_source(document_id: &str, source: &str) -> String {
    format!("{document_id}::{source}")
}

trait DocumentIndexSource {
    fn index_source(&self) -> &str;
}

impl DocumentIndexSource for KnowledgeDocument {
    fn index_source(&self) -> &str {
        self.index_source.as_deref().unwrap_or(&self.source)
    }
}

fn record_knowledge_event(
    state: &KnowledgeState,
    action: impl Into<String>,
    summary: impl Into<String>,
    source_id: Option<String>,
    payload: serde_json::Value,
) {
    let _ = state.memory.record_event(
        MemoryEventCategory::Knowledge,
        action,
        summary,
        source_id,
        payload,
    );
}

pub fn knowledge_router(
    store: Arc<KnowledgeStore>,
    memory: Arc<MemoryStore>,
    rag_root: impl Into<PathBuf>,
) -> Router {
    let state = KnowledgeState {
        store,
        jobs: Arc::new(IngestionJobs::default()),
        memory,
        rag_root: rag_root.into(),
    };
    Router::new()
        .route("/api/attachments/parse", post(parse_chat_attachment))
        .route(
            "/api/knowledge-bases",
            get(list_knowledge_bases).post(create_knowledge_base),
        )
        .route("/api/knowledge-bases/{kb}", delete(delete_knowledge_base))
        .route(
            "/api/knowledge-bases/{kb}/reindex",
            post(reindex_knowledge_base),
        )
        .route("/api/ingest-jobs/{job_id}", get(get_ingest_job))
        .route("/api/knowledge-bases/{kb}/documents", post(ingest_document))
        .route(
            "/api/knowledge-bases/{kb}/documents/upload",
            post(upload_document),
        )
        .route(
            "/api/knowledge-bases/{kb}/documents/{document_id}/content",
            get(get_document_content),
        )
        .route(
            "/api/knowledge-bases/{kb}/documents/{document_id}/file",
            get(get_document_file),
        )
        .route(
            "/api/knowledge-bases/{kb}/documents/{document_id}",
            delete(delete_document),
        )
        .route(
            "/api/knowledge-bases/{kb}/documents/{document_id}/reindex",
            post(reindex_document),
        )
        .route(
            "/api/knowledge-bases/{kb}/documents/{document_id}/chunks",
            get(get_document_chunks),
        )
        .route("/api/knowledge-bases/{kb}/search", post(search_knowledge))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    #[test]
    fn detects_pdf_upload_by_extension_or_content_type() {
        assert!(is_pdf_upload("lesson.PDF", None));
        assert!(is_pdf_upload("lesson.bin", Some("application/pdf")));
        assert!(!is_pdf_upload("lesson.txt", Some("text/plain")));
    }

    #[test]
    fn extracts_utf8_text_upload() {
        let text = extract_upload_text("lesson.md", Some("text/markdown"), b"# Title").unwrap();
        assert_eq!(text, "# Title");
    }

    #[test]
    fn rejects_non_utf8_non_pdf_upload() {
        let err = extract_upload_text("lesson.bin", None, &[0xff, 0xfe]).unwrap_err();
        assert!(
            err.to_string()
                .contains("UTF-8 text attachments and PDF files")
        );
    }

    #[tokio::test]
    async fn parses_chat_attachment_upload() {
        let root = tempfile::tempdir().unwrap();
        let store = KnowledgeStore::new_with_path(root.path().join("knowledge-bases.json"));
        let memory = Arc::new(MemoryStore::new_with_root(root.path().join("memory")));
        let app = knowledge_router(store, memory.clone(), root.path().join("rag"));

        let boundary = "X-LLM-TUTOR-ATTACHMENT";
        let upload_body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"notes.txt\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             alpha beta gamma\r\n\
             --{boundary}--\r\n"
        );
        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/attachments/parse")
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(upload_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        assert_eq!(body["attachment"]["name"], "notes.txt");
        assert_eq!(body["attachment"]["text"], "alpha beta gamma");
    }

    #[tokio::test]
    async fn upload_search_and_chunks_work_without_real_llm() {
        let root = tempfile::tempdir().unwrap();
        let store = KnowledgeStore::new_with_path(root.path().join("knowledge-bases.json"));
        let memory = Arc::new(MemoryStore::new_with_root(root.path().join("memory")));
        let app = knowledge_router(store, memory.clone(), root.path().join("rag"));

        let create_body = serde_json::json!({
            "name": "Physics",
            "embedding": {
                "provider": "hash",
                "model": "local-hash",
                "api_key": "test",
                "base_url": null,
                "embeddings_path": null,
                "dimensions": 32,
                "send_dimensions": false
            }
        });
        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                "/api/knowledge-bases",
                create_body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = response_json(response).await;
        let kb = body["knowledge_base"]["id"].as_str().unwrap().to_string();

        let boundary = "X-LLM-TUTOR-BOUNDARY";
        let upload_body = format!(
            "--{boundary}\r\n\
             Content-Disposition: form-data; name=\"file\"; filename=\"lesson.txt\"\r\n\
             Content-Type: text/plain\r\n\r\n\
             lithography photoresist wafer mask exposure alignment overlay\r\n\
             --{boundary}--\r\n"
        );
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/knowledge-bases/{kb}/documents/upload"))
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from(upload_body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = response_json(response).await;
        let job_id = body["job"]["id"].as_str().unwrap();

        let mut completed = None;
        for _ in 0..20 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(format!("/api/ingest-jobs/{job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = response_json(response).await;
            if body["job"]["status"] == "done" {
                completed = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let completed = completed.expect("ingestion job should complete");
        let document_id = completed["job"]["knowledge_base"]["documents"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();

        let response = app
            .clone()
            .oneshot(json_request(
                Method::POST,
                &format!("/api/knowledge-bases/{kb}/search"),
                serde_json::json!({ "query": "photoresist wafer", "top_k": 3 }),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let hits = body["hits"].as_array().unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0]["source"], "lesson.txt");

        let events = memory.recent_events(20).unwrap();
        assert!(events.iter().any(|event| {
            event.category == MemoryEventCategory::Knowledge && event.action == "ingest_document"
        }));
        assert!(events.iter().any(|event| {
            event.category == MemoryEventCategory::Knowledge && event.action == "search"
        }));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("/api/knowledge-bases/{kb}/reindex"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = response_json(response).await;
        let rebuild_job_id = body["job"]["id"].as_str().unwrap();
        let mut rebuilt = None;
        for _ in 0..20 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::GET)
                        .uri(format!("/api/ingest-jobs/{rebuild_job_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let body = response_json(response).await;
            if body["job"]["status"] == "done" {
                rebuilt = Some(body);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let rebuilt = rebuilt.expect("knowledge base rebuild should complete");
        assert_eq!(
            rebuilt["job"]["knowledge_base"]["documents"][0]["id"],
            document_id
        );

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!(
                        "/api/knowledge-bases/{kb}/documents/{document_id}/chunks"
                    ))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response_json(response).await;
        let chunks = body["chunks"].as_array().unwrap();
        assert!(!chunks.is_empty());
        assert!(chunks[0]["text"].as_str().unwrap().contains("photoresist"));
    }

    fn json_request(method: Method, uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn response_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
