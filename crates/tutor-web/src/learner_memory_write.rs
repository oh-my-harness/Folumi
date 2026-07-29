use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{KnowledgeError, KnowledgeRef, KnowledgeRequestContext};
use llm_harness_runtime_memory::{
    MemoryConsistency, MemoryDeleteReceipt, MemoryMutationGate, MemoryMutationGateError,
    MemoryMutationRequest, MemoryPolicyError, MemoryPolicyRejection, MemoryProvenance,
    MemoryService, MemoryServiceBuildError, MemoryStore, MemoryStoreDescriptor, MemoryVisibility,
    MemoryWrite, MemoryWriteIntent, MemoryWritePolicy, MemoryWriteReceipt, SecureMemoryWritePolicy,
    SecureMemoryWritePolicyBuildError, SecureMemoryWritePolicyConfig,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::learner_memory_source::{
    LEARNER_MEMORY_KINDS_ATTRIBUTE, LEARNER_MEMORY_LAYERS_ATTRIBUTE, LEARNER_MEMORY_NAMESPACE,
    LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE, LEARNER_MEMORY_PROFILE_ATTRIBUTE, LEARNER_MEMORY_SOURCE_ID,
    LearnerMemoryKnowledgeSource,
};
use crate::memory_store::{
    DurableMemoryWrite, ExactMemoryDeleteOutcome, FileMemoryBackend, memory_entry_revision,
};

const PREFERENCE_ITEM_PREFIX: &str = "l3/preferences/";
const PREFERENCE_TARGET: &str = "L3/preferences.md";

#[derive(Clone)]
pub struct LearnerMemoryWriteStore {
    backend: Arc<FileMemoryBackend>,
    descriptor: MemoryStoreDescriptor,
}

impl LearnerMemoryWriteStore {
    pub fn new(backend: Arc<FileMemoryBackend>) -> Self {
        Self {
            backend,
            descriptor: MemoryStoreDescriptor {
                read_source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                consistency: MemoryConsistency::Immediate,
            },
        }
    }
}

impl MemoryStore for LearnerMemoryWriteStore {
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
            if write.kind.as_deref() != Some("preference")
                || write.metadata.get("target") != Some(&json!(PREFERENCE_TARGET))
            {
                return Err(KnowledgeError::Unauthorized);
            }
            let entry = self
                .backend
                .upsert_durable_preference(DurableMemoryWrite {
                    content: write.content,
                    kind: "preference".into(),
                    provenance: serde_json::to_value(write.provenance).map_err(backend_error)?,
                    idempotency_key: write.idempotency_key,
                    expires_at: write.expires_at,
                })
                .map_err(backend_error)?;
            let revision = memory_entry_revision(&entry).map_err(backend_error)?;
            Ok(MemoryWriteReceipt {
                reference: KnowledgeRef {
                    source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                    item_id: format!("{PREFERENCE_ITEM_PREFIX}{}", entry.marker),
                    revision: Some(revision),
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
            if reference.source_id != LEARNER_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            let marker = reference
                .item_id
                .strip_prefix(PREFERENCE_ITEM_PREFIX)
                .filter(|marker| !marker.is_empty() && !marker.contains('/'))
                .ok_or(KnowledgeError::NotFound)?;
            let expected_revision = reference
                .revision
                .as_deref()
                .ok_or(KnowledgeError::StaleReference { latest: None })?;
            match self
                .backend
                .delete_durable_preference(marker, expected_revision)
                .map_err(backend_error)?
            {
                ExactMemoryDeleteOutcome::Deleted => Ok(MemoryDeleteReceipt {
                    reference,
                    visibility: MemoryVisibility::Visible,
                }),
                ExactMemoryDeleteOutcome::NotFound => Err(KnowledgeError::NotFound),
                ExactMemoryDeleteOutcome::Stale { latest_revision } => {
                    Err(KnowledgeError::StaleReference {
                        latest: Some(KnowledgeRef {
                            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
                            item_id: reference.item_id,
                            revision: Some(latest_revision),
                        }),
                    })
                }
            }
        })
    }
}

pub struct LearnerMemoryWritePolicy {
    inner: SecureMemoryWritePolicy,
}

impl LearnerMemoryWritePolicy {
    pub fn new(secret: Vec<u8>) -> Result<Self, SecureMemoryWritePolicyBuildError> {
        let mut metadata = std::collections::BTreeMap::new();
        metadata.insert("target".into(), json!(PREFERENCE_TARGET));
        let inner = SecureMemoryWritePolicy::new(
            secret,
            SecureMemoryWritePolicyConfig {
                max_content_bytes: 4 * 1_200,
                allowed_kinds: Some(BTreeSet::from(["preference".into()])),
                default_ttl: None,
                max_ttl: Duration::from_secs(365 * 24 * 60 * 60),
                metadata,
            },
        )?;
        Ok(Self { inner })
    }
}

impl MemoryWritePolicy for LearnerMemoryWritePolicy {
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

pub trait LearnerMemoryApprover: Send + Sync {
    fn authorize<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>>;
}

pub struct LearnerMemoryMutationGate {
    approver: Arc<dyn LearnerMemoryApprover>,
}

impl LearnerMemoryMutationGate {
    pub fn new(approver: Arc<dyn LearnerMemoryApprover>) -> Self {
        Self { approver }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LearnerMemoryServiceBuildError {
    #[error(transparent)]
    Policy(#[from] SecureMemoryWritePolicyBuildError),
    #[error(transparent)]
    Service(#[from] MemoryServiceBuildError),
}

pub fn assemble_learner_memory_service(
    source: Arc<LearnerMemoryKnowledgeSource>,
    backend: Arc<FileMemoryBackend>,
    access_control: Arc<llm_harness_runtime_knowledge::KnowledgeAccessControl>,
    policy_secret: Vec<u8>,
    approver: Arc<dyn LearnerMemoryApprover>,
) -> Result<MemoryService, LearnerMemoryServiceBuildError> {
    let store = Arc::new(LearnerMemoryWriteStore::new(backend));
    let policy = Arc::new(LearnerMemoryWritePolicy::new(policy_secret)?);
    let gate = Arc::new(LearnerMemoryMutationGate::new(approver));
    Ok(MemoryService::new(
        access_control,
        source,
        store,
        policy,
        gate,
    )?)
}

impl MemoryMutationGate for LearnerMemoryMutationGate {
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

fn authorize_mutation(
    ctx: KnowledgeRequestContext<'_>,
    abort: &CancellationToken,
) -> Result<(), KnowledgeError> {
    if abort.is_cancelled() {
        return Err(KnowledgeError::Aborted);
    }
    let access = ctx.access;
    let attributes = &access.scope.attributes;
    let allowed = access.scope.namespace == LEARNER_MEMORY_NAMESPACE
        && !access.principal.subject.trim().is_empty()
        && attributes
            .get(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE)
            .is_some_and(|subject| subject == &access.principal.subject)
        && attributes
            .get(LEARNER_MEMORY_PROFILE_ATTRIBUTE)
            .is_some_and(|profile| profile == "interactive_mutation")
        && csv_contains(attributes.get(LEARNER_MEMORY_LAYERS_ATTRIBUTE), "l3")
        && csv_contains(
            attributes.get(LEARNER_MEMORY_KINDS_ATTRIBUTE),
            "preferences",
        );
    if allowed {
        Ok(())
    } else {
        Err(KnowledgeError::Unauthorized)
    }
}

fn csv_contains(value: Option<&String>, expected: &str) -> bool {
    value.is_some_and(|value| value.split(',').map(str::trim).any(|item| item == expected))
}

fn backend_error(error: impl std::fmt::Display) -> KnowledgeError {
    KnowledgeError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    use chrono::Utc;
    use llm_harness_runtime_knowledge::{
        AuthorizationDecision, ContentSelector, KnowledgeAccessContext, KnowledgeAccessControl,
        KnowledgeAction, KnowledgeAuthorizer, KnowledgeReadRequest, KnowledgeResourceRef,
        KnowledgeScope, KnowledgeSource, PrincipalRef,
    };
    use llm_harness_runtime_memory::contract::{
        MemoryStoreContractCase, verify_memory_store_contract,
    };
    use llm_harness_runtime_memory::{
        MemoryMutation, MemoryMutationOrigin, MemoryProvenance, MemorySessionId,
    };
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;
    use crate::learner_memory_source::{
        LEARNER_MEMORY_SURFACES_ATTRIBUTE, LearnerMemoryKnowledgeSource,
    };
    use crate::memory_store::try_parse_memory_entries;

    fn access(profile: &str, principal: &str) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(LEARNER_MEMORY_NAMESPACE);
        scope
            .attributes
            .insert(LEARNER_MEMORY_PROFILE_ATTRIBUTE.into(), profile.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE.into(), principal.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_LAYERS_ATTRIBUTE.into(), "l3".into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_KINDS_ATTRIBUTE.into(), "preferences".into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_SURFACES_ATTRIBUTE.into(), String::new());
        KnowledgeAccessContext::new(scope, PrincipalRef::new(principal, "user"))
    }

    fn context<'a>(
        run: &'a RunContext,
        access: &'a KnowledgeAccessContext,
    ) -> KnowledgeRequestContext<'a> {
        KnowledgeRequestContext { run, access }
    }

    fn write(run: &RunContext, key: &str, content: &str) -> MemoryWrite {
        MemoryWrite {
            content: content.into(),
            kind: Some("preference".into()),
            metadata: BTreeMap::from([("target".into(), json!(PREFERENCE_TARGET))]),
            provenance: MemoryProvenance {
                run_id: run.id(),
                session_id: Some("session-1".into()),
                origin: MemoryMutationOrigin::Application {
                    operation_id: "test-write".into(),
                },
                recorded_at: Utc::now(),
            },
            idempotency_key: key.into(),
            expires_at: None,
        }
    }

    #[tokio::test]
    async fn learner_memory_store_passes_runtime_contract() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let store = LearnerMemoryWriteStore::new(backend);
        let run = RunContext::new(RunRequest::from_text("remember"));
        let allowed = access("interactive_mutation", "local-user");
        let denied = access("read_only", "local-user");
        let write = write(&run, "contract-key", "Prefers concise explanations.");
        let expected = store
            .upsert(
                context(&run, &allowed),
                write.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap()
            .reference;

        verify_memory_store_contract(
            &store,
            &run,
            &MemoryStoreContractCase {
                allowed_access: allowed,
                denied_access: denied,
                write,
                expected_reference: expected,
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn concurrent_retries_create_one_entry_and_one_receipt() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let store = Arc::new(LearnerMemoryWriteStore::new(backend.clone()));
        let run = Arc::new(RunContext::new(RunRequest::from_text("remember")));
        let allowed = Arc::new(access("interactive_mutation", "local-user"));
        let handles = (0..2)
            .map(|_| {
                let store = store.clone();
                let run = run.clone();
                let allowed = allowed.clone();
                let write = write(&run, "same-key", "Prefers visual examples.");
                tokio::spawn(async move {
                    store
                        .upsert(context(&run, &allowed), write, CancellationToken::new())
                        .await
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let mut receipts = Vec::new();
        for handle in handles {
            receipts.push(handle.await.unwrap());
        }

        assert_eq!(receipts[0], receipts[1]);
        let entries =
            try_parse_memory_entries(&backend.read(PREFERENCE_TARGET).unwrap().markdown).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn receipt_reads_back_and_stale_delete_does_not_mutate() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let store = LearnerMemoryWriteStore::new(backend.clone());
        let source = LearnerMemoryKnowledgeSource::new(backend.clone());
        let run = RunContext::new(RunRequest::from_text("remember"));
        let allowed = access("interactive_mutation", "local-user");
        let receipt = store
            .upsert(
                context(&run, &allowed),
                write(&run, "readback-key", "Prefers diagrams."),
                CancellationToken::new(),
            )
            .await
            .unwrap();

        source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: receipt.reference.clone(),
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let stale = store
            .delete(
                context(&run, &allowed),
                KnowledgeRef {
                    revision: Some("stale".into()),
                    ..receipt.reference.clone()
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            stale,
            KnowledgeError::StaleReference { latest: Some(_) }
        ));
        source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: receipt.reference,
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn product_policy_defaults_to_preference_and_rejects_secrets() {
        let policy = LearnerMemoryWritePolicy::new(vec![7; 32]).unwrap();
        let run = RunContext::new(RunRequest::from_text("remember"));
        let allowed = access("interactive_mutation", "local-user");
        let provenance = MemoryProvenance {
            run_id: run.id(),
            session_id: None,
            origin: MemoryMutationOrigin::Application {
                operation_id: "policy-test".into(),
            },
            recorded_at: Utc::now(),
        };
        let prepared = policy
            .prepare(
                context(&run, &allowed),
                MemoryWriteIntent {
                    content: "  Prefers examples  ".into(),
                    kind: None,
                    requested_ttl: Some(Duration::from_secs(60)),
                },
                provenance.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(prepared.kind.as_deref(), Some("preference"));
        assert_eq!(prepared.content, "Prefers examples");
        assert_eq!(prepared.metadata["target"], PREFERENCE_TARGET);
        assert!(prepared.expires_at.is_some());

        let rejected = match policy
            .prepare(
                context(&run, &allowed),
                MemoryWriteIntent {
                    content: "password = do-not-store-this".into(),
                    kind: Some("preference".into()),
                    requested_ttl: None,
                },
                provenance,
                CancellationToken::new(),
            )
            .await
        {
            Ok(_) => panic!("secret-like memory content must be rejected"),
            Err(error) => error,
        };
        assert!(matches!(
            rejected,
            MemoryPolicyError::Rejected(
                llm_harness_runtime_memory::MemoryPolicyRejection::SensitiveContent
            )
        ));

        let oversized = match policy
            .prepare(
                context(&run, &allowed),
                MemoryWriteIntent {
                    content: "例".repeat(1_201),
                    kind: Some("preference".into()),
                    requested_ttl: None,
                },
                MemoryProvenance {
                    run_id: run.id(),
                    session_id: None,
                    origin: MemoryMutationOrigin::Application {
                        operation_id: "oversized".into(),
                    },
                    recorded_at: Utc::now(),
                },
                CancellationToken::new(),
            )
            .await
        {
            Ok(_) => panic!("product character limit must be enforced by policy"),
            Err(error) => error,
        };
        assert!(matches!(
            oversized,
            MemoryPolicyError::Rejected(
                llm_harness_runtime_memory::MemoryPolicyRejection::ContentTooLarge
            )
        ));
    }

    #[derive(Default)]
    struct RecordingApprover {
        mutations: Mutex<Vec<String>>,
    }

    impl LearnerMemoryApprover for RecordingApprover {
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
                let summary = match request.mutation {
                    MemoryMutation::Write { write } => format!("write:{}", write.content),
                    MemoryMutation::Delete { reference } => {
                        format!("delete:{}", reference.item_id)
                    }
                };
                self.mutations.lock().unwrap().push(summary);
                Ok(())
            })
        }
    }

    struct InteractiveAuthorizer;

    impl KnowledgeAuthorizer for InteractiveAuthorizer {
        fn authorize<'a>(
            &'a self,
            access: &'a KnowledgeAccessContext,
            _action: KnowledgeAction,
            _resource: KnowledgeResourceRef<'a>,
        ) -> BoxFuture<'a, Result<AuthorizationDecision, KnowledgeError>> {
            Box::pin(async move {
                Ok(
                    if access
                        .scope
                        .attributes
                        .get(LEARNER_MEMORY_PROFILE_ATTRIBUTE)
                        == Some(&"interactive_mutation".into())
                    {
                        AuthorizationDecision::Allow
                    } else {
                        AuthorizationDecision::Deny
                    },
                )
            })
        }
    }

    #[tokio::test]
    async fn memory_service_gates_write_and_delete_and_undo_restores_delete() {
        let dir = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(dir.path().join("memory")));
        let source = Arc::new(LearnerMemoryKnowledgeSource::new(backend.clone()));
        let approver = Arc::new(RecordingApprover::default());
        let service = assemble_learner_memory_service(
            source.clone(),
            backend.clone(),
            Arc::new(KnowledgeAccessControl::new(Arc::new(InteractiveAuthorizer))),
            vec![9; 32],
            approver.clone(),
        )
        .unwrap();
        let request = RunRequest::from_text("remember")
            .with_extension(MemorySessionId::new("session-1").unwrap());
        let run = RunContext::new(request);
        let allowed = access("interactive_mutation", "local-user");
        let origin = MemoryMutationOrigin::Application {
            operation_id: "service-write".into(),
        };
        let receipt = service
            .write(
                context(&run, &allowed),
                MemoryWriteIntent {
                    content: "Prefers diagrams.".into(),
                    kind: None,
                    requested_ttl: None,
                },
                origin.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: receipt.reference.clone(),
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();

        service
            .delete(
                context(&run, &allowed),
                receipt.reference.clone(),
                origin,
                CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(approver.mutations.lock().unwrap().len(), 2);
        backend.undo_latest_write(PREFERENCE_TARGET).unwrap();
        source
            .read(
                context(&run, &allowed),
                KnowledgeReadRequest {
                    reference: receipt.reference,
                    selector: ContentSelector::Document,
                    max_bytes: 4096,
                },
                CancellationToken::new(),
            )
            .await
            .unwrap();
    }
}
