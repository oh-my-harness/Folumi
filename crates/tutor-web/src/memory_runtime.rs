use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    ContentSelector, FreshnessClass, KnowledgeCapability, KnowledgeContent, KnowledgeError,
    KnowledgeHit, KnowledgeReadRequest, KnowledgeRef, KnowledgeRequestContext, KnowledgeSource,
    KnowledgeSourceDescriptor, SourceSearchPage, SourceSearchRequest,
};
use llm_harness_runtime_memory::{
    MemoryConsistency, MemoryDeleteReceipt, MemoryMutationGate, MemoryMutationGateError,
    MemoryMutationRequest, MemoryPolicyError, MemoryPolicyRejection, MemoryProvenance,
    MemoryService, MemoryServiceBuildError, MemoryStore as RuntimeMemoryStore,
    MemoryStoreDescriptor, MemoryVisibility, MemoryWrite, MemoryWriteIntent, MemoryWritePolicy,
    MemoryWriteReceipt, SecureMemoryWritePolicy, SecureMemoryWritePolicyBuildError,
    SecureMemoryWritePolicyConfig,
};
use llm_harness_types::DataBlock;
use tokio_util::sync::CancellationToken;

use crate::memory_store::{
    ConflictAction, CreateMemoryItem, MemoryKind, MemoryOrigin, MemoryPriority, MemorySourceRef,
    MemoryStore, MemoryStoreError,
};

pub const USER_MEMORY_SOURCE_ID: &str = "folumi.user-memory";
pub const USER_MEMORY_PROFILE_ATTRIBUTE: &str = "folumi_memory_profile";
pub const USER_MEMORY_PRINCIPAL_ATTRIBUTE: &str = "folumi_memory_principal";

const MEMORY_SEARCH_SNIPPET: &str =
    "Saved Memory match. Use knowledge_read with the exact revision to read its content.";

#[derive(Clone)]
pub struct SavedMemoryKnowledgeSource {
    store: Arc<MemoryStore>,
    descriptor: KnowledgeSourceDescriptor,
}

impl SavedMemoryKnowledgeSource {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self {
            store,
            descriptor: KnowledgeSourceDescriptor {
                id: USER_MEMORY_SOURCE_ID.into(),
                name: "Saved Memory".into(),
                description: "User-confirmed facts, preferences, goals, and continuity items."
                    .into(),
                domains: [
                    "user-memory.fact",
                    "user-memory.preference",
                    "user-memory.goal",
                    "user-memory.continuity",
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
                filter_fields: Vec::new(),
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
        let access = ctx.access;
        let profile = access
            .scope
            .attributes
            .get(USER_MEMORY_PROFILE_ATTRIBUTE)
            .map(String::as_str);
        let principal = access
            .scope
            .attributes
            .get(USER_MEMORY_PRINCIPAL_ATTRIBUTE)
            .map(String::as_str);
        if access.scope.namespace != tutor_rag::AGENT_KNOWLEDGE_NAMESPACE
            || access.principal.subject.trim().is_empty()
            || principal != Some(access.principal.subject.as_str())
            || !matches!(profile, Some("read_only" | "interactive_mutation"))
        {
            return Err(KnowledgeError::Unauthorized);
        }
        Ok(())
    }
}

impl KnowledgeSource for SavedMemoryKnowledgeSource {
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
            if !request.filters.is_empty() {
                return Err(KnowledgeError::InvalidFilter(
                    "Saved Memory does not expose model-selected filters".into(),
                ));
            }
            if request.cursor.is_some() {
                return Err(KnowledgeError::InvalidCursor);
            }
            let items = self
                .store
                .recall(&request.query, request.limit.min(20))
                .map_err(backend_error)?;
            if abort.is_cancelled() {
                return Err(KnowledgeError::Aborted);
            }
            Ok(SourceSearchPage {
                hits: items
                    .into_iter()
                    .map(|item| KnowledgeHit {
                        reference: KnowledgeRef {
                            source_id: USER_MEMORY_SOURCE_ID.into(),
                            item_id: item.id.clone(),
                            revision: Some(item.revision.clone()),
                        },
                        title: Some(format!("Saved {}", item.kind.as_str())),
                        snippet: MEMORY_SEARCH_SNIPPET.into(),
                        suggested_selectors: vec![ContentSelector::Document],
                        uri: Some(format!("folumi://memory/{}", item.id)),
                        score: None,
                        updated_at: Some(item.updated_at),
                        metadata: BTreeMap::from([
                            ("kind".into(), serde_json::json!(item.kind)),
                            ("priority".into(), serde_json::json!(item.priority)),
                        ]),
                    })
                    .collect(),
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
            if request.reference.source_id != USER_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            if request.selector != ContentSelector::Document {
                return Err(KnowledgeError::UnsupportedCapability(
                    "Saved Memory supports exact document reads only".into(),
                ));
            }
            let item = self
                .store
                .get(&request.reference.item_id)
                .map_err(|error| match error {
                    MemoryStoreError::NotFound => KnowledgeError::NotFound,
                    other => backend_error(other),
                })?;
            if item.status != crate::memory_store::MemoryStatus::Active || item.expired {
                return Err(KnowledgeError::NotFound);
            }
            if request.reference.revision.as_deref() != Some(item.revision.as_str()) {
                return Err(KnowledgeError::StaleReference {
                    latest: Some(KnowledgeRef {
                        source_id: USER_MEMORY_SOURCE_ID.into(),
                        item_id: item.id,
                        revision: Some(item.revision),
                    }),
                });
            }
            if abort.is_cancelled() {
                return Err(KnowledgeError::Aborted);
            }
            let (content, truncated) = truncate_utf8(&item.content, request.max_bytes);
            Ok(KnowledgeContent {
                reference: request.reference,
                selector: request.selector,
                title: Some(format!("Saved {}", item.kind.as_str())),
                blocks: vec![DataBlock::text(content)],
                uri: Some(format!("folumi://memory/{}", item.id)),
                updated_at: Some(item.updated_at),
                obtained_at: Utc::now(),
                truncated,
                metadata: BTreeMap::from([
                    ("kind".into(), serde_json::json!(item.kind)),
                    ("topic_key".into(), serde_json::json!(item.topic_key)),
                ]),
            })
        })
    }
}

#[derive(Clone)]
pub struct SavedMemoryWriteStore {
    store: Arc<MemoryStore>,
    descriptor: MemoryStoreDescriptor,
}

impl SavedMemoryWriteStore {
    pub fn new(store: Arc<MemoryStore>) -> Self {
        Self {
            store,
            descriptor: MemoryStoreDescriptor {
                read_source_id: USER_MEMORY_SOURCE_ID.into(),
                consistency: MemoryConsistency::Immediate,
            },
        }
    }
}

impl RuntimeMemoryStore for SavedMemoryWriteStore {
    fn descriptor(&self) -> &MemoryStoreDescriptor {
        &self.descriptor
    }

    fn upsert<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        write: MemoryWrite,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<MemoryWriteReceipt, KnowledgeError>> {
        Box::pin(async move {
            authorize_mutation(ctx, &abort)?;
            let kind = write
                .kind
                .as_deref()
                .and_then(MemoryKind::parse)
                .ok_or(KnowledgeError::Unauthorized)?;
            let source_refs = write
                .provenance
                .session_id
                .as_ref()
                .map(|session_id| {
                    vec![MemorySourceRef {
                        source_type: "session".into(),
                        source_id: session_id.clone(),
                        source_revision: None,
                        metadata: serde_json::json!({ "run_id": write.provenance.run_id }),
                    }]
                })
                .unwrap_or_default();
            let item = self
                .store
                .create(CreateMemoryItem {
                    kind,
                    content: write.content,
                    topic_key: None,
                    priority: MemoryPriority::Normal,
                    origin: MemoryOrigin::AssistantSuggested,
                    source_refs,
                    provenance: serde_json::to_value(write.provenance).map_err(backend_error)?,
                    valid_until: write.expires_at,
                    idempotency_key: Some(write.idempotency_key),
                    conflict_action: ConflictAction::Reject,
                })
                .map_err(backend_error)?;
            Ok(MemoryWriteReceipt {
                reference: KnowledgeRef {
                    source_id: USER_MEMORY_SOURCE_ID.into(),
                    item_id: item.id,
                    revision: Some(item.revision),
                },
                visibility: MemoryVisibility::Visible,
            })
        })
    }

    fn delete<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        reference: KnowledgeRef,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<MemoryDeleteReceipt, KnowledgeError>> {
        Box::pin(async move {
            authorize_mutation(ctx, &abort)?;
            if reference.source_id != USER_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            let revision = reference
                .revision
                .as_deref()
                .ok_or(KnowledgeError::StaleReference { latest: None })?;
            self.store
                .forget(&reference.item_id, revision)
                .map_err(|error| match error {
                    MemoryStoreError::NotFound => KnowledgeError::NotFound,
                    MemoryStoreError::Stale { latest } => KnowledgeError::StaleReference {
                        latest: Some(KnowledgeRef {
                            source_id: USER_MEMORY_SOURCE_ID.into(),
                            item_id: latest.id,
                            revision: Some(latest.revision),
                        }),
                    },
                    other => backend_error(other),
                })?;
            Ok(MemoryDeleteReceipt {
                reference,
                visibility: MemoryVisibility::Visible,
            })
        })
    }
}

pub struct SavedMemoryWritePolicy {
    inner: SecureMemoryWritePolicy,
}

impl SavedMemoryWritePolicy {
    pub fn new(secret: Vec<u8>) -> Result<Self, SecureMemoryWritePolicyBuildError> {
        Ok(Self {
            inner: SecureMemoryWritePolicy::new(
                secret,
                SecureMemoryWritePolicyConfig {
                    max_content_bytes: 4 * 1_200,
                    allowed_kinds: Some(BTreeSet::from([
                        "fact".into(),
                        "preference".into(),
                        "goal".into(),
                        "continuity".into(),
                    ])),
                    default_ttl: None,
                    max_ttl: Duration::from_secs(5 * 365 * 24 * 60 * 60),
                    metadata: BTreeMap::new(),
                },
            )?,
        })
    }
}

impl MemoryWritePolicy for SavedMemoryWritePolicy {
    fn prepare<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        mut intent: MemoryWriteIntent,
        provenance: MemoryProvenance,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<MemoryWrite, MemoryPolicyError>> {
        if intent.kind.is_none() {
            intent.kind = Some("preference".into());
        }
        Box::pin(async move {
            let write = self.inner.prepare(ctx, intent, provenance, abort).await?;
            if write.content.chars().count() > 1_200 {
                return Err(MemoryPolicyError::Rejected(
                    MemoryPolicyRejection::ContentTooLarge,
                ));
            }
            Ok(write)
        })
    }
}

pub trait SavedMemoryApprover: Send + Sync {
    fn authorize<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>>;
}

pub struct SavedMemoryMutationGate {
    approver: Arc<dyn SavedMemoryApprover>,
}

impl SavedMemoryMutationGate {
    pub fn new(approver: Arc<dyn SavedMemoryApprover>) -> Self {
        Self { approver }
    }
}

impl MemoryMutationGate for SavedMemoryMutationGate {
    fn authorize<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>> {
        Box::pin(async move {
            authorize_mutation(ctx, &abort).map_err(|error| match error {
                KnowledgeError::Aborted => MemoryMutationGateError::Aborted,
                _ => MemoryMutationGateError::Unavailable,
            })?;
            self.approver.authorize(ctx, request, abort).await
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SavedMemoryServiceBuildError {
    #[error(transparent)]
    Policy(#[from] SecureMemoryWritePolicyBuildError),
    #[error(transparent)]
    Service(#[from] MemoryServiceBuildError),
    #[error(transparent)]
    Store(#[from] MemoryStoreError),
}

pub fn assemble_saved_memory_service(
    source: Arc<SavedMemoryKnowledgeSource>,
    store: Arc<MemoryStore>,
    access_control: Arc<llm_harness_runtime_knowledge::KnowledgeAccessControl>,
    approver: Arc<dyn SavedMemoryApprover>,
) -> Result<MemoryService, SavedMemoryServiceBuildError> {
    Ok(MemoryService::new(
        access_control,
        source,
        Arc::new(SavedMemoryWriteStore::new(store.clone())),
        Arc::new(SavedMemoryWritePolicy::new(store.policy_secret()?)?),
        Arc::new(SavedMemoryMutationGate::new(approver)),
    )?)
}

fn authorize_mutation(
    ctx: KnowledgeRequestContext<'_>,
    abort: &CancellationToken,
) -> Result<(), KnowledgeError> {
    if abort.is_cancelled() {
        return Err(KnowledgeError::Aborted);
    }
    let access = ctx.access;
    let allowed = access.scope.namespace == tutor_rag::AGENT_KNOWLEDGE_NAMESPACE
        && !access.principal.subject.trim().is_empty()
        && access
            .scope
            .attributes
            .get(USER_MEMORY_PRINCIPAL_ATTRIBUTE)
            .is_some_and(|subject| subject == &access.principal.subject)
        && access
            .scope
            .attributes
            .get(USER_MEMORY_PROFILE_ATTRIBUTE)
            .is_some_and(|profile| profile == "interactive_mutation");
    if allowed {
        Ok(())
    } else {
        Err(KnowledgeError::Unauthorized)
    }
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    (value[..end].to_string(), true)
}

fn backend_error(error: impl std::fmt::Display) -> KnowledgeError {
    KnowledgeError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use futures::future::BoxFuture;
    use llm_harness_runtime_knowledge::{
        KnowledgeAccessContext, KnowledgeRequestContext, KnowledgeScope, PrincipalRef,
    };
    use llm_harness_runtime_memory::{
        MemoryMutation, MemoryMutationGateError, MemoryMutationOrigin, MemoryMutationRequest,
        MemoryServiceError, MemorySessionId, MemoryWriteIntent,
    };
    use llm_harness_types::{RunContext, RunRequest};
    use tokio_util::sync::CancellationToken;

    use super::*;

    struct RecordingApprover {
        approved: bool,
        calls: AtomicUsize,
        presented: Mutex<Vec<(String, Option<String>)>>,
    }

    impl RecordingApprover {
        fn new(approved: bool) -> Self {
            Self {
                approved,
                calls: AtomicUsize::new(0),
                presented: Mutex::new(Vec::new()),
            }
        }
    }

    impl SavedMemoryApprover for RecordingApprover {
        fn authorize<'a>(
            &'a self,
            _ctx: KnowledgeRequestContext<'a>,
            request: MemoryMutationRequest,
            abort: CancellationToken,
        ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>> {
            Box::pin(async move {
                if abort.is_cancelled() {
                    return Err(MemoryMutationGateError::Aborted);
                }
                self.calls.fetch_add(1, Ordering::SeqCst);
                if let MemoryMutation::Write { write } = request.mutation {
                    self.presented
                        .lock()
                        .unwrap()
                        .push((write.content, write.kind));
                }
                if self.approved {
                    Ok(())
                } else {
                    Err(MemoryMutationGateError::Denied)
                }
            })
        }
    }

    fn run_context() -> RunContext {
        let mut scope = KnowledgeScope::new(tutor_rag::AGENT_KNOWLEDGE_NAMESPACE);
        scope.attributes.insert(
            USER_MEMORY_PROFILE_ATTRIBUTE.into(),
            "interactive_mutation".into(),
        );
        scope
            .attributes
            .insert(USER_MEMORY_PRINCIPAL_ATTRIBUTE.into(), "local-user".into());
        let access =
            KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "local_user"));
        RunContext::new(
            RunRequest::from_text("请记住我偏好中文。")
                .with_extension(access)
                .with_extension(MemorySessionId::new("session-a").unwrap()),
        )
    }

    fn service(store: Arc<MemoryStore>, approver: Arc<dyn SavedMemoryApprover>) -> MemoryService {
        assemble_saved_memory_service(
            Arc::new(SavedMemoryKnowledgeSource::new(store.clone())),
            store,
            crate::knowledge_runtime::agent_knowledge_access_control(),
            approver,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn approved_runtime_write_persists_exact_presented_content_and_session_source() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            Arc::new(MemoryStore::new_with_path(directory.path().join("memory.sqlite3")).unwrap());
        let approver = Arc::new(RecordingApprover::new(true));
        let service = service(store.clone(), approver.clone());
        let run = run_context();
        let context = KnowledgeRequestContext::from_run(&run).unwrap();

        let receipt = service
            .write(
                context,
                MemoryWriteIntent {
                    content: "  请使用简洁的中文回答。  ".into(),
                    kind: Some("preference".into()),
                    requested_ttl: None,
                },
                MemoryMutationOrigin::ExplicitTool {
                    tool_use_id: "memory-write-a".into(),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        assert_eq!(approver.calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            approver.presented.lock().unwrap().as_slice(),
            &[("请使用简洁的中文回答。".into(), Some("preference".into()))]
        );
        let item = store.get(&receipt.reference.item_id).unwrap();
        assert_eq!(item.content, "请使用简洁的中文回答。");
        assert_eq!(item.origin, MemoryOrigin::AssistantSuggested);
        assert_eq!(item.source_refs.len(), 1);
        assert_eq!(item.source_refs[0].source_type, "session");
        assert_eq!(item.source_refs[0].source_id, "session-a");
    }

    #[tokio::test]
    async fn denied_runtime_write_never_reaches_the_store() {
        let directory = tempfile::tempdir().unwrap();
        let store =
            Arc::new(MemoryStore::new_with_path(directory.path().join("memory.sqlite3")).unwrap());
        let approver = Arc::new(RecordingApprover::new(false));
        let service = service(store.clone(), approver.clone());
        let run = run_context();
        let context = KnowledgeRequestContext::from_run(&run).unwrap();

        let error = service
            .write(
                context,
                MemoryWriteIntent {
                    content: "Never persist this denied memory.".into(),
                    kind: Some("fact".into()),
                    requested_ttl: None,
                },
                MemoryMutationOrigin::ExplicitTool {
                    tool_use_id: "memory-write-denied".into(),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            MemoryServiceError::MutationGate(MemoryMutationGateError::Denied)
        ));
        assert_eq!(approver.calls.load(Ordering::SeqCst), 1);
        assert!(store.list(&Default::default()).unwrap().is_empty());
    }
}
