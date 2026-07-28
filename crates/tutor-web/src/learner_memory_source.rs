use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    ContentSelector, FilterExpr, FilterFieldDescriptor, FilterOperator, FilterValueType,
    FreshnessClass, KnowledgeCapability, KnowledgeContent, KnowledgeError, KnowledgeHit,
    KnowledgeReadRequest, KnowledgeRef, KnowledgeRequestContext, KnowledgeSource,
    KnowledgeSourceDescriptor, SourceSearchPage, SourceSearchRequest,
};
use llm_harness_types::DataBlock;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::memory_store::{
    FileMemoryBackend, MemoryEntry, MemoryEvent, MemoryEventCategory, memory_entry_revision,
    memory_revision, try_parse_memory_entries,
};

pub const LEARNER_MEMORY_SOURCE_ID: &str = "llm-tutor.learner-memory";
pub const LEARNER_MEMORY_NAMESPACE: &str = tutor_rag::AGENT_KNOWLEDGE_NAMESPACE;
pub const LEARNER_MEMORY_PROFILE_ATTRIBUTE: &str = "learner_memory_profile";
pub const LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE: &str = "learner_memory_principal";
pub const LEARNER_MEMORY_LAYERS_ATTRIBUTE: &str = "learner_memory_layers";
pub const LEARNER_MEMORY_SURFACES_ATTRIBUTE: &str = "learner_memory_surfaces";
pub const LEARNER_MEMORY_KINDS_ATTRIBUTE: &str = "learner_memory_kinds";
pub const LEARNER_MEMORY_L1_SESSION_ATTRIBUTE: &str = "learner_memory_l1_session_id";

const MAX_SEARCH_LIMIT: usize = 50;
const MAX_SNIPPET_BYTES: usize = 500;

#[derive(Clone)]
pub struct LearnerMemoryKnowledgeSource {
    backend: Arc<FileMemoryBackend>,
    descriptor: KnowledgeSourceDescriptor,
}

#[derive(Clone)]
struct ReadScope {
    layers: BTreeSet<String>,
    surfaces: BTreeSet<String>,
    kinds: BTreeSet<String>,
    l1_session_id: Option<String>,
}

#[derive(Clone)]
struct MemoryItem {
    layer: &'static str,
    surface: Option<String>,
    kind: Option<String>,
    reference: KnowledgeRef,
    title: String,
    body: String,
    updated_at: Option<DateTime<Utc>>,
    metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SearchCursor {
    version: u32,
    offset: usize,
    fingerprint: String,
}

impl LearnerMemoryKnowledgeSource {
    pub fn new(backend: Arc<FileMemoryBackend>) -> Self {
        let filter_fields = ["layer", "surface", "kind"]
            .into_iter()
            .map(|name| FilterFieldDescriptor {
                name: name.into(),
                value_type: FilterValueType::String,
                operators: BTreeSet::from([FilterOperator::Eq, FilterOperator::In]),
            })
            .collect();
        Self {
            backend,
            descriptor: KnowledgeSourceDescriptor {
                id: LEARNER_MEMORY_SOURCE_ID.into(),
                name: "Learner memory".into(),
                description: "Durable learner events, summaries, and profile entries.".into(),
                domains: [
                    "learner-memory.event.chat",
                    "learner-memory.event.quiz",
                    "learner-memory.event.notebook",
                    "learner-memory.event.knowledge",
                    "learner-memory.summary.chat",
                    "learner-memory.summary.quiz",
                    "learner-memory.summary.notebook",
                    "learner-memory.summary.knowledge",
                    "learner-memory.profile.recent",
                    "learner-memory.profile.profile",
                    "learner-memory.profile.scope",
                    "learner-memory.profile.preferences",
                    "learner-memory.profile.teaching_strategy",
                ]
                .into_iter()
                .map(str::to_string)
                .collect(),
                capabilities: BTreeSet::from([
                    KnowledgeCapability::Search,
                    KnowledgeCapability::Read,
                    KnowledgeCapability::Revisioned,
                ]),
                freshness: FreshnessClass::Live,
                filter_fields,
            },
        }
    }

    fn scope(
        &self,
        ctx: KnowledgeRequestContext<'_>,
        abort: &CancellationToken,
    ) -> Result<ReadScope, KnowledgeError> {
        if abort.is_cancelled() {
            return Err(KnowledgeError::Aborted);
        }
        let access = ctx.access;
        let profile = access
            .scope
            .attributes
            .get(LEARNER_MEMORY_PROFILE_ATTRIBUTE)
            .map(String::as_str);
        let bound_principal = access
            .scope
            .attributes
            .get(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE)
            .map(String::as_str);
        if access.scope.namespace != LEARNER_MEMORY_NAMESPACE
            || access.principal.subject.trim().is_empty()
            || bound_principal != Some(access.principal.subject.as_str())
            || !matches!(profile, Some("read_only" | "interactive_mutation"))
        {
            return Err(KnowledgeError::Unauthorized);
        }
        let layers = parse_access_set(access, LEARNER_MEMORY_LAYERS_ATTRIBUTE)?;
        let surfaces = parse_access_set(access, LEARNER_MEMORY_SURFACES_ATTRIBUTE)?;
        let kinds = parse_access_set(access, LEARNER_MEMORY_KINDS_ATTRIBUTE)?;
        if !layers
            .iter()
            .all(|value| matches!(value.as_str(), "l1" | "l2" | "l3"))
            || !surfaces
                .iter()
                .all(|value| matches!(value.as_str(), "chat" | "quiz" | "notebook" | "knowledge"))
            || !kinds.iter().all(|value| {
                matches!(
                    value.as_str(),
                    "recent" | "profile" | "scope" | "preferences" | "teaching_strategy"
                )
            })
        {
            return Err(KnowledgeError::Unauthorized);
        }
        Ok(ReadScope {
            layers,
            surfaces,
            kinds,
            l1_session_id: access
                .scope
                .attributes
                .get(LEARNER_MEMORY_L1_SESSION_ATTRIBUTE)
                .map(String::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
        })
    }

    fn items(&self, scope: &ReadScope) -> Result<Vec<MemoryItem>, KnowledgeError> {
        let mut items = Vec::new();
        if scope.layers.contains("l1") {
            for event in self.backend.all_events().map_err(backend_error)? {
                if let Some(item) = event_item(event, scope) {
                    items.push(item);
                }
            }
        }
        for file in self.backend.list().map_err(backend_error)? {
            let Some((layer, category)) = file_category(&file.path) else {
                continue;
            };
            if !scope.layers.contains(layer)
                || (layer == "l2" && !scope.surfaces.contains(category))
                || (layer == "l3" && !scope.kinds.contains(category))
            {
                continue;
            }
            let entries = try_parse_memory_entries(&file.markdown).map_err(backend_error)?;
            for entry in entries {
                if entry
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.target != file.path)
                {
                    return Err(KnowledgeError::Backend(
                        "memory metadata target does not match its canonical file".into(),
                    ));
                }
                if entry
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.expires_at)
                    .is_some_and(|expires_at| expires_at <= Utc::now())
                {
                    continue;
                }
                items.push(entry_item(layer, category, entry).map_err(backend_error)?);
            }
        }
        items.sort_by(|left, right| left.reference.item_id.cmp(&right.reference.item_id));
        Ok(items)
    }
}

impl KnowledgeSource for LearnerMemoryKnowledgeSource {
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
            let scope = self.scope(ctx, &abort)?;
            validate_filters(&request.filters)?;
            if request.limit == 0 || request.query.trim().is_empty() {
                return Ok(SourceSearchPage {
                    hits: Vec::new(),
                    next_cursor: None,
                });
            }
            let fingerprint = search_fingerprint(&scope, &request)?;
            let offset = decode_cursor(request.cursor.as_deref(), &fingerprint)?;
            let query = request.query.trim().to_lowercase();
            let terms = query.split_whitespace().collect::<Vec<_>>();
            let mut matches = self
                .items(&scope)?
                .into_iter()
                .filter(|item| filters_match(item, &request.filters))
                .filter_map(|item| {
                    let haystack = format!("{} {}", item.title, item.body).to_lowercase();
                    let matched = terms
                        .iter()
                        .filter(|term| haystack.contains(**term))
                        .count();
                    (query == "*" || matched > 0).then_some((item, matched))
                })
                .collect::<Vec<_>>();
            if abort.is_cancelled() {
                return Err(KnowledgeError::Aborted);
            }
            matches.sort_by(|(left, left_score), (right, right_score)| {
                right_score
                    .cmp(left_score)
                    .then_with(|| left.reference.item_id.cmp(&right.reference.item_id))
            });
            let limit = request.limit.min(MAX_SEARCH_LIMIT);
            let total = matches.len();
            let hits = matches
                .into_iter()
                .skip(offset)
                .take(limit)
                .map(|(item, matched)| {
                    let (snippet, _) = truncate_utf8(&item.body, MAX_SNIPPET_BYTES);
                    let uri = item_uri(&item.reference.item_id);
                    KnowledgeHit {
                        reference: item.reference,
                        title: Some(item.title),
                        snippet,
                        suggested_selectors: vec![ContentSelector::Document],
                        uri: Some(uri),
                        score: (query != "*").then_some(matched as f32 / terms.len() as f32),
                        updated_at: item.updated_at,
                        metadata: item.metadata,
                    }
                })
                .collect::<Vec<_>>();
            let next_offset = offset.saturating_add(hits.len());
            Ok(SourceSearchPage {
                hits,
                next_cursor: (next_offset < total)
                    .then(|| encode_cursor(next_offset, fingerprint.clone()))
                    .transpose()?,
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
            let scope = self.scope(ctx, &abort)?;
            if request.reference.source_id != LEARNER_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            if request.selector != ContentSelector::Document {
                return Err(KnowledgeError::UnsupportedCapability(
                    "learner memory supports exact document reads only".into(),
                ));
            }
            let item = self
                .items(&scope)?
                .into_iter()
                .find(|item| item.reference.item_id == request.reference.item_id)
                .ok_or(KnowledgeError::NotFound)?;
            if request.reference.revision != item.reference.revision {
                return Err(KnowledgeError::StaleReference {
                    latest: Some(item.reference),
                });
            }
            if abort.is_cancelled() {
                return Err(KnowledgeError::Aborted);
            }
            let (body, truncated) = truncate_utf8(&item.body, request.max_bytes);
            let uri = item_uri(&item.reference.item_id);
            Ok(KnowledgeContent {
                reference: request.reference,
                selector: request.selector,
                title: Some(item.title),
                blocks: vec![DataBlock::text(body)],
                uri: Some(uri),
                updated_at: item.updated_at,
                obtained_at: Utc::now(),
                truncated,
                metadata: item.metadata,
            })
        })
    }
}

fn parse_access_set(
    access: &llm_harness_runtime_knowledge::KnowledgeAccessContext,
    attribute: &str,
) -> Result<BTreeSet<String>, KnowledgeError> {
    access
        .scope
        .attributes
        .get(attribute)
        .ok_or(KnowledgeError::Unauthorized)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
}

fn event_item(event: MemoryEvent, scope: &ReadScope) -> Option<MemoryItem> {
    let surface = event_surface(event.category);
    if !scope.surfaces.contains(surface)
        || scope
            .l1_session_id
            .as_deref()
            .is_some_and(|session| event.source_id.as_deref() != Some(session))
    {
        return None;
    }
    let revision = memory_revision(&serde_json::to_string(&event).ok()?);
    let body = serde_json::to_string_pretty(&json!({
        "action": event.action,
        "summary": event.summary,
        "payload": event.payload,
        "createdAt": event.created_at,
    }))
    .ok()?;
    let mut metadata = BTreeMap::new();
    metadata.insert("layer".into(), json!("l1"));
    metadata.insert("surface".into(), json!(surface));
    Some(MemoryItem {
        layer: "l1",
        surface: Some(surface.into()),
        kind: None,
        reference: KnowledgeRef {
            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
            item_id: format!("l1/{surface}/{}", event.id),
            revision: Some(revision),
        },
        title: event.summary,
        body,
        updated_at: Some(event.created_at),
        metadata,
    })
}

fn entry_item(
    layer: &'static str,
    category: &str,
    entry: MemoryEntry,
) -> anyhow::Result<MemoryItem> {
    let revision = memory_entry_revision(&entry)?;
    let item_id = format!("{layer}/{category}/{}", entry.marker);
    let mut metadata = BTreeMap::new();
    metadata.insert("layer".into(), json!(layer));
    let (surface, kind) = if layer == "l2" {
        metadata.insert("surface".into(), json!(category));
        (Some(category.into()), None)
    } else {
        metadata.insert("kind".into(), json!(category));
        (None, Some(category.into()))
    };
    let title = entry
        .section
        .as_deref()
        .map(|section| format!("{category}: {section}"))
        .unwrap_or_else(|| category.to_string());
    let body = if entry.source_refs.is_empty() {
        entry.text
    } else {
        format!(
            "{}\n\nSources: {}",
            entry.text,
            entry.source_refs.join(", ")
        )
    };
    Ok(MemoryItem {
        layer,
        surface,
        kind,
        reference: KnowledgeRef {
            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
            item_id,
            revision: Some(revision),
        },
        title,
        body,
        updated_at: None,
        metadata,
    })
}

fn file_category(path: &str) -> Option<(&'static str, &str)> {
    let (directory, filename) = path.split_once('/')?;
    let category = filename.strip_suffix(".md")?;
    match directory {
        "L2" => Some(("l2", category)),
        "L3" => Some(("l3", category)),
        _ => None,
    }
}

fn event_surface(category: MemoryEventCategory) -> &'static str {
    match category {
        MemoryEventCategory::Chat => "chat",
        MemoryEventCategory::Quiz => "quiz",
        MemoryEventCategory::Notebook => "notebook",
        MemoryEventCategory::Knowledge => "knowledge",
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
                    "learner memory filters support non-empty eq/in string values".into(),
                ));
            }
        }
    }
    Ok(())
}

fn validate_filter_value(field: &str, value: &serde_json::Value) -> Result<(), KnowledgeError> {
    let Some(value) = value.as_str() else {
        return Err(KnowledgeError::InvalidFilter(
            "learner memory filter values must be strings".into(),
        ));
    };
    let valid = match field {
        "layer" => matches!(value, "l1" | "l2" | "l3"),
        "surface" => matches!(value, "chat" | "quiz" | "notebook" | "knowledge"),
        "kind" => matches!(
            value,
            "recent" | "profile" | "scope" | "preferences" | "teaching_strategy"
        ),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(KnowledgeError::InvalidFilter(
            "learner memory filter field or value is not allowlisted".into(),
        ))
    }
}

fn filters_match(item: &MemoryItem, filters: &[FilterExpr]) -> bool {
    filters.iter().all(|filter| match filter {
        FilterExpr::Eq { field, value } => item_field(item, field) == value.as_str(),
        FilterExpr::In { field, values } => values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .any(|value| item_field(item, field) == Some(value)),
        FilterExpr::Range { .. } => false,
    })
}

fn item_field<'a>(item: &'a MemoryItem, field: &str) -> Option<&'a str> {
    match field {
        "layer" => Some(item.layer),
        "surface" => item.surface.as_deref(),
        "kind" => item.kind.as_deref(),
        _ => None,
    }
}

fn search_fingerprint(
    scope: &ReadScope,
    request: &SourceSearchRequest,
) -> Result<String, KnowledgeError> {
    serde_json::to_string(&json!({
        "query": request.query.trim(),
        "filters": request.filters,
        "layers": scope.layers,
        "surfaces": scope.surfaces,
        "kinds": scope.kinds,
        "l1SessionId": scope.l1_session_id,
    }))
    .map(|value| memory_revision(&value))
    .map_err(|error| KnowledgeError::Backend(error.to_string()))
}

fn encode_cursor(offset: usize, fingerprint: String) -> Result<String, KnowledgeError> {
    serde_json::to_vec(&SearchCursor {
        version: 1,
        offset,
        fingerprint,
    })
    .map(|value| URL_SAFE_NO_PAD.encode(value))
    .map_err(|error| KnowledgeError::Backend(error.to_string()))
}

fn decode_cursor(cursor: Option<&str>, fingerprint: &str) -> Result<usize, KnowledgeError> {
    let Some(cursor) = cursor else {
        return Ok(0);
    };
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| KnowledgeError::InvalidCursor)?;
    let cursor = serde_json::from_slice::<SearchCursor>(&decoded)
        .map_err(|_| KnowledgeError::InvalidCursor)?;
    if cursor.version != 1 || cursor.fingerprint != fingerprint {
        return Err(KnowledgeError::InvalidCursor);
    }
    Ok(cursor.offset)
}

fn item_uri(item_id: &str) -> String {
    format!("llm-tutor://learner-memory/{item_id}")
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

fn backend_error(error: anyhow::Error) -> KnowledgeError {
    KnowledgeError::Backend(format!("{error:#}"))
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use llm_harness_runtime_knowledge::contract::{SourceContractCase, verify_source_contract};
    use llm_harness_runtime_knowledge::{
        KnowledgeAccessContext, KnowledgeErrorCode, KnowledgeScope, PrincipalRef,
    };
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;
    use crate::memory_store::{MemoryEntryMetadata, MemoryEventCategory, serialize_memory_entries};

    fn access(
        namespace: &str,
        layers: &str,
        surfaces: &str,
        kinds: &str,
    ) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(namespace);
        scope
            .attributes
            .insert(LEARNER_MEMORY_PROFILE_ATTRIBUTE.into(), "read_only".into());
        scope.attributes.insert(
            LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE.into(),
            "local-user".into(),
        );
        scope
            .attributes
            .insert(LEARNER_MEMORY_LAYERS_ATTRIBUTE.into(), layers.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_SURFACES_ATTRIBUTE.into(), surfaces.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_KINDS_ATTRIBUTE.into(), kinds.into());
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "user"))
    }

    fn context<'a>(
        run: &'a RunContext,
        access: &'a KnowledgeAccessContext,
    ) -> KnowledgeRequestContext<'a> {
        KnowledgeRequestContext { run, access }
    }

    fn preference_entry(marker: &str, text: &str) -> MemoryEntry {
        MemoryEntry {
            line_number: 3,
            section: Some("Preferences".into()),
            text: text.into(),
            marker: marker.into(),
            source_refs: Vec::new(),
            metadata: None,
        }
    }

    fn fixture() -> (
        tempfile::TempDir,
        LearnerMemoryKnowledgeSource,
        KnowledgeAccessContext,
        RunContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let markdown = serialize_memory_entries(
            "Learning preferences",
            &[
                preference_entry("m_visual", "Prefers visual diagrams."),
                preference_entry("m_steps", "Prefers step-by-step explanations."),
            ],
        )
        .unwrap();
        backend.write("L3/preferences.md", markdown).unwrap();
        let source = LearnerMemoryKnowledgeSource::new(backend);
        let allowed = access(LEARNER_MEMORY_NAMESPACE, "l3", "", "preferences");
        let run = RunContext::new(RunRequest::from_text("learner memory"));
        (temp, source, allowed, run)
    }

    #[tokio::test]
    async fn learner_memory_source_passes_shared_contract() {
        let (_temp, source, allowed, run) = fixture();
        let search = SourceSearchRequest {
            query: "visual".into(),
            filters: vec![FilterExpr::Eq {
                field: "kind".into(),
                value: json!("preferences"),
            }],
            limit: 5,
            cursor: None,
        };
        let hit = source
            .search(
                context(&run, &allowed),
                search.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .hits
            .remove(0);
        let case = SourceContractCase {
            allowed_access: allowed,
            denied_access: access("wrong.namespace", "l3", "", "preferences"),
            search,
            expected_reference: hit.reference.clone(),
            selector: ContentSelector::Document,
            max_bytes: 4096,
            missing_reference: KnowledgeRef {
                source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                item_id: "l3/preferences/m_missing".into(),
                revision: Some("missing".into()),
            },
            stale_reference: Some(KnowledgeRef {
                revision: Some("stale".into()),
                ..hit.reference
            }),
        };

        verify_source_contract(&source, &run, &case).await.unwrap();
    }

    #[tokio::test]
    async fn filters_and_cursors_are_allowlisted_and_request_bound() {
        let (_temp, source, allowed, run) = fixture();
        let first = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![FilterExpr::Eq {
                        field: "layer".into(),
                        value: json!("l3"),
                    }],
                    limit: 1,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(first.hits.len(), 1);
        let cursor = first.next_cursor.unwrap();
        let second = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![FilterExpr::Eq {
                        field: "layer".into(),
                        value: json!("l3"),
                    }],
                    limit: 1,
                    cursor: Some(cursor.clone()),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(second.hits.len(), 1);
        assert_ne!(first.hits[0].reference, second.hits[0].reference);

        let error = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "different".into(),
                    filters: vec![],
                    limit: 1,
                    cursor: Some(cursor),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), KnowledgeErrorCode::InvalidCursor);

        let error = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![FilterExpr::Eq {
                        field: "path".into(),
                        value: json!("../../secret"),
                    }],
                    limit: 5,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code(), KnowledgeErrorCode::InvalidFilter);
    }

    #[tokio::test]
    async fn expired_entries_are_invisible_to_search_and_read() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let expired = MemoryEntry {
            metadata: Some(MemoryEntryMetadata {
                schema_version: 1,
                item_id: "m_expired".into(),
                kind: "preference".into(),
                target: "L3/preferences.md".into(),
                provenance: json!({ "principalId": "local-user" }),
                idempotency_key: "expired-key".into(),
                expires_at: Some(Utc::now() - Duration::minutes(1)),
            }),
            ..preference_entry("m_expired", "Expired visual preference.")
        };
        let active = preference_entry("m_active", "Active visual preference.");
        let markdown =
            serialize_memory_entries("Learning preferences", &[expired, active]).unwrap();
        backend.write("L3/preferences.md", markdown).unwrap();
        let source = LearnerMemoryKnowledgeSource::new(backend);
        let allowed = access(LEARNER_MEMORY_NAMESPACE, "l3", "", "preferences");
        let run = RunContext::new(RunRequest::from_text("visual"));
        let page = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "visual".into(),
                    filters: vec![],
                    limit: 10,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].reference.item_id, "l3/preferences/m_active");
        let error = source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: KnowledgeRef {
                        source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                        item_id: "l3/preferences/m_expired".into(),
                        revision: Some("forged".into()),
                    },
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
    async fn l1_session_scope_and_layer_scope_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        backend
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "Session one vectors",
                Some("session-1".into()),
                json!({}),
            )
            .unwrap();
        backend
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "Session two vectors",
                Some("session-2".into()),
                json!({}),
            )
            .unwrap();
        let source = LearnerMemoryKnowledgeSource::new(backend);
        let mut allowed = access(LEARNER_MEMORY_NAMESPACE, "l1", "chat", "");
        allowed.scope.attributes.insert(
            LEARNER_MEMORY_L1_SESSION_ATTRIBUTE.into(),
            "session-1".into(),
        );
        let run = RunContext::new(RunRequest::from_text("vectors"));
        let page = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "vectors".into(),
                    filters: vec![],
                    limit: 10,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(page.hits.len(), 1);
        assert!(page.hits[0].snippet.contains("Session one"));
        assert!(
            page.hits
                .iter()
                .all(|hit| hit.reference.item_id.starts_with("l1/chat/"))
        );
    }

    #[tokio::test]
    async fn access_scope_cannot_be_replayed_for_another_principal() {
        let (_temp, source, mut allowed, run) = fixture();
        allowed.principal = PrincipalRef::new("attacker", "user");

        let error = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![],
                    limit: 5,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), KnowledgeErrorCode::Unauthorized);
    }

    #[tokio::test]
    async fn fixed_item_mapping_and_scope_hide_ungranted_layers() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let event = backend
            .record_event(
                MemoryEventCategory::Chat,
                "asked",
                "Vector question",
                Some("session-1".into()),
                json!({}),
            )
            .unwrap();
        backend
            .write(
                "L2/chat.md",
                "# Chat memory\n\n- Vector summary. <!--m_summary-->".into(),
            )
            .unwrap();
        backend
            .write(
                "L3/preferences.md",
                "# Learning preferences\n\n- Visual preference. <!--m_preference-->".into(),
            )
            .unwrap();
        let source = LearnerMemoryKnowledgeSource::new(backend);
        let run = RunContext::new(RunRequest::from_text("*"));
        let all = access(LEARNER_MEMORY_NAMESPACE, "l1,l2,l3", "chat", "preferences");
        let page = source
            .search(
                context(&run, &all),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![],
                    limit: 10,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let ids = page
            .hits
            .iter()
            .map(|hit| hit.reference.item_id.as_str())
            .collect::<BTreeSet<_>>();
        assert!(ids.contains(format!("l1/chat/{}", event.id).as_str()));
        assert!(ids.contains("l2/chat/m_summary"));
        assert!(ids.contains("l3/preferences/m_preference"));

        let l3_only = access(LEARNER_MEMORY_NAMESPACE, "l3", "", "preferences");
        let page = source
            .search(
                context(&run, &l3_only),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![],
                    limit: 10,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(page.hits.len(), 1);
        assert_eq!(
            page.hits[0].reference.item_id,
            "l3/preferences/m_preference"
        );
    }

    #[tokio::test]
    async fn disabled_profile_and_forged_item_fail_closed() {
        let (_temp, source, mut allowed, run) = fixture();
        let forged = source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: KnowledgeRef {
                        source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                        item_id: "l3/preferences/../../secret".into(),
                        revision: Some("forged".into()),
                    },
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(forged.code(), KnowledgeErrorCode::NotFound);

        allowed
            .scope
            .attributes
            .insert(LEARNER_MEMORY_PROFILE_ATTRIBUTE.into(), "disabled".into());
        let disabled = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![],
                    limit: 5,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert_eq!(disabled.code(), KnowledgeErrorCode::Unauthorized);
    }

    #[tokio::test]
    async fn malformed_backend_details_are_sanitized() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        backend
            .write(
                "L3/preferences.md",
                "# Learning preferences\n\n- Broken. <!--m_broken--> <!--llm-tutor-memory:v1:not-base64!!-->"
                    .into(),
            )
            .unwrap();
        let source = LearnerMemoryKnowledgeSource::new(backend);
        let allowed = access(LEARNER_MEMORY_NAMESPACE, "l3", "", "preferences");
        let run = RunContext::new(RunRequest::from_text("*"));
        let error = source
            .search(
                context(&run, &allowed),
                SourceSearchRequest {
                    query: "*".into(),
                    filters: vec![],
                    limit: 5,
                    cursor: None,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code(), KnowledgeErrorCode::Backend);
        assert_eq!(error.to_string(), "knowledge backend failed");
        assert!(
            error
                .diagnostic()
                .unwrap()
                .contains("invalid memory metadata")
        );
    }
}
