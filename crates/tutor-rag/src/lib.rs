use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use arrow_array::cast::AsArray;
use arrow_array::types::Float32Type;
use arrow_array::{
    ArrayRef, FixedSizeListArray, Int32Array, RecordBatch, RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase, Select};
use llm_adapter::EmbeddingProvider;
use llm_adapter::openai::OpenAIProvider;
use llm_adapter::types::EmbeddingRequest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod knowledge_source;

pub use knowledge_source::{
    COURSE_KNOWLEDGE_NAMESPACE, COURSE_KNOWLEDGE_SOURCE_ID, KNOWLEDGE_BASE_SCOPE_ATTRIBUTE,
    LanceDbKnowledgeSource,
};

const TABLE_NAME: &str = "knowledge_chunks_v1";
const CHUNK_SCHEMA_VERSION: i32 = 1;
const DEFAULT_TOP_K: usize = 5;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub embeddings_path: Option<String>,
    pub dimensions: Option<usize>,
    pub send_dimensions: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SearchHit {
    pub id: String,
    pub kb: String,
    pub source: String,
    pub raw_source: String,
    pub document_id: Option<String>,
    pub text: String,
    pub score: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SourceChunk {
    pub id: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub(crate) struct KnowledgeRow {
    pub item_id: String,
    pub revision: String,
    pub kb: String,
    pub document_id: String,
    pub chunk_id: String,
    pub source: String,
    pub title: String,
    pub uri: String,
    pub text: String,
    pub score: Option<f32>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct IngestProgress {
    pub stage: IngestStage,
    pub message: String,
    pub progress: u8,
    pub chunks: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IngestStage {
    Chunk,
    Embed,
    Index,
    Done,
}

#[derive(Clone)]
pub struct LanceDbRag {
    root: PathBuf,
    embedding: EmbeddingConfig,
}

impl LanceDbRag {
    pub fn new(root: impl Into<PathBuf>, embedding: EmbeddingConfig) -> Self {
        Self {
            root: root.into(),
            embedding,
        }
    }

    pub fn default_root() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".llm-tutor")
            .join("rag")
    }

    pub async fn search_for_management(
        &self,
        kb: &str,
        query: &str,
        top_k: usize,
    ) -> Result<Vec<SearchHit>> {
        if kb.trim().is_empty() {
            return Ok(vec![]);
        }
        let rows = self
            .search_rows(kb, query, if top_k == 0 { DEFAULT_TOP_K } else { top_k })
            .await?;
        Ok(rows.into_iter().map(SearchHit::from).collect())
    }

    pub async fn ingest_text(&self, kb: &str, source: &str, text: &str) -> Result<usize> {
        self.ingest_text_with_progress(kb, source, text, |_| {})
            .await
    }

    pub async fn ingest_text_with_progress(
        &self,
        kb: &str,
        source: &str,
        text: &str,
        mut on_progress: impl FnMut(IngestProgress),
    ) -> Result<usize> {
        on_progress(IngestProgress {
            stage: IngestStage::Chunk,
            message: "Splitting document into chunks".into(),
            progress: 25,
            chunks: None,
        });
        let chunks = chunk_text(text, 900, 160);
        if chunks.is_empty() {
            self.delete_source(kb, source).await?;
            on_progress(IngestProgress {
                stage: IngestStage::Done,
                message: "Document did not produce chunks".into(),
                progress: 100,
                chunks: Some(0),
            });
            return Ok(0);
        }
        let chunk_count = chunks.len();

        on_progress(IngestProgress {
            stage: IngestStage::Embed,
            message: format!("Embedding {chunk_count} chunks"),
            progress: 45,
            chunks: Some(chunk_count),
        });
        let vectors = self.embed_texts(chunks.clone()).await?;

        on_progress(IngestProgress {
            stage: IngestStage::Index,
            message: "Writing chunks to LanceDB".into(),
            progress: 78,
            chunks: Some(chunk_count),
        });
        let batch = chunks_to_batch(kb, source, chunks, vectors)?;
        let db = connect_db(&self.root).await?;

        match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => {
                let schema = batch.schema();
                let reader = batches_reader(schema, vec![batch]);
                let mut merge = table.merge_insert(&["item_id"]);
                merge.when_matched_update_all(None);
                merge.when_not_matched_insert_all();
                merge.when_not_matched_by_source_delete(Some(source_predicate(kb, source)));
                merge.execute(reader).await?;
            }
            Err(_) => {
                db.create_table(TABLE_NAME, batch).execute().await?;
            }
        }

        on_progress(IngestProgress {
            stage: IngestStage::Done,
            message: "Indexed document".into(),
            progress: 92,
            chunks: Some(chunk_count),
        });
        Ok(chunk_count)
    }

    pub async fn delete_source(&self, kb: &str, source: &str) -> Result<usize> {
        let db = connect_db(&self.root).await?;
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => table,
            Err(_) => return Ok(0),
        };
        let predicate = source_predicate(kb, source);
        let before = table.count_rows(Some(predicate.clone())).await?;
        if before > 0 {
            table.delete(&predicate).await?;
        }
        Ok(before)
    }

    pub async fn delete_kb(&self, kb: &str) -> Result<usize> {
        let db = connect_db(&self.root).await?;
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => table,
            Err(_) => return Ok(0),
        };
        let predicate = kb_predicate(kb);
        let before = table.count_rows(Some(predicate.clone())).await?;
        if before > 0 {
            table.delete(&predicate).await?;
        }
        Ok(before)
    }

    pub async fn chunks_for_source(
        &self,
        kb: &str,
        source: &str,
        limit: usize,
    ) -> Result<Vec<SourceChunk>> {
        let db = connect_db(&self.root).await?;
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => table,
            Err(_) => return Ok(vec![]),
        };

        let batches = table
            .query()
            .select(Select::columns(&["item_id", "text"]))
            .only_if(source_predicate(kb, source))
            .limit(if limit == 0 { 100 } else { limit })
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        Ok(source_chunks_from_batches(&batches))
    }

    async fn embed_texts(&self, input: Vec<String>) -> Result<Vec<Vec<f32>>> {
        if self.embedding.provider == "hash" || self.embedding.provider == "local-test" {
            let dimensions = self.embedding.dimensions.unwrap_or(32).max(8);
            return Ok(input
                .iter()
                .map(|text| hash_embedding(text, dimensions))
                .collect());
        }

        if self.embedding.provider != "openai" {
            return Err(anyhow!(
                "unsupported embedding provider `{}`",
                self.embedding.provider
            ));
        }

        let mut builder = OpenAIProvider::builder(self.embedding.api_key.clone());
        if let Some(base_url) = &self.embedding.base_url {
            builder = builder.base_url(base_url.clone());
        }
        if let Some(path) = &self.embedding.embeddings_path {
            builder = builder.embeddings_path(path.clone());
        }
        let provider = builder.build();

        let mut req = EmbeddingRequest::builder(self.embedding.model.clone()).inputs(input);
        if self.embedding.send_dimensions
            && let Some(dimensions) = self.embedding.dimensions
        {
            req = req.dimensions(dimensions);
        }

        let response = provider.embed(&req.build()).await?;
        let mut vectors = response.vectors;
        vectors.sort_by_key(|vector| vector.index);
        Ok(vectors.into_iter().map(|vector| vector.embedding).collect())
    }

    pub(crate) async fn search_rows(
        &self,
        kb: &str,
        query_text: &str,
        limit: usize,
    ) -> Result<Vec<KnowledgeRow>> {
        let mut vectors = self.embed_texts(vec![query_text.to_string()]).await?;
        let Some(query_vector) = vectors.pop() else {
            return Ok(vec![]);
        };

        let db = connect_db(&self.root).await?;
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => table,
            Err(_) => return Ok(vec![]),
        };

        let batches = table
            .query()
            .select(Select::columns(&[
                "item_id",
                "revision",
                "kb",
                "document_id",
                "chunk_id",
                "source",
                "title",
                "uri",
                "text",
            ]))
            .limit(limit)
            .nearest_to(query_vector.as_slice())?
            .column("vector")
            .only_if(kb_predicate(kb))
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        Ok(knowledge_rows_from_batches(&batches))
    }

    pub(crate) async fn row_by_item(
        &self,
        kb: &str,
        item_id: &str,
    ) -> Result<Option<KnowledgeRow>> {
        let db = connect_db(&self.root).await?;
        let table = match db.open_table(TABLE_NAME).execute().await {
            Ok(table) => table,
            Err(_) => return Ok(None),
        };

        let predicate = format!(
            "kb = '{}' AND item_id = '{}'",
            escape_sql_string(kb),
            escape_sql_string(item_id)
        );
        let batches = table
            .query()
            .select(Select::columns(&[
                "item_id",
                "revision",
                "kb",
                "document_id",
                "chunk_id",
                "source",
                "title",
                "uri",
                "text",
            ]))
            .only_if(predicate)
            .limit(1)
            .execute()
            .await?
            .try_collect::<Vec<_>>()
            .await?;
        Ok(knowledge_rows_from_batches(&batches).into_iter().next())
    }
}

impl From<KnowledgeRow> for SearchHit {
    fn from(row: KnowledgeRow) -> Self {
        Self {
            id: row.item_id,
            kb: row.kb,
            source: row.title,
            raw_source: row.source,
            document_id: Some(row.document_id),
            text: row.text,
            score: row.score,
        }
    }
}

async fn connect_db(root: &Path) -> Result<lancedb::Connection> {
    tokio::fs::create_dir_all(root).await?;
    let uri = root
        .to_str()
        .ok_or_else(|| anyhow!("RAG database path is not valid UTF-8"))?;
    Ok(lancedb::connect(uri).execute().await?)
}

fn batches_reader(
    schema: SchemaRef,
    batches: Vec<RecordBatch>,
) -> Box<dyn arrow_array::RecordBatchReader + Send> {
    Box::new(RecordBatchIterator::new(
        batches.into_iter().map(Ok::<_, arrow_schema::ArrowError>),
        schema,
    ))
}

fn chunks_to_batch(
    kb: &str,
    source: &str,
    chunks: Vec<String>,
    vectors: Vec<Vec<f32>>,
) -> Result<RecordBatch> {
    if chunks.len() != vectors.len() {
        return Err(anyhow!("embedding count does not match chunk count"));
    }
    let dimensions = vectors
        .first()
        .map(|vector| vector.len())
        .ok_or_else(|| anyhow!("embedding response did not contain vectors"))?;
    if dimensions == 0 {
        return Err(anyhow!("embedding vectors must not be empty"));
    }
    if vectors.iter().any(|vector| vector.len() != dimensions) {
        return Err(anyhow!("embedding vectors have inconsistent dimensions"));
    }

    let document_id = source_document_id(source).unwrap_or_else(|| source.to_string());
    let title = display_source(source);
    let item_ids = StringArray::from_iter_values(
        chunks
            .iter()
            .enumerate()
            .map(|(ordinal, _)| stable_item_id(kb, &document_id, ordinal)),
    );
    let revisions = StringArray::from_iter_values(
        chunks
            .iter()
            .enumerate()
            .map(|(ordinal, text)| chunk_revision(kb, &document_id, ordinal, text)),
    );
    let kbs = StringArray::from_iter_values(std::iter::repeat_n(kb.to_string(), chunks.len()));
    let document_ids =
        StringArray::from_iter_values(std::iter::repeat_n(document_id.clone(), chunks.len()));
    let chunk_ids = StringArray::from_iter_values((0..chunks.len()).map(chunk_id));
    let chunk_ordinals = Int32Array::from_iter_values(0..chunks.len() as i32);
    let sources =
        StringArray::from_iter_values(std::iter::repeat_n(source.to_string(), chunks.len()));
    let titles = StringArray::from_iter_values(std::iter::repeat_n(title.clone(), chunks.len()));
    let uris = StringArray::from_iter_values(
        (0..chunks.len()).map(|ordinal| knowledge_uri(kb, &document_id, ordinal)),
    );
    let schema_versions =
        Int32Array::from_iter_values(std::iter::repeat_n(CHUNK_SCHEMA_VERSION, chunks.len()));
    let texts = StringArray::from_iter_values(chunks);
    let vectors = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        vectors
            .into_iter()
            .map(|vector| Some(vector.into_iter().map(Some).collect::<Vec<_>>())),
        dimensions as i32,
    );

    let schema = Arc::new(Schema::new(vec![
        Field::new("item_id", DataType::Utf8, false),
        Field::new("revision", DataType::Utf8, false),
        Field::new("kb", DataType::Utf8, false),
        Field::new("document_id", DataType::Utf8, false),
        Field::new("chunk_id", DataType::Utf8, false),
        Field::new("chunk_ordinal", DataType::Int32, false),
        Field::new("source", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("uri", DataType::Utf8, false),
        Field::new("schema_version", DataType::Int32, false),
        Field::new("text", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensions as i32,
            ),
            false,
        ),
    ]));

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(item_ids) as ArrayRef,
            Arc::new(revisions) as ArrayRef,
            Arc::new(kbs) as ArrayRef,
            Arc::new(document_ids) as ArrayRef,
            Arc::new(chunk_ids) as ArrayRef,
            Arc::new(chunk_ordinals) as ArrayRef,
            Arc::new(sources) as ArrayRef,
            Arc::new(titles) as ArrayRef,
            Arc::new(uris) as ArrayRef,
            Arc::new(schema_versions) as ArrayRef,
            Arc::new(texts) as ArrayRef,
            Arc::new(vectors) as ArrayRef,
        ],
    )
    .context("failed to build RAG record batch")
}

fn knowledge_rows_from_batches(batches: &[RecordBatch]) -> Vec<KnowledgeRow> {
    let mut rows = Vec::new();
    for batch in batches {
        let Some(item_ids) = batch
            .column_by_name("item_id")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(revisions) = batch
            .column_by_name("revision")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(kbs) = batch
            .column_by_name("kb")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(document_ids) = batch
            .column_by_name("document_id")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(chunk_ids) = batch
            .column_by_name("chunk_id")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(sources) = batch
            .column_by_name("source")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(titles) = batch
            .column_by_name("title")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(uris) = batch
            .column_by_name("uri")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(texts) = batch
            .column_by_name("text")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let scores = batch
            .column_by_name("_distance")
            .and_then(|array| array.as_primitive_opt::<Float32Type>());

        for row in 0..batch.num_rows() {
            rows.push(KnowledgeRow {
                item_id: item_ids.value(row).to_string(),
                revision: revisions.value(row).to_string(),
                kb: kbs.value(row).to_string(),
                document_id: document_ids.value(row).to_string(),
                chunk_id: chunk_ids.value(row).to_string(),
                source: sources.value(row).to_string(),
                title: titles.value(row).to_string(),
                uri: uris.value(row).to_string(),
                text: texts.value(row).to_string(),
                score: scores.map(|array| array.value(row)),
            });
        }
    }
    rows
}

fn source_chunks_from_batches(batches: &[RecordBatch]) -> Vec<SourceChunk> {
    let mut chunks = Vec::new();
    for batch in batches {
        let Some(ids) = batch
            .column_by_name("item_id")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        let Some(texts) = batch
            .column_by_name("text")
            .and_then(|array| array.as_string_opt::<i32>())
        else {
            continue;
        };
        for row in 0..batch.num_rows() {
            chunks.push(SourceChunk {
                id: ids.value(row).to_string(),
                text: texts.value(row).to_string(),
            });
        }
    }
    chunks
}

fn chunk_text(text: &str, max_chars: usize, overlap_chars: usize) -> Vec<String> {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return vec![];
    }

    let chars = normalized.chars().collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + max_chars).min(chars.len());
        chunks.push(chars[start..end].iter().collect::<String>());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(overlap_chars);
    }
    chunks
}

fn source_predicate(kb: &str, source: &str) -> String {
    format!(
        "kb = '{}' AND source = '{}'",
        escape_sql_string(kb),
        escape_sql_string(source)
    )
}

fn kb_predicate(kb: &str) -> String {
    format!("kb = '{}'", escape_sql_string(kb))
}

fn escape_sql_string(value: &str) -> String {
    value.replace('\'', "''")
}

fn display_source(value: &str) -> String {
    value
        .split_once("::")
        .map(|(_, source)| source.to_string())
        .unwrap_or_else(|| value.to_string())
}

fn source_document_id(value: &str) -> Option<String> {
    value
        .split_once("::")
        .map(|(document_id, _)| document_id.trim().to_string())
        .filter(|document_id| !document_id.is_empty())
}

fn stable_item_id(kb: &str, document_id: &str, ordinal: usize) -> String {
    format!(
        "chunk_{}",
        digest_parts(&[
            &CHUNK_SCHEMA_VERSION.to_string(),
            kb,
            document_id,
            &ordinal.to_string(),
        ])
    )
}

fn chunk_revision(kb: &str, document_id: &str, ordinal: usize, text: &str) -> String {
    format!(
        "sha256:{}",
        digest_parts(&[
            &CHUNK_SCHEMA_VERSION.to_string(),
            kb,
            document_id,
            &ordinal.to_string(),
            text,
        ])
    )
}

fn chunk_id(ordinal: usize) -> String {
    format!("chunk-{ordinal}")
}

fn knowledge_uri(kb: &str, document_id: &str, ordinal: usize) -> String {
    format!("kb:{kb}:{document_id}:{}", chunk_id(ordinal))
}

fn digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.len().to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn hash_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vector = vec![0.0; dimensions];
    for token in text
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|token| !token.is_empty())
    {
        let mut hash = 0xcbf29ce484222325_u64;
        for byte in token.to_lowercase().bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        let index = (hash as usize) % dimensions;
        vector[index] += 1.0;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_config() -> EmbeddingConfig {
        EmbeddingConfig {
            provider: "hash".into(),
            model: "test".into(),
            api_key: String::new(),
            base_url: None,
            embeddings_path: None,
            dimensions: Some(32),
            send_dimensions: false,
        }
    }

    #[test]
    fn chunk_text_splits_with_overlap() {
        let chunks = chunk_text("abcdefghijklmnopqrstuvwxyz", 10, 2);
        assert_eq!(chunks[0], "abcdefghij");
        assert_eq!(chunks[1], "ijklmnopqr");
    }

    #[test]
    fn chunks_to_batch_requires_matching_vectors() {
        let err = chunks_to_batch("default", "doc", vec!["hello".into()], vec![]).unwrap_err();
        assert!(err.to_string().contains("embedding count"));
    }

    #[tokio::test]
    async fn reindex_keeps_item_ids_and_revises_changed_content() {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        let source = "document-1::lesson.md";
        rag.ingest_text("kb-a", source, &"alpha ".repeat(300))
            .await
            .unwrap();
        let mut before = rag.chunks_for_source("kb-a", source, 100).await.unwrap();
        before.sort_by(|left, right| left.id.cmp(&right.id));
        let before_revisions = futures::future::try_join_all(
            before
                .iter()
                .map(|chunk| rag.row_by_item("kb-a", &chunk.id)),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.unwrap().revision)
        .collect::<Vec<_>>();

        rag.ingest_text("kb-a", source, &"bravo ".repeat(300))
            .await
            .unwrap();
        let mut after = rag.chunks_for_source("kb-a", source, 100).await.unwrap();
        after.sort_by(|left, right| left.id.cmp(&right.id));
        let after_revisions = futures::future::try_join_all(
            after.iter().map(|chunk| rag.row_by_item("kb-a", &chunk.id)),
        )
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.unwrap().revision)
        .collect::<Vec<_>>();

        assert_eq!(
            before.iter().map(|chunk| &chunk.id).collect::<Vec<_>>(),
            after.iter().map(|chunk| &chunk.id).collect::<Vec<_>>()
        );
        assert_ne!(before_revisions, after_revisions);
    }

    #[tokio::test]
    async fn reindex_removes_chunks_that_are_no_longer_in_the_document() {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        let source = "document-1::lesson.md";
        rag.ingest_text("kb-a", source, &"long ".repeat(600))
            .await
            .unwrap();
        assert!(
            rag.chunks_for_source("kb-a", source, 100)
                .await
                .unwrap()
                .len()
                > 1
        );

        rag.ingest_text("kb-a", source, "short replacement")
            .await
            .unwrap();
        let chunks = rag.chunks_for_source("kb-a", source, 100).await.unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, "short replacement");
    }
}
