use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    KnowledgeAccessControl, KnowledgeError, KnowledgeRef, KnowledgeRequestContext,
};
use llm_harness_runtime_memory::{
    MemoryConsistency, MemoryDeleteReceipt, MemoryMutationGate, MemoryMutationGateError,
    MemoryMutationRequest, MemoryPolicyError, MemoryPolicyRejection, MemoryProvenance,
    MemoryService, MemoryServiceBuildError, MemoryStore, MemoryStoreDescriptor, MemoryVisibility,
    MemoryWrite, MemoryWriteIntent, MemoryWritePolicy, MemoryWriteReceipt, SecureMemoryWritePolicy,
    SecureMemoryWritePolicyBuildError, SecureMemoryWritePolicyConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio_util::sync::CancellationToken;

use crate::tutor_memory_source::{
    TUTOR_MEMORY_MODE_ATTRIBUTE, TUTOR_MEMORY_SOURCE_ID, TUTOR_MEMORY_TUTOR_ID_ATTRIBUTE,
    TutorMemoryKnowledgeSource,
};
use crate::tutor_memory_store::{
    ExactTutorMemoryDeleteOutcome, RuntimeTutorMemoryWrite, TutorMemoryKind, TutorMemoryStore,
    tutor_memory_entry_revision,
};

const ITEM_PREFIX: &str = "entry/";
const MAX_MEMORY_TEXT_CHARS: usize = 2_000;
const MAX_NEXT_ACTION_CHARS: usize = 500;
const MAX_SOURCE_MESSAGE_ID_CHARS: usize = 256;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TutorMemoryWritePayload {
    pub text: String,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub source_message_id: Option<String>,
}

pub fn tutor_memory_write_intent(
    kind: TutorMemoryKind,
    text: String,
    next_action: Option<String>,
    source_message_id: Option<String>,
) -> Result<MemoryWriteIntent, serde_json::Error> {
    Ok(MemoryWriteIntent {
        content: serde_json::to_string(&TutorMemoryWritePayload {
            text,
            next_action,
            source_message_id,
        })?,
        kind: Some(kind_name(kind).into()),
        requested_ttl: None,
    })
}

pub struct TutorMemoryWriteStore {
    store: Arc<TutorMemoryStore>,
    tutor_id: String,
    descriptor: MemoryStoreDescriptor,
}

impl TutorMemoryWriteStore {
    pub fn new(store: Arc<TutorMemoryStore>, tutor_id: impl Into<String>) -> Self {
        Self {
            store,
            tutor_id: tutor_id.into(),
            descriptor: MemoryStoreDescriptor {
                read_source_id: TUTOR_MEMORY_SOURCE_ID.into(),
                consistency: MemoryConsistency::Immediate,
            },
        }
    }
}

impl MemoryStore for TutorMemoryWriteStore {
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
            authorize_mutation(ctx, &self.tutor_id, &abort)?;
            let kind = write
                .kind
                .as_deref()
                .and_then(parse_kind)
                .ok_or(KnowledgeError::Unauthorized)?;
            let next_action = optional_metadata_string(&write.metadata, "next_action")?;
            let source_message_id = optional_metadata_string(&write.metadata, "source_message_id")?;
            let entry = self
                .store
                .upsert_runtime(
                    &self.tutor_id,
                    RuntimeTutorMemoryWrite {
                        kind,
                        text: write.content,
                        next_action,
                        source_session_id: write.provenance.session_id.clone(),
                        source_message_id,
                        idempotency_key: write.idempotency_key,
                        provenance: serde_json::to_value(write.provenance)
                            .map_err(backend_error)?,
                    },
                )
                .map_err(backend_error)?;
            Ok(MemoryWriteReceipt {
                reference: KnowledgeRef {
                    source_id: TUTOR_MEMORY_SOURCE_ID.into(),
                    item_id: format!("{ITEM_PREFIX}{}", entry.id),
                    revision: Some(tutor_memory_entry_revision(&entry)),
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
            authorize_mutation(ctx, &self.tutor_id, &abort)?;
            if reference.source_id != TUTOR_MEMORY_SOURCE_ID {
                return Err(KnowledgeError::NotFound);
            }
            let entry_id = reference
                .item_id
                .strip_prefix(ITEM_PREFIX)
                .filter(|entry_id| !entry_id.is_empty() && !entry_id.contains('/'))
                .ok_or(KnowledgeError::NotFound)?;
            let expected_revision = reference
                .revision
                .as_deref()
                .ok_or(KnowledgeError::StaleReference { latest: None })?;
            match self
                .store
                .delete_exact(&self.tutor_id, entry_id, expected_revision)
                .map_err(backend_error)?
            {
                ExactTutorMemoryDeleteOutcome::Deleted => Ok(MemoryDeleteReceipt {
                    reference,
                    visibility: MemoryVisibility::Visible,
                }),
                ExactTutorMemoryDeleteOutcome::NotFound => Err(KnowledgeError::NotFound),
                ExactTutorMemoryDeleteOutcome::Stale { latest_revision } => {
                    Err(KnowledgeError::StaleReference {
                        latest: (!latest_revision.is_empty()).then_some(KnowledgeRef {
                            source_id: TUTOR_MEMORY_SOURCE_ID.into(),
                            item_id: reference.item_id,
                            revision: Some(latest_revision),
                        }),
                    })
                }
            }
        })
    }
}

pub struct TutorMemoryWritePolicy {
    inner: SecureMemoryWritePolicy,
}

impl TutorMemoryWritePolicy {
    pub fn new(secret: Vec<u8>) -> Result<Self, SecureMemoryWritePolicyBuildError> {
        let allowed_kinds = [
            TutorMemoryKind::Commitment,
            TutorMemoryKind::OpenLoop,
            TutorMemoryKind::LessonPlan,
            TutorMemoryKind::Reflection,
            TutorMemoryKind::Strategy,
        ]
        .into_iter()
        .map(kind_name)
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
        let inner = SecureMemoryWritePolicy::new(
            secret,
            SecureMemoryWritePolicyConfig {
                max_content_bytes: 16 * 1024,
                allowed_kinds: Some(allowed_kinds),
                default_ttl: None,
                max_ttl: Duration::from_secs(365 * 24 * 60 * 60),
                metadata: BTreeMap::new(),
            },
        )?;
        Ok(Self { inner })
    }
}

impl MemoryWritePolicy for TutorMemoryWritePolicy {
    fn prepare<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        intent: MemoryWriteIntent,
        provenance: MemoryProvenance,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<MemoryWrite, MemoryPolicyError>> {
        Box::pin(async move {
            let mut write = self.inner.prepare(ctx, intent, provenance, abort).await?;
            let payload: TutorMemoryWritePayload = serde_json::from_str(&write.content)
                .map_err(|_| MemoryPolicyError::Rejected(MemoryPolicyRejection::Other))?;
            let text = clean_required(payload.text, MAX_MEMORY_TEXT_CHARS)?;
            let next_action = clean_optional(payload.next_action, MAX_NEXT_ACTION_CHARS)?;
            let source_message_id =
                clean_optional(payload.source_message_id, MAX_SOURCE_MESSAGE_ID_CHARS)?;
            write.content = text;
            if let Some(next_action) = next_action {
                write
                    .metadata
                    .insert("next_action".into(), json!(next_action));
            }
            if let Some(source_message_id) = source_message_id {
                write
                    .metadata
                    .insert("source_message_id".into(), json!(source_message_id));
            }
            Ok(write)
        })
    }
}

pub struct TutorMemoryMutationGate {
    tutor_id: String,
}

impl TutorMemoryMutationGate {
    pub fn new(tutor_id: impl Into<String>) -> Self {
        Self {
            tutor_id: tutor_id.into(),
        }
    }
}

impl MemoryMutationGate for TutorMemoryMutationGate {
    fn authorize<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        _request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>> {
        Box::pin(async move {
            authorize_mutation(ctx, &self.tutor_id, &abort).map_err(|error| match error {
                KnowledgeError::Aborted => MemoryMutationGateError::Aborted,
                KnowledgeError::Unauthorized => MemoryMutationGateError::Denied,
                _ => MemoryMutationGateError::Unavailable,
            })
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TutorMemoryServiceBuildError {
    #[error(transparent)]
    Policy(#[from] SecureMemoryWritePolicyBuildError),
    #[error(transparent)]
    Service(#[from] MemoryServiceBuildError),
}

pub fn assemble_tutor_memory_service(
    source: Arc<TutorMemoryKnowledgeSource>,
    store: Arc<TutorMemoryStore>,
    tutor_id: impl Into<String>,
    access_control: Arc<KnowledgeAccessControl>,
    policy_secret: Vec<u8>,
) -> Result<MemoryService, TutorMemoryServiceBuildError> {
    let tutor_id = tutor_id.into();
    Ok(MemoryService::new(
        access_control,
        source,
        Arc::new(TutorMemoryWriteStore::new(store, tutor_id.clone())),
        Arc::new(TutorMemoryWritePolicy::new(policy_secret)?),
        Arc::new(TutorMemoryMutationGate::new(tutor_id)),
    )?)
}

fn authorize_mutation(
    ctx: KnowledgeRequestContext<'_>,
    tutor_id: &str,
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
            .is_some_and(|bound_tutor_id| bound_tutor_id == tutor_id)
        && attributes
            .get(TUTOR_MEMORY_MODE_ATTRIBUTE)
            .is_some_and(|mode| mode == "autonomous");
    if allowed {
        Ok(())
    } else {
        Err(KnowledgeError::Unauthorized)
    }
}

fn optional_metadata_string(
    metadata: &BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Result<Option<String>, KnowledgeError> {
    match metadata.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(str::to_string)
            .map(Some)
            .ok_or(KnowledgeError::Unauthorized),
    }
}

fn clean_required(value: String, max_chars: usize) -> Result<String, MemoryPolicyError> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(MemoryPolicyError::Rejected(
            MemoryPolicyRejection::EmptyContent,
        ));
    }
    if value.chars().count() > max_chars {
        return Err(MemoryPolicyError::Rejected(
            MemoryPolicyRejection::ContentTooLarge,
        ));
    }
    Ok(value)
}

fn clean_optional(
    value: Option<String>,
    max_chars: usize,
) -> Result<Option<String>, MemoryPolicyError> {
    value
        .map(|value| clean_required(value, max_chars))
        .transpose()
}

fn parse_kind(value: &str) -> Option<TutorMemoryKind> {
    match value {
        "commitment" => Some(TutorMemoryKind::Commitment),
        "open_loop" => Some(TutorMemoryKind::OpenLoop),
        "lesson_plan" => Some(TutorMemoryKind::LessonPlan),
        "reflection" => Some(TutorMemoryKind::Reflection),
        "strategy" => Some(TutorMemoryKind::Strategy),
        _ => None,
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

fn backend_error(error: impl std::fmt::Display) -> KnowledgeError {
    KnowledgeError::Backend(error.to_string())
}

#[cfg(test)]
mod tests {
    use llm_harness_runtime_knowledge::{
        AuthorizationDecision, KnowledgeAccessContext, KnowledgeAction, KnowledgeAuthorizer,
        KnowledgeResourceRef, KnowledgeScope, PrincipalRef,
    };
    use llm_harness_runtime_memory::{MemoryMutationOrigin, MemorySessionId};
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;

    struct AllowTutorMemory;

    impl KnowledgeAuthorizer for AllowTutorMemory {
        fn authorize<'a>(
            &'a self,
            _access: &'a KnowledgeAccessContext,
            _action: KnowledgeAction,
            _resource: KnowledgeResourceRef<'a>,
        ) -> BoxFuture<'a, Result<AuthorizationDecision, KnowledgeError>> {
            Box::pin(async { Ok(AuthorizationDecision::Allow) })
        }
    }

    fn access(tutor_id: &str, mode: &str) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(tutor_rag::AGENT_KNOWLEDGE_NAMESPACE);
        scope
            .attributes
            .insert(TUTOR_MEMORY_TUTOR_ID_ATTRIBUTE.into(), tutor_id.into());
        scope
            .attributes
            .insert(TUTOR_MEMORY_MODE_ATTRIBUTE.into(), mode.into());
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "user"))
    }

    #[tokio::test]
    async fn runtime_service_is_idempotent_and_rejects_stale_delete() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(TutorMemoryStore::new_with_root(dir.path()));
        let source = Arc::new(TutorMemoryKnowledgeSource::new(store.clone(), "tutor-a"));
        let service = assemble_tutor_memory_service(
            source,
            store.clone(),
            "tutor-a",
            Arc::new(KnowledgeAccessControl::new(Arc::new(AllowTutorMemory))),
            vec![7; 32],
        )
        .unwrap();
        let access_ctx = access("tutor-a", "autonomous");
        let run = RunContext::new(
            RunRequest::from_text("remember")
                .with_extension(access_ctx.clone())
                .with_extension(MemorySessionId::new("session-a").unwrap()),
        );
        let ctx = KnowledgeRequestContext {
            run: &run,
            access: &access_ctx,
        };
        let intent = tutor_memory_write_intent(
            TutorMemoryKind::OpenLoop,
            "Continue the attention exercise".into(),
            Some("Review question 3".into()),
            None,
        )
        .unwrap();
        let origin = MemoryMutationOrigin::Application {
            operation_id: "op-1".into(),
        };
        let first = service
            .write(
                ctx,
                intent.clone(),
                origin.clone(),
                CancellationToken::new(),
            )
            .await
            .unwrap();
        let second = service
            .write(ctx, intent, origin, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(first.reference, second.reference);
        assert_eq!(store.list("tutor-a", true).unwrap().len(), 1);

        let entry_id = first.reference.item_id.strip_prefix(ITEM_PREFIX).unwrap();
        store
            .resolve("tutor-a", entry_id, Some("Done".into()))
            .unwrap();
        let stale = service
            .delete(
                ctx,
                first.reference,
                MemoryMutationOrigin::Application {
                    operation_id: "op-2".into(),
                },
                CancellationToken::new(),
            )
            .await
            .unwrap_err();
        assert!(matches!(
            stale,
            llm_harness_runtime_memory::MemoryServiceError::Knowledge(
                KnowledgeError::StaleReference { .. }
            )
        ));

        let read_only = access("tutor-a", "read_only");
        let denied_run = RunContext::new(RunRequest::from_text("remember"));
        let denied_ctx = KnowledgeRequestContext {
            run: &denied_run,
            access: &read_only,
        };
        assert!(
            service
                .write(
                    denied_ctx,
                    tutor_memory_write_intent(
                        TutorMemoryKind::Strategy,
                        "Use diagrams".into(),
                        None,
                        None,
                    )
                    .unwrap(),
                    MemoryMutationOrigin::Application {
                        operation_id: "op-3".into(),
                    },
                    CancellationToken::new(),
                )
                .await
                .is_err()
        );
    }
}
