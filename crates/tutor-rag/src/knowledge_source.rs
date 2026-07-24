use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    ContentSelector, FreshnessClass, KnowledgeCapability, KnowledgeContent, KnowledgeError,
    KnowledgeHit, KnowledgeReadRequest, KnowledgeRef, KnowledgeRequestContext, KnowledgeSource,
    KnowledgeSourceDescriptor, SourceSearchPage, SourceSearchRequest,
};
use llm_harness_types::DataBlock;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::{KnowledgeRow, LanceDbRag};

pub const COURSE_KNOWLEDGE_SOURCE_ID: &str = "course_knowledge";
pub const COURSE_KNOWLEDGE_NAMESPACE: &str = "llm-tutor.course-knowledge";
pub const KNOWLEDGE_BASE_SCOPE_ATTRIBUTE: &str = "knowledge_base_id";

const MAX_SEARCH_LIMIT: usize = 50;
const MAX_SNIPPET_BYTES: usize = 600;

#[derive(Clone)]
pub struct LanceDbKnowledgeSource {
    rag: LanceDbRag,
    knowledge_base_id: String,
    descriptor: KnowledgeSourceDescriptor,
}

impl LanceDbKnowledgeSource {
    pub fn new(rag: LanceDbRag, knowledge_base_id: impl Into<String>) -> Self {
        Self {
            rag,
            knowledge_base_id: knowledge_base_id.into(),
            descriptor: KnowledgeSourceDescriptor {
                id: COURSE_KNOWLEDGE_SOURCE_ID.into(),
                name: "Course knowledge".into(),
                description: "The course knowledge base selected for this conversation.".into(),
                domains: vec!["course".into()],
                capabilities: BTreeSet::from([
                    KnowledgeCapability::Search,
                    KnowledgeCapability::Read,
                    KnowledgeCapability::Revisioned,
                    KnowledgeCapability::ChunkRead,
                ]),
                freshness: FreshnessClass::NearRealTime,
                filter_fields: vec![],
            },
        }
    }

    pub fn knowledge_base_id(&self) -> &str {
        &self.knowledge_base_id
    }

    fn authorize(
        &self,
        ctx: KnowledgeRequestContext<'_>,
        abort: &CancellationToken,
    ) -> Result<(), KnowledgeError> {
        if abort.is_cancelled() {
            return Err(KnowledgeError::Aborted);
        }
        let scoped_kb = ctx
            .access
            .scope
            .attributes
            .get(KNOWLEDGE_BASE_SCOPE_ATTRIBUTE);
        if ctx.access.scope.namespace != COURSE_KNOWLEDGE_NAMESPACE
            || scoped_kb.map(String::as_str) != Some(self.knowledge_base_id.as_str())
        {
            return Err(KnowledgeError::Unauthorized);
        }
        Ok(())
    }

    fn reference(&self, row: &KnowledgeRow) -> KnowledgeRef {
        KnowledgeRef {
            source_id: COURSE_KNOWLEDGE_SOURCE_ID.into(),
            item_id: row.item_id.clone(),
            revision: Some(row.revision.clone()),
        }
    }
}

impl KnowledgeSource for LanceDbKnowledgeSource {
    fn descriptor(&self) -> &KnowledgeSourceDescriptor {
        &self.descriptor
    }

    fn search<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: SourceSearchRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<SourceSearchPage, KnowledgeError>> {
        Box::pin(async move {
            self.authorize(ctx, &abort)?;
            if request.cursor.is_some() {
                return Err(KnowledgeError::InvalidCursor);
            }
            if !request.filters.is_empty() {
                return Err(KnowledgeError::InvalidFilter(
                    "course knowledge does not accept model-provided filters".into(),
                ));
            }
            if request.limit == 0 || request.query.trim().is_empty() {
                return Ok(SourceSearchPage {
                    hits: vec![],
                    next_cursor: None,
                });
            }

            let rows = tokio::select! {
                _ = abort.cancelled() => return Err(KnowledgeError::Aborted),
                result = self.rag.search_rows(
                    &self.knowledge_base_id,
                    request.query.trim(),
                    request.limit.min(MAX_SEARCH_LIMIT),
                ) => result.map_err(backend_error)?,
            };
            self.authorize(ctx, &abort)?;

            let hits = rows
                .into_iter()
                .map(|row| {
                    let (snippet, _) = truncate_utf8(&row.text, MAX_SNIPPET_BYTES);
                    let mut metadata = BTreeMap::new();
                    metadata.insert("document_id".into(), json!(row.document_id));
                    metadata.insert("chunk_id".into(), json!(row.chunk_id));
                    metadata.insert("knowledge_base_id".into(), json!(row.kb));
                    KnowledgeHit {
                        reference: self.reference(&row),
                        title: Some(row.title),
                        snippet,
                        suggested_selectors: vec![ContentSelector::Chunks {
                            ids: vec![row.chunk_id],
                        }],
                        uri: Some(row.uri),
                        score: row.score.map(distance_to_score),
                        updated_at: None,
                        metadata,
                    }
                })
                .collect();

            Ok(SourceSearchPage {
                hits,
                next_cursor: None,
            })
        })
    }

    fn read<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: KnowledgeReadRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<KnowledgeContent, KnowledgeError>> {
        Box::pin(async move {
            self.authorize(ctx, &abort)?;
            if request.reference.source_id != COURSE_KNOWLEDGE_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }

            let row = tokio::select! {
                _ = abort.cancelled() => return Err(KnowledgeError::Aborted),
                result = self.rag.row_by_item(
                    &self.knowledge_base_id,
                    &request.reference.item_id,
                ) => result.map_err(backend_error)?,
            }
            .ok_or(KnowledgeError::NotFound)?;
            self.authorize(ctx, &abort)?;

            let latest = self.reference(&row);
            if request.reference.revision.as_deref() != Some(row.revision.as_str()) {
                return Err(KnowledgeError::StaleReference {
                    latest: Some(latest),
                });
            }

            match &request.selector {
                ContentSelector::Document => {}
                ContentSelector::Chunks { ids }
                    if ids.len() == 1 && ids.first() == Some(&row.chunk_id) => {}
                ContentSelector::Chunks { .. } => return Err(KnowledgeError::NotFound),
                ContentSelector::Sections { .. } | ContentSelector::LineRange { .. } => {
                    return Err(KnowledgeError::UnsupportedCapability(
                        "course knowledge supports document and exact chunk reads".into(),
                    ));
                }
            }

            let (text, truncated) = truncate_utf8(&row.text, request.max_bytes);
            let mut metadata = BTreeMap::new();
            metadata.insert("document_id".into(), json!(row.document_id));
            metadata.insert("chunk_id".into(), json!(row.chunk_id));
            metadata.insert("knowledge_base_id".into(), json!(row.kb));

            Ok(KnowledgeContent {
                reference: request.reference,
                selector: request.selector,
                title: Some(row.title),
                blocks: vec![DataBlock::text(text)],
                uri: Some(row.uri),
                updated_at: None,
                obtained_at: Utc::now(),
                truncated,
                metadata,
            })
        })
    }
}

fn backend_error(error: anyhow::Error) -> KnowledgeError {
    KnowledgeError::Backend(format!("{error:#}"))
}

fn distance_to_score(distance: f32) -> f32 {
    1.0 / (1.0 + distance.max(0.0))
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use llm_harness_runtime_knowledge::contract::{SourceContractCase, verify_source_contract};
    use llm_harness_runtime_knowledge::{
        KnowledgeAccessContext, KnowledgeErrorCode, KnowledgeScope, PrincipalRef,
    };
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;
    use crate::EmbeddingConfig;

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

    fn access(kb: &str) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(COURSE_KNOWLEDGE_NAMESPACE);
        scope
            .attributes
            .insert(KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(), kb.into());
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-test-user", "test"))
    }

    async fn fixture() -> (
        tempfile::TempDir,
        LanceDbKnowledgeSource,
        KnowledgeAccessContext,
        RunContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        rag.ingest_text(
            "kb-a",
            "document-1::refunds.md",
            "Refund requests are accepted for thirty days after purchase.",
        )
        .await
        .unwrap();
        let allowed = access("kb-a");
        let run =
            RunContext::new(RunRequest::from_text("refund policy").with_extension(allowed.clone()));
        (temp, LanceDbKnowledgeSource::new(rag, "kb-a"), allowed, run)
    }

    const REPRESENTATIVE_DOCUMENTS: [(&str, &str); 5] = [
        (
            "refund-policy::refund-policy.md",
            "The refund policy accepts refund requests within thirty days after purchase. \
             Customers must provide the original receipt and order number.",
        ),
        (
            "newton-laws::newton-laws.md",
            "Newton's laws explain how force changes motion. The second law relates force, \
             mass, and acceleration.",
        ),
        (
            "photosynthesis::photosynthesis.md",
            "Photosynthesis uses chlorophyll and sunlight to convert carbon dioxide and water \
             into glucose and oxygen.",
        ),
        (
            "tcp-handshake::tcp-handshake.md",
            "A TCP connection begins with the three-way handshake: SYN, SYN-ACK, and ACK.",
        ),
        (
            "calculus-derivative::calculus-derivative.md",
            "A derivative in calculus describes an instantaneous rate of change and the slope \
             of a tangent line.",
        ),
    ];

    const REPRESENTATIVE_QUERIES: [(&str, &str); 5] = [
        (
            "refund receipt thirty days",
            "refund-policy::refund-policy.md",
        ),
        ("force mass acceleration", "newton-laws::newton-laws.md"),
        (
            "chlorophyll sunlight glucose",
            "photosynthesis::photosynthesis.md",
        ),
        ("SYN SYN-ACK ACK", "tcp-handshake::tcp-handshake.md"),
        (
            "derivative rate change tangent",
            "calculus-derivative::calculus-derivative.md",
        ),
    ];

    async fn representative_fixture() -> (
        tempfile::TempDir,
        LanceDbRag,
        LanceDbKnowledgeSource,
        KnowledgeAccessContext,
        RunContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        for (source, body) in REPRESENTATIVE_DOCUMENTS {
            rag.ingest_text("kb-a", source, body).await.unwrap();
        }
        let allowed = access("kb-a");
        let run = RunContext::new(
            RunRequest::from_text("Knowledge A6 acceptance").with_extension(allowed.clone()),
        );
        let source = LanceDbKnowledgeSource::new(rag.clone(), "kb-a");
        (temp, rag, source, allowed, run)
    }

    fn percentile_micros(samples: &[u128], percentile: f64) -> u128 {
        let mut ordered = samples.to_vec();
        ordered.sort_unstable();
        let index = ((ordered.len() - 1) as f64 * percentile).ceil() as usize;
        ordered[index]
    }

    #[tokio::test]
    async fn lance_source_passes_shared_contract() {
        let (_temp, source, allowed, run) = fixture().await;
        let page = source
            .search(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                SourceSearchRequest {
                    query: "refund".into(),
                    filters: vec![],
                    limit: 5,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let hit = page.hits.into_iter().next().unwrap();
        let selector = hit.suggested_selectors[0].clone();
        let case = SourceContractCase {
            allowed_access: allowed,
            denied_access: access("kb-b"),
            search: SourceSearchRequest {
                query: "refund".into(),
                filters: vec![],
                limit: 5,
                cursor: None,
            },
            expected_reference: hit.reference.clone(),
            selector,
            max_bytes: 4096,
            missing_reference: KnowledgeRef {
                source_id: COURSE_KNOWLEDGE_SOURCE_ID.into(),
                item_id: "missing".into(),
                revision: Some("sha256:missing".into()),
            },
            stale_reference: Some(KnowledgeRef {
                revision: Some("sha256:stale".into()),
                ..hit.reference
            }),
        };

        verify_source_contract(&source, &run, &case).await.unwrap();
    }

    #[tokio::test]
    async fn exact_read_rejects_a_cross_kb_item() {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        rag.ingest_text("kb-a", "doc-a::a.md", "alpha material")
            .await
            .unwrap();
        rag.ingest_text("kb-b", "doc-b::b.md", "beta material")
            .await
            .unwrap();
        let access_b = access("kb-b");
        let run_b = RunContext::new(RunRequest::from_text("beta"));
        let source_b = LanceDbKnowledgeSource::new(rag.clone(), "kb-b");
        let hit_b = source_b
            .search(
                KnowledgeRequestContext {
                    run: &run_b,
                    access: &access_b,
                },
                SourceSearchRequest {
                    query: "beta".into(),
                    filters: vec![],
                    limit: 1,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .hits
            .remove(0);

        let access_a = access("kb-a");
        let run_a = RunContext::new(RunRequest::from_text("alpha"));
        let source_a = LanceDbKnowledgeSource::new(rag, "kb-a");
        let error = source_a
            .read(
                KnowledgeRequestContext {
                    run: &run_a,
                    access: &access_a,
                },
                KnowledgeReadRequest {
                    reference: hit_b.reference,
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), KnowledgeErrorCode::NotFound);
    }

    #[tokio::test]
    async fn runtime_boundary_preserves_representative_management_search_quality() {
        let (_temp, rag, source, allowed, run) = representative_fixture().await;

        for (query, expected_source) in REPRESENTATIVE_QUERIES {
            let management_hits = rag.search_for_management("kb-a", query, 3).await.unwrap();
            let runtime_hits = source
                .search(
                    KnowledgeRequestContext {
                        run: &run,
                        access: &allowed,
                    },
                    SourceSearchRequest {
                        query: query.into(),
                        filters: vec![],
                        limit: 3,
                        cursor: None,
                    },
                    CancellationToken::new(),
                )
                .await
                .unwrap()
                .hits;

            assert_eq!(
                management_hits
                    .iter()
                    .map(|hit| hit.id.as_str())
                    .collect::<Vec<_>>(),
                runtime_hits
                    .iter()
                    .map(|hit| hit.reference.item_id.as_str())
                    .collect::<Vec<_>>(),
                "runtime boundary changed ranking for query `{query}`"
            );
            assert_eq!(
                management_hits.first().map(|hit| hit.raw_source.as_str()),
                Some(expected_source),
                "representative query `{query}` did not retrieve its expected document"
            );
        }
    }

    #[tokio::test]
    #[ignore = "acceptance benchmark; run in release mode with --ignored --nocapture"]
    async fn knowledge_a6_search_and_read_latency_baseline() {
        const WARMUPS: usize = 5;
        const ITERATIONS: usize = 100;

        let (_temp, rag, source, allowed, run) = representative_fixture().await;
        let query = REPRESENTATIVE_QUERIES[0].0;
        let context = KnowledgeRequestContext {
            run: &run,
            access: &allowed,
        };
        let search_request = SourceSearchRequest {
            query: query.into(),
            filters: vec![],
            limit: 3,
            cursor: None,
        };

        for _ in 0..WARMUPS {
            rag.search_for_management("kb-a", query, 3).await.unwrap();
            source
                .search(context, search_request.clone(), CancellationToken::new())
                .await
                .unwrap();
        }

        let mut management_search_micros = Vec::with_capacity(ITERATIONS);
        let mut runtime_search_micros = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let started = Instant::now();
            rag.search_for_management("kb-a", query, 3).await.unwrap();
            management_search_micros.push(started.elapsed().as_micros());

            let started = Instant::now();
            source
                .search(context, search_request.clone(), CancellationToken::new())
                .await
                .unwrap();
            runtime_search_micros.push(started.elapsed().as_micros());
        }

        let hit = source
            .search(context, search_request, CancellationToken::new())
            .await
            .unwrap()
            .hits
            .remove(0);
        let read_request = KnowledgeReadRequest {
            reference: hit.reference,
            selector: hit.suggested_selectors[0].clone(),
            max_bytes: 4096,
        };
        for _ in 0..WARMUPS {
            source
                .read(context, read_request.clone(), CancellationToken::new())
                .await
                .unwrap();
        }

        let mut runtime_read_micros = Vec::with_capacity(ITERATIONS);
        for _ in 0..ITERATIONS {
            let started = Instant::now();
            source
                .read(context, read_request.clone(), CancellationToken::new())
                .await
                .unwrap();
            runtime_read_micros.push(started.elapsed().as_micros());
        }

        println!(
            "{}",
            serde_json::json!({
                "fixture_documents": REPRESENTATIVE_DOCUMENTS.len(),
                "warmups": WARMUPS,
                "iterations": ITERATIONS,
                "unit": "microseconds",
                "management_search": {
                    "p50": percentile_micros(&management_search_micros, 0.50),
                    "p95": percentile_micros(&management_search_micros, 0.95),
                },
                "runtime_search": {
                    "p50": percentile_micros(&runtime_search_micros, 0.50),
                    "p95": percentile_micros(&runtime_search_micros, 0.95),
                },
                "runtime_read": {
                    "p50": percentile_micros(&runtime_read_micros, 0.50),
                    "p95": percentile_micros(&runtime_read_micros, 0.95),
                },
            })
        );
    }

    #[tokio::test]
    async fn search_is_bounded_and_read_honors_max_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let rag = LanceDbRag::new(temp.path(), hash_config());
        let body = "refund ".repeat(120);
        rag.ingest_text("kb-a", "doc-a::refunds.md", &body)
            .await
            .unwrap();
        let allowed = access("kb-a");
        let run = RunContext::new(RunRequest::from_text("refund"));
        let source = LanceDbKnowledgeSource::new(rag, "kb-a");
        let hit = source
            .search(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                SourceSearchRequest {
                    query: "refund".into(),
                    filters: vec![],
                    limit: 1,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .hits
            .remove(0);
        assert!(hit.snippet.len() <= MAX_SNIPPET_BYTES);
        assert!(hit.snippet.len() < body.trim().len());

        let content = source
            .read(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                KnowledgeReadRequest {
                    reference: hit.reference,
                    selector: hit.suggested_selectors[0].clone(),
                    max_bytes: 31,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert!(content.truncated);
        let serialized = serde_json::to_value(&content.blocks).unwrap();
        assert!(serialized.to_string().len() < body.len());
    }

    #[tokio::test]
    async fn unsupported_selector_and_backend_details_are_sanitized() {
        let (_temp, source, allowed, run) = fixture().await;
        let hit = source
            .search(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                SourceSearchRequest {
                    query: "refund".into(),
                    filters: vec![],
                    limit: 1,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .hits
            .remove(0);
        let error = source
            .read(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                KnowledgeReadRequest {
                    reference: hit.reference,
                    selector: ContentSelector::LineRange { start: 1, end: 2 },
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), KnowledgeErrorCode::UnsupportedCapability);
        assert_eq!(error.to_string(), "knowledge capability is unsupported");

        let failing = LanceDbKnowledgeSource::new(
            LanceDbRag::new(
                tempfile::tempdir().unwrap().path(),
                EmbeddingConfig {
                    provider: "provider-with-secret-name".into(),
                    ..hash_config()
                },
            ),
            "kb-a",
        );
        let error = failing
            .search(
                KnowledgeRequestContext {
                    run: &run,
                    access: &allowed,
                },
                SourceSearchRequest {
                    query: "refund".into(),
                    filters: vec![],
                    limit: 1,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "knowledge backend failed");
        assert!(!error.to_string().contains("secret"));
        assert!(error.diagnostic().unwrap().contains("secret"));
    }

    #[test]
    fn truncation_preserves_utf8_boundaries() {
        assert_eq!(truncate_utf8("你好世界", 7), ("你好".into(), true));
        assert_eq!(truncate_utf8("hello", 5), ("hello".into(), false));
    }
}
