use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::Utc;
use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    ContentSelector, FilterExpr, FilterFieldDescriptor, FilterOperator, FilterValueType,
    FreshnessClass, KnowledgeCapability, KnowledgeContent, KnowledgeError, KnowledgeHit,
    KnowledgeReadRequest, KnowledgeRef, KnowledgeRequestContext, KnowledgeSource,
    KnowledgeSourceDescriptor, SourceSearchPage, SourceSearchRequest,
};
use llm_harness_types::DataBlock;
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::tutor_memory_store::{
    TutorMemoryEntry, TutorMemoryKind, TutorMemoryStatus, TutorMemoryStore,
    tutor_memory_entry_revision,
};

pub const TUTOR_MEMORY_SOURCE_ID: &str = "llm-tutor.tutor-memory";
pub const TUTOR_MEMORY_TUTOR_ID_ATTRIBUTE: &str = "tutor_memory_tutor_id";
pub const TUTOR_MEMORY_MODE_ATTRIBUTE: &str = "tutor_memory_mode";

const ITEM_PREFIX: &str = "entry/";
const SEARCH_SNIPPET: &str =
    "Tutor Memory match. Content is available only through an exact knowledge_read.";
const MAX_SEARCH_LIMIT: usize = 50;

pub struct TutorMemoryKnowledgeSource {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
    descriptor: KnowledgeSourceDescriptor,
}

impl TutorMemoryKnowledgeSource {
    pub fn new(store: Arc<TutorMemoryStore>, tutor_id: impl Into<String>) -> Self {
        let tutor_id = tutor_id.into();
        Self {
            store,
            tutor_id,
            descriptor: KnowledgeSourceDescriptor {
                id: TUTOR_MEMORY_SOURCE_ID.into(),
                name: "Tutor Memory".into(),
                description:
                    "Private continuity memory for the tutor bound to the current Agent run.".into(),
                domains: vec![
                    "tutor-memory.commitment".into(),
                    "tutor-memory.open-loop".into(),
                    "tutor-memory.lesson-plan".into(),
                    "tutor-memory.reflection".into(),
                    "tutor-memory.strategy".into(),
                ],
                capabilities: BTreeSet::from([
                    KnowledgeCapability::Search,
                    KnowledgeCapability::Read,
                    KnowledgeCapability::Revisioned,
                ]),
                freshness: FreshnessClass::Live,
                filter_fields: vec![
                    FilterFieldDescriptor {
                        name: "kind".into(),
                        value_type: FilterValueType::String,
                        operators: BTreeSet::from([FilterOperator::Eq, FilterOperator::In]),
                    },
                    FilterFieldDescriptor {
                        name: "status".into(),
                        value_type: FilterValueType::String,
                        operators: BTreeSet::from([FilterOperator::Eq, FilterOperator::In]),
                    },
                ],
            },
        }
    }

    fn authorize(
        &self,
        ctx: KnowledgeRequestContext<'_>,
        abort: &CancellationToken,
    ) -> Result<(), KnowledgeError> {
        if abort.is_cancelled() {
            return Err(KnowledgeError::Aborted);
        }
        let attributes = &ctx.access.scope.attributes;
        let allowed = ctx.access.scope.namespace == tutor_rag::AGENT_KNOWLEDGE_NAMESPACE
            && !ctx.access.principal.subject.trim().is_empty()
            && attributes
                .get(TUTOR_MEMORY_TUTOR_ID_ATTRIBUTE)
                .is_some_and(|tutor_id| tutor_id == &self.tutor_id)
            && matches!(
                attributes
                    .get(TUTOR_MEMORY_MODE_ATTRIBUTE)
                    .map(String::as_str),
                Some("read_only" | "autonomous")
            );
        if allowed {
            Ok(())
        } else {
            Err(KnowledgeError::Unauthorized)
        }
    }

    fn entries(&self) -> Result<Vec<TutorMemoryEntry>, KnowledgeError> {
        self.store
            .list(&self.tutor_id, true)
            .map_err(|error| KnowledgeError::Backend(error.to_string()))
    }
}

impl KnowledgeSource for TutorMemoryKnowledgeSource {
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
            validate_filters(&request.filters)?;
            if request.limit == 0 || request.query.trim().is_empty() {
                return Ok(SourceSearchPage {
                    hits: Vec::new(),
                    next_cursor: None,
                });
            }
            let offset = decode_cursor(request.cursor.as_deref())?;
            let query = request.query.trim().to_lowercase();
            let terms = query.split_whitespace().collect::<Vec<_>>();
            let mut entries = self
                .entries()?
                .into_iter()
                .filter(|entry| filters_match(entry, &request.filters))
                .filter_map(|entry| {
                    let haystack = format!(
                        "{} {} {}",
                        kind_name(entry.kind),
                        entry.text,
                        entry.next_action.as_deref().unwrap_or_default()
                    )
                    .to_lowercase();
                    let score = terms
                        .iter()
                        .filter(|term| haystack.contains(**term))
                        .count();
                    (query == "*" || score > 0).then_some((entry, score))
                })
                .collect::<Vec<_>>();
            entries.sort_by(|(left, left_score), (right, right_score)| {
                right_score
                    .cmp(left_score)
                    .then_with(|| right.updated_at.cmp(&left.updated_at))
                    .then_with(|| left.id.cmp(&right.id))
            });
            let limit = request.limit.min(MAX_SEARCH_LIMIT);
            let total = entries.len();
            let hits = entries
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|(entry, score)| {
                    let reference = entry_reference(&entry);
                    let mut metadata = BTreeMap::new();
                    metadata.insert("kind".into(), json!(kind_name(entry.kind)));
                    metadata.insert("status".into(), json!(status_name(entry.status)));
                    metadata.insert("tutor_id".into(), json!(self.tutor_id));
                    KnowledgeHit {
                        reference,
                        title: Some(format!("Tutor {}", kind_name(entry.kind))),
                        snippet: SEARCH_SNIPPET.into(),
                        suggested_selectors: vec![ContentSelector::Document],
                        uri: Some(format!("tutor-memory:{}", entry.id)),
                        score: (query != "*").then_some(score as f32),
                        updated_at: Some(entry.updated_at),
                        metadata,
                    }
                })
                .collect::<Vec<_>>();
            let next_offset = offset.saturating_add(hits.len());
            Ok(SourceSearchPage {
                hits,
                next_cursor: (next_offset < total).then(|| next_offset.to_string()),
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
            if request.reference.source_id != TUTOR_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            if request.selector != ContentSelector::Document {
                return Err(KnowledgeError::UnsupportedCapability(
                    "Tutor Memory supports exact document reads only".into(),
                ));
            }
            let entry_id = request
                .reference
                .item_id
                .strip_prefix(ITEM_PREFIX)
                .filter(|entry_id| !entry_id.is_empty() && !entry_id.contains('/'))
                .ok_or(KnowledgeError::NotFound)?;
            let entry = self
                .store
                .get(&self.tutor_id, entry_id)
                .map_err(|error| match error {
                    crate::tutor_memory_store::TutorMemoryError::NotFound => {
                        KnowledgeError::NotFound
                    }
                    other => KnowledgeError::Backend(other.to_string()),
                })?;
            let latest = entry_reference(&entry);
            if request.reference.revision != latest.revision {
                return Err(KnowledgeError::StaleReference {
                    latest: Some(latest),
                });
            }
            if abort.is_cancelled() {
                return Err(KnowledgeError::Aborted);
            }
            let body = serde_json::to_string_pretty(&json!({
                "kind": kind_name(entry.kind),
                "text": entry.text,
                "status": status_name(entry.status),
                "nextAction": entry.next_action,
                "dueAt": entry.due_at,
                "resolutionNote": entry.resolution_note,
                "createdAt": entry.created_at,
                "updatedAt": entry.updated_at,
                "resolvedAt": entry.resolved_at,
            }))
            .map_err(|error| KnowledgeError::Backend(error.to_string()))?;
            let (body, truncated) = truncate_utf8(&body, request.max_bytes);
            let mut metadata = BTreeMap::new();
            metadata.insert("kind".into(), json!(kind_name(entry.kind)));
            metadata.insert("status".into(), json!(status_name(entry.status)));
            metadata.insert("tutor_id".into(), json!(self.tutor_id));
            Ok(KnowledgeContent {
                reference: request.reference,
                selector: request.selector,
                title: Some(format!("Tutor {}", kind_name(entry.kind))),
                blocks: vec![DataBlock::text(body)],
                uri: Some(format!("tutor-memory:{}", entry.id)),
                updated_at: Some(entry.updated_at),
                obtained_at: Utc::now(),
                truncated,
                metadata,
            })
        })
    }
}

fn entry_reference(entry: &TutorMemoryEntry) -> KnowledgeRef {
    KnowledgeRef {
        source_id: TUTOR_MEMORY_SOURCE_ID.into(),
        item_id: format!("{ITEM_PREFIX}{}", entry.id),
        revision: Some(tutor_memory_entry_revision(entry)),
    }
}

fn kind_name(kind: TutorMemoryKind) -> &'static str {
    match kind {
        TutorMemoryKind::Commitment => "commitment",
        TutorMemoryKind::OpenLoop => "open_loop",
        TutorMemoryKind::LessonPlan => "lesson_plan",
        TutorMemoryKind::Reflection => "reflection",
        TutorMemoryKind::Strategy => "strategy",
    }
}

fn status_name(status: TutorMemoryStatus) -> &'static str {
    match status {
        TutorMemoryStatus::Active => "active",
        TutorMemoryStatus::Resolved => "resolved",
    }
}

fn validate_filters(filters: &[FilterExpr]) -> Result<(), KnowledgeError> {
    for filter in filters {
        match filter {
            FilterExpr::Eq { field, value } => validate_filter_value(field, value)?,
            FilterExpr::In { field, values } if !values.is_empty() => {
                for value in values {
                    validate_filter_value(field, value)?;
                }
            }
            FilterExpr::In { .. } | FilterExpr::Range { .. } => {
                return Err(KnowledgeError::InvalidFilter(
                    "Tutor Memory filters support non-empty eq/in string values".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_filter_value(field: &str, value: &serde_json::Value) -> Result<(), KnowledgeError> {
    let value = value.as_str().ok_or_else(|| {
        KnowledgeError::InvalidFilter("Tutor Memory filter values must be strings".into())
    })?;
    let valid = match field {
        "kind" => matches!(
            value,
            "commitment" | "open_loop" | "lesson_plan" | "reflection" | "strategy"
        ),
        "status" => matches!(value, "active" | "resolved"),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(KnowledgeError::InvalidFilter(
            "Tutor Memory filter field or value is not allowlisted".into(),
        ))
    }
}

fn filters_match(entry: &TutorMemoryEntry, filters: &[FilterExpr]) -> bool {
    filters.iter().all(|filter| match filter {
        FilterExpr::Eq { field, value } => entry_field(entry, field) == value.as_str(),
        FilterExpr::In { field, values } => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|value| entry_field(entry, field) == Some(value)),
        FilterExpr::Range { .. } => false,
    })
}

fn entry_field<'a>(entry: &'a TutorMemoryEntry, field: &str) -> Option<&'a str> {
    match field {
        "kind" => Some(kind_name(entry.kind)),
        "status" => Some(status_name(entry.status)),
        _ => None,
    }
}

fn decode_cursor(cursor: Option<&str>) -> Result<usize, KnowledgeError> {
    cursor
        .map(str::parse::<usize>)
        .transpose()
        .map_err(|_| KnowledgeError::InvalidCursor)
        .map(Option::unwrap_or_default)
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
    use llm_harness_runtime_knowledge::{KnowledgeAccessContext, KnowledgeScope, PrincipalRef};
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;
    use crate::tutor_memory_store::CreateTutorMemoryEntry;

    fn access(tutor_id: &str) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(tutor_rag::AGENT_KNOWLEDGE_NAMESPACE);
        scope
            .attributes
            .insert(TUTOR_MEMORY_TUTOR_ID_ATTRIBUTE.into(), tutor_id.into());
        scope
            .attributes
            .insert(TUTOR_MEMORY_MODE_ATTRIBUTE.into(), "autonomous".into());
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "user"))
    }

    fn request_context<'a>(
        run: &'a RunContext,
        access: &'a KnowledgeAccessContext,
    ) -> KnowledgeRequestContext<'a> {
        KnowledgeRequestContext { run, access }
    }

    #[tokio::test]
    async fn search_and_exact_read_are_tutor_scoped_and_revisioned() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(TutorMemoryStore::new_with_root(dir.path()));
        let entry = store
            .create(
                "tutor-a",
                CreateTutorMemoryEntry {
                    kind: TutorMemoryKind::OpenLoop,
                    text: "Continue the attention exercise".into(),
                    next_action: Some("Review question 3".into()),
                    due_at: None,
                    source_session_id: Some("session-a".into()),
                    source_message_id: None,
                },
            )
            .unwrap();
        let source = TutorMemoryKnowledgeSource::new(store.clone(), "tutor-a");
        let run = RunContext::new(RunRequest::from_text("attention"));
        let allowed = access("tutor-a");
        let page = source
            .search(
                request_context(&run, &allowed),
                SourceSearchRequest {
                    query: "attention".into(),
                    filters: vec![],
                    limit: 10,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].snippet, SEARCH_SNIPPET);
        assert!(!page.hits[0].snippet.contains("attention exercise"));

        let reference = page.hits[0].reference.clone();
        let content = source
            .read(
                request_context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: reference.clone(),
                    selector: ContentSelector::Document,
                    max_bytes: 4_096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(content.reference, reference);
        match &content.blocks[0] {
            DataBlock::Text { text, .. } => assert!(text.contains(&entry.text)),
            other => panic!("expected text content, got {other:?}"),
        }

        store
            .resolve("tutor-a", &entry.id, Some("Completed".into()))
            .unwrap();
        let stale = source
            .read(
                request_context(&run, &allowed),
                KnowledgeReadRequest {
                    reference,
                    selector: ContentSelector::Document,
                    max_bytes: 4_096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(stale, KnowledgeError::StaleReference { .. }));

        let denied = access("tutor-b");
        assert!(
            source
                .search(
                    request_context(&run, &denied),
                    SourceSearchRequest {
                        query: "*".into(),
                        filters: vec![],
                        limit: 10,
                        cursor: None,
                    },
                    CancellationToken::new(),
                )
                .await
                .is_err()
        );
    }
}
