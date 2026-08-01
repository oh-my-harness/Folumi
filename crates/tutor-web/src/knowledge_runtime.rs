use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_agent::Plugin;
use llm_harness_runtime_knowledge::{
    AuthorizationDecision, EvidenceAuthority, KnowledgeAccessContext, KnowledgeAccessControl,
    KnowledgeAction, KnowledgeAuthorizer, KnowledgeCitationPolicy, KnowledgeCitationRequirement,
    KnowledgeError, KnowledgeResourceRef, KnowledgeSource,
};
use llm_harness_runtime_memory::MemoryPlugin;

use crate::learner_memory_source::{
    LEARNER_MEMORY_KINDS_ATTRIBUTE, LEARNER_MEMORY_LAYERS_ATTRIBUTE, LEARNER_MEMORY_NAMESPACE,
    LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE, LEARNER_MEMORY_PROFILE_ATTRIBUTE, LEARNER_MEMORY_SOURCE_ID,
    LEARNER_MEMORY_SURFACES_ATTRIBUTE, LearnerMemoryKnowledgeSource,
};
use crate::learner_memory_write::{LearnerMemoryApprover, assemble_learner_memory_service};
use crate::memory_store::FileMemoryBackend;

#[derive(Clone)]
pub struct AgentRuntimeSecurity {
    evidence_authority: Arc<EvidenceAuthority>,
    memory_policy_secret: Arc<[u8]>,
}

impl AgentRuntimeSecurity {
    pub fn generate() -> Self {
        let evidence_secret = random_process_secret();
        let memory_policy_secret = Arc::<[u8]>::from(random_process_secret());
        let evidence_authority = Arc::new(
            EvidenceAuthority::new(
                evidence_secret,
                [tutor_agent::agent_knowledge_evidence_provider_id()],
            )
            .expect("generated evidence secret and registered provider are valid"),
        );
        Self {
            evidence_authority,
            memory_policy_secret,
        }
    }

    pub fn evidence_authority(&self) -> Arc<EvidenceAuthority> {
        self.evidence_authority.clone()
    }

    pub(crate) fn memory_policy_secret(&self) -> Vec<u8> {
        self.memory_policy_secret.as_ref().to_vec()
    }
}

fn random_process_secret() -> Vec<u8> {
    let mut secret = Vec::with_capacity(32);
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret
}

pub(crate) fn agent_knowledge_citation_policy(
    course_source: bool,
    learner_memory_source: bool,
) -> tutor_agent::Result<KnowledgeCitationPolicy> {
    let mut builder = KnowledgeCitationPolicy::builder();
    if course_source {
        builder = builder.source(
            tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            KnowledgeCitationRequirement::Required,
        );
    }
    if learner_memory_source {
        builder = builder.source(
            LEARNER_MEMORY_SOURCE_ID,
            KnowledgeCitationRequirement::Optional,
        );
    }
    builder
        .build()
        .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))
}

pub(crate) struct LearnerMemoryRuntimeInput {
    pub(crate) backend: Arc<FileMemoryBackend>,
    pub(crate) semantic_rag: Option<tutor_rag::LanceDbRag>,
    pub(crate) mode: tutor_agent::LearnerMemoryMode,
    pub(crate) approver: Option<Arc<dyn LearnerMemoryApprover>>,
}

pub(crate) fn install_agent_knowledge_and_memory(
    mut router: tutor_agent::CapabilityRouter,
    course_source: Option<Arc<dyn KnowledgeSource>>,
    learner_memory: Option<LearnerMemoryRuntimeInput>,
    security: &AgentRuntimeSecurity,
) -> tutor_agent::Result<tutor_agent::CapabilityRouter> {
    let access_control = agent_knowledge_access_control();
    let has_course_source = course_source.is_some();
    let mut sources = Vec::new();
    if let Some(source) = course_source {
        sources.push(source);
    }

    let mut memory_source = None;
    if let Some(input) = learner_memory {
        if input.mode == tutor_agent::LearnerMemoryMode::Disabled {
            return Err(tutor_agent::TutorError::Internal(
                "disabled Learner Memory must not provide a backend".into(),
            ));
        }
        let source = Arc::new(match input.semantic_rag.clone() {
            Some(rag) => {
                LearnerMemoryKnowledgeSource::with_semantic_rag(input.backend.clone(), rag)
            }
            None => LearnerMemoryKnowledgeSource::new(input.backend.clone()),
        });
        sources.push(source.clone());
        memory_source = Some((source, input));
    }

    if !sources.is_empty() {
        let citation_policy =
            agent_knowledge_citation_policy(has_course_source, memory_source.is_some())?;
        let runtime = tutor_agent::assemble_knowledge_runtime(
            sources,
            access_control.clone(),
            security.evidence_authority(),
            tutor_agent::agent_knowledge_evidence_provider_id(),
            citation_policy,
        )?;
        router = router.with_knowledge_runtime(runtime);
    }

    match memory_source {
        None => router.with_learner_memory_runtime(tutor_agent::LearnerMemoryMode::Disabled, None),
        Some((_source, input)) if input.mode == tutor_agent::LearnerMemoryMode::ReadOnly => {
            if input.approver.is_some() {
                return Err(tutor_agent::TutorError::Internal(
                    "read-only Learner Memory must not install an approver".into(),
                ));
            }
            router.with_learner_memory_runtime(tutor_agent::LearnerMemoryMode::ReadOnly, None)
        }
        Some((source, input)) => {
            let approver = input.approver.ok_or_else(|| {
                tutor_agent::TutorError::Internal(
                    "interactive Learner Memory requires an approval coordinator".into(),
                )
            })?;
            let service = assemble_learner_memory_service(
                source,
                input.backend,
                access_control,
                security.memory_policy_secret(),
                approver,
            )
            .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))?;
            let plugin: Arc<dyn Plugin> = Arc::new(MemoryPlugin::new(Arc::new(service)));
            router.with_learner_memory_runtime(
                tutor_agent::LearnerMemoryMode::InteractiveMutation,
                Some(plugin),
            )
        }
    }
}

pub(crate) fn agent_knowledge_access_control() -> Arc<KnowledgeAccessControl> {
    Arc::new(KnowledgeAccessControl::new(Arc::new(
        AgentKnowledgeAuthorizer,
    )))
}

struct AgentKnowledgeAuthorizer;

impl KnowledgeAuthorizer for AgentKnowledgeAuthorizer {
    fn authorize<'a>(
        &'a self,
        access: &'a KnowledgeAccessContext,
        action: KnowledgeAction,
        resource: KnowledgeResourceRef<'a>,
    ) -> BoxFuture<'a, Result<AuthorizationDecision, KnowledgeError>> {
        Box::pin(async move {
            if access.scope.namespace != tutor_rag::AGENT_KNOWLEDGE_NAMESPACE
                || access.principal.subject.trim().is_empty()
            {
                return Ok(AuthorizationDecision::Deny);
            }

            let source_id = match resource {
                KnowledgeResourceRef::Source { source_id, .. } => source_id,
                KnowledgeResourceRef::Item(reference) => &reference.source_id,
            };
            let allowed = match source_id {
                tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID => {
                    matches!(
                        action,
                        KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read
                    ) && access
                        .scope
                        .attributes
                        .get(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE)
                        .is_some_and(|kb| !kb.trim().is_empty())
                }
                LEARNER_MEMORY_SOURCE_ID => memory_access_allows(access, action),
                _ => false,
            };
            Ok(if allowed {
                AuthorizationDecision::Allow
            } else {
                AuthorizationDecision::Deny
            })
        })
    }
}

fn memory_access_allows(access: &KnowledgeAccessContext, action: KnowledgeAction) -> bool {
    let attributes = &access.scope.attributes;
    let principal_is_bound = attributes
        .get(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE)
        .is_some_and(|subject| subject == &access.principal.subject);
    let profile = attributes
        .get(LEARNER_MEMORY_PROFILE_ATTRIBUTE)
        .map(String::as_str);
    let scope_is_valid = access.scope.namespace == LEARNER_MEMORY_NAMESPACE
        && principal_is_bound
        && csv_is_subset(
            attributes.get(LEARNER_MEMORY_LAYERS_ATTRIBUTE),
            &["l1", "l2", "l3"],
            false,
        )
        && csv_is_subset(
            attributes.get(LEARNER_MEMORY_SURFACES_ATTRIBUTE),
            &["chat", "notebook", "knowledge"],
            true,
        )
        && csv_is_subset(
            attributes.get(LEARNER_MEMORY_KINDS_ATTRIBUTE),
            &[
                "recent",
                "profile",
                "scope",
                "preferences",
                "teaching_strategy",
                "continuity",
            ],
            false,
        );
    if !scope_is_valid {
        return false;
    }
    match action {
        KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read => {
            matches!(profile, Some("read_only" | "interactive_mutation"))
        }
        KnowledgeAction::Write | KnowledgeAction::Delete => profile == Some("interactive_mutation"),
    }
}

fn csv_is_subset(value: Option<&String>, allowed: &[&str], allow_empty: bool) -> bool {
    value.is_some_and(|value| {
        let values = value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>();
        (allow_empty || !values.is_empty()) && values.iter().all(|item| allowed.contains(item))
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use llm_harness_agent::{
        JsonlSessionRepo, Session, SessionRepo, session::CreateSessionOptions,
    };
    use llm_harness_loop::test_utils::{MockLlmClient, MockResponse, NoOpEnv};
    use llm_harness_runtime_knowledge::{KnowledgeRef, KnowledgeScope, PrincipalRef};
    use llm_harness_runtime_memory::MemorySessionId;
    use llm_harness_types::RunRequest;
    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::memory_approval::{ApprovalResponseOutcome, WebMemoryApprovalCoordinator};
    use crate::memory_store::{DurableMemoryWrite, memory_entry_revision};
    use crate::stream::{StreamEvent, TutorStream};
    use tutor_agent::governance::GovernanceConfig;
    use tutor_agent::{Capability, CapabilityRouter, LearnerMemoryMode, LlmConfig};

    fn access(profile: &str, principal: &str) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(tutor_rag::AGENT_KNOWLEDGE_NAMESPACE);
        scope.attributes.insert(
            tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(),
            "kb-a".into(),
        );
        scope
            .attributes
            .insert(LEARNER_MEMORY_PROFILE_ATTRIBUTE.into(), profile.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE.into(), principal.into());
        scope
            .attributes
            .insert(LEARNER_MEMORY_LAYERS_ATTRIBUTE.into(), "l1,l2,l3".into());
        scope.attributes.insert(
            LEARNER_MEMORY_SURFACES_ATTRIBUTE.into(),
            "chat,notebook,knowledge".into(),
        );
        scope.attributes.insert(
            LEARNER_MEMORY_KINDS_ATTRIBUTE.into(),
            "recent,profile,scope,preferences,teaching_strategy,continuity".into(),
        );
        KnowledgeAccessContext::new(scope, PrincipalRef::new(principal, "user"))
    }

    async fn decision(
        access: &KnowledgeAccessContext,
        action: KnowledgeAction,
        source_id: &str,
    ) -> AuthorizationDecision {
        AgentKnowledgeAuthorizer
            .authorize(
                access,
                action,
                KnowledgeResourceRef::Item(&KnowledgeRef {
                    source_id: source_id.into(),
                    item_id: "item".into(),
                    revision: Some("revision".into()),
                }),
            )
            .await
            .unwrap()
    }

    fn router(responses: Vec<MockResponse>) -> CapabilityRouter {
        CapabilityRouter::new(
            Arc::new(NoOpEnv),
            LlmConfig::anthropic("mock-model", ""),
            GovernanceConfig::new(2.0, None, false),
        )
        .with_client(Arc::new(MockLlmClient::new(responses)))
    }

    fn run_request(profile: &str, session_id: &str, text: &str) -> RunRequest {
        RunRequest::from_text(text)
            .with_extension(access(profile, "local-user"))
            .with_extension(MemorySessionId::new(session_id).unwrap())
    }

    #[tokio::test]
    async fn source_aware_authorizer_keeps_course_read_only_and_memory_profiled() {
        let read_only = access("read_only", "local-user");
        assert_eq!(
            decision(
                &read_only,
                KnowledgeAction::Read,
                tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            )
            .await,
            AuthorizationDecision::Allow
        );
        assert_eq!(
            decision(
                &read_only,
                KnowledgeAction::Write,
                tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            )
            .await,
            AuthorizationDecision::Deny
        );
        assert_eq!(
            decision(&read_only, KnowledgeAction::Read, LEARNER_MEMORY_SOURCE_ID,).await,
            AuthorizationDecision::Allow
        );
        assert_eq!(
            decision(&read_only, KnowledgeAction::Write, LEARNER_MEMORY_SOURCE_ID,).await,
            AuthorizationDecision::Deny
        );
        let interactive = access("interactive_mutation", "local-user");
        assert_eq!(
            decision(
                &interactive,
                KnowledgeAction::Delete,
                LEARNER_MEMORY_SOURCE_ID,
            )
            .await,
            AuthorizationDecision::Allow
        );
    }

    #[tokio::test]
    async fn source_aware_authorizer_rejects_cross_principal_and_unknown_sources() {
        let mut forged = access("interactive_mutation", "local-user");
        forged.principal.subject = "other-user".into();
        assert_eq!(
            decision(&forged, KnowledgeAction::Read, LEARNER_MEMORY_SOURCE_ID).await,
            AuthorizationDecision::Deny
        );
        assert_eq!(
            decision(
                &access("read_only", "local-user"),
                KnowledgeAction::Read,
                "forged"
            )
            .await,
            AuthorizationDecision::Deny
        );
    }

    #[test]
    fn interactive_assembly_requires_a_live_approval_coordinator() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let error = install_agent_knowledge_and_memory(
            router(vec![]),
            None,
            Some(LearnerMemoryRuntimeInput {
                backend,
                semantic_rag: None,
                mode: LearnerMemoryMode::InteractiveMutation,
                approver: None,
            }),
            &AgentRuntimeSecurity::generate(),
        )
        .err()
        .expect("interactive assembly must fail closed");

        assert!(
            error
                .to_string()
                .contains("requires an approval coordinator")
        );
    }

    #[tokio::test]
    async fn interactive_chat_calls_web_gate_before_persisting_memory() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let stream = TutorStream::new(8);
        let mut events = stream.subscribe();
        let coordinator = Arc::new(WebMemoryApprovalCoordinator::new(
            stream,
            "session-a",
            "run-a",
            CancellationToken::new(),
        ));
        let router = install_agent_knowledge_and_memory(
            router(vec![
                MockResponse::tool_use(
                    "memory-write-1",
                    "memory_write",
                    r#"{"content":"Prefers diagrams.","kind":"preference"}"#,
                ),
                MockResponse::text("I will remember that preference."),
            ]),
            None,
            Some(LearnerMemoryRuntimeInput {
                backend: backend.clone(),
                semantic_rag: None,
                mode: LearnerMemoryMode::InteractiveMutation,
                approver: Some(coordinator.clone()),
            }),
            &AgentRuntimeSecurity::generate(),
        )
        .unwrap();

        let task = tokio::spawn(async move {
            router
                .run_request(
                    Capability::Chat,
                    run_request(
                        "interactive_mutation",
                        "session-a",
                        "Remember that I prefer diagrams.",
                    ),
                )
                .await
        });
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .expect("memory write must request approval")
            .unwrap();
        let StreamEvent::Status { kind, data } = event else {
            panic!("expected approval status");
        };
        assert_eq!(kind, "approval_request");
        assert_eq!(data["tool"], "memory_write");
        assert_eq!(
            backend.read("L3/preferences.md").unwrap().markdown,
            "# User preferences\n\n"
        );
        let request_id = data["request_id"].as_str().unwrap();
        assert_eq!(
            coordinator.resolve(request_id, true),
            ApprovalResponseOutcome::Resolved
        );

        let answer = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("approved Chat run must finish")
            .unwrap()
            .unwrap();
        assert!(answer.contains("remember"));
        assert!(
            backend
                .read("L3/preferences.md")
                .unwrap()
                .markdown
                .contains("Prefers diagrams.")
        );
        coordinator.close();
    }

    #[tokio::test]
    async fn interactive_chat_forget_uses_exact_ref_and_waits_for_web_approval() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let entry = backend
            .upsert_durable_preference(DurableMemoryWrite {
                content: "Prefers diagrams.".into(),
                kind: "preference".into(),
                provenance: serde_json::json!({"source": "test"}),
                idempotency_key: "forget-test".into(),
                expires_at: None,
            })
            .unwrap();
        let reference = KnowledgeRef {
            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
            item_id: format!("l3/preferences/{}", entry.marker),
            revision: Some(memory_entry_revision(&entry).unwrap()),
        };
        let stream = TutorStream::new(8);
        let mut events = stream.subscribe();
        let coordinator = Arc::new(WebMemoryApprovalCoordinator::new(
            stream,
            "session-a",
            "run-forget",
            CancellationToken::new(),
        ));
        let router = install_agent_knowledge_and_memory(
            router(vec![
                MockResponse::tool_use(
                    "memory-forget-1",
                    "memory_forget",
                    &serde_json::json!({"reference": reference}).to_string(),
                ),
                MockResponse::text("I forgot that preference."),
            ]),
            None,
            Some(LearnerMemoryRuntimeInput {
                backend: backend.clone(),
                semantic_rag: None,
                mode: LearnerMemoryMode::InteractiveMutation,
                approver: Some(coordinator.clone()),
            }),
            &AgentRuntimeSecurity::generate(),
        )
        .unwrap();

        let task = tokio::spawn(async move {
            router
                .run_request(
                    Capability::Chat,
                    run_request(
                        "interactive_mutation",
                        "session-a",
                        "Forget my diagram preference.",
                    ),
                )
                .await
        });
        let StreamEvent::Status { kind, data } =
            tokio::time::timeout(Duration::from_secs(5), events.recv())
                .await
                .expect("memory forget must request approval")
                .unwrap()
        else {
            panic!("expected approval status");
        };
        assert_eq!(kind, "approval_request");
        assert_eq!(data["tool"], "memory_forget");
        assert!(
            backend
                .read("L3/preferences.md")
                .unwrap()
                .markdown
                .contains("Prefers diagrams.")
        );
        assert_eq!(
            coordinator.resolve(data["request_id"].as_str().unwrap(), true),
            ApprovalResponseOutcome::Resolved
        );

        let answer = tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("approved forget must finish")
            .unwrap()
            .unwrap();
        assert!(answer.contains("forgot"));
        assert!(
            !backend
                .read("L3/preferences.md")
                .unwrap()
                .markdown
                .contains("Prefers diagrams.")
        );
        coordinator.close();
    }

    #[tokio::test]
    async fn read_only_chat_uses_memory_without_forcing_a_visible_citation() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let entry = backend
            .upsert_durable_preference(DurableMemoryWrite {
                content: "Prefers diagrams.".into(),
                kind: "preference".into(),
                provenance: serde_json::json!({"source": "test"}),
                idempotency_key: "test-preference".into(),
                expires_at: None,
            })
            .unwrap();
        let reference = KnowledgeRef {
            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
            item_id: format!("l3/preferences/{}", entry.marker),
            revision: Some(memory_entry_revision(&entry).unwrap()),
        };
        let read_args = serde_json::json!({
            "reference": reference,
            "selector": {"kind": "document"}
        })
        .to_string();
        let router = install_agent_knowledge_and_memory(
            router(vec![
                MockResponse::tool_use(
                    "memory-search-1",
                    "knowledge_search",
                    r#"{"query":"diagrams","source_id":"llm-tutor.learner-memory"}"#,
                ),
                MockResponse::tool_use("memory-read-1", "knowledge_read", &read_args),
                MockResponse::text("I will explain this with diagrams."),
            ]),
            None,
            Some(LearnerMemoryRuntimeInput {
                backend,
                semantic_rag: None,
                mode: LearnerMemoryMode::ReadOnly,
                approver: None,
            }),
            &AgentRuntimeSecurity::generate(),
        )
        .unwrap();

        let answer = router
            .run_request(
                Capability::Chat,
                run_request(
                    "read_only",
                    "session-a",
                    "Explain this in the way I prefer.",
                ),
            )
            .await
            .unwrap();

        assert_eq!(answer, "I will explain this with diagrams.");
        assert!(!answer.contains("[^"));
    }

    #[tokio::test]
    async fn memory_read_persists_a_receipt_but_not_body_or_trusted_context() {
        let temp = tempfile::tempdir().unwrap();
        let backend = Arc::new(FileMemoryBackend::new_with_root(temp.path().join("memory")));
        let private_tail = "MEMORY_PRIVATE_READ_TAIL_MUST_NOT_PERSIST";
        let private_body =
            format!("visual map preference {}", "context ".repeat(80)) + private_tail;
        let private_idempotency = "PRIVATE_IDEMPOTENCY_SENTINEL";
        let entry = backend
            .upsert_durable_preference(DurableMemoryWrite {
                content: private_body,
                kind: "preference".into(),
                provenance: serde_json::json!({"source": "test"}),
                idempotency_key: private_idempotency.into(),
                expires_at: None,
            })
            .unwrap();
        let reference = KnowledgeRef {
            source_id: LEARNER_MEMORY_SOURCE_ID.into(),
            item_id: format!("l3/preferences/{}", entry.marker),
            revision: Some(memory_entry_revision(&entry).unwrap()),
        };
        let read_args = serde_json::json!({
            "reference": reference,
            "selector": {"kind": "document"}
        })
        .to_string();
        let router = install_agent_knowledge_and_memory(
            router(vec![
                MockResponse::tool_use(
                    "memory-search",
                    "knowledge_search",
                    r#"{"query":"visual maps","source_id":"llm-tutor.learner-memory"}"#,
                ),
                MockResponse::tool_use("memory-read", "knowledge_read", &read_args),
                MockResponse::text("I will use a visual map."),
            ]),
            None,
            Some(LearnerMemoryRuntimeInput {
                backend,
                semantic_rag: None,
                mode: LearnerMemoryMode::ReadOnly,
                approver: None,
            }),
            &AgentRuntimeSecurity::generate(),
        )
        .unwrap();
        let sessions_root = temp.path().join("sessions");
        let repo = JsonlSessionRepo::new(&sessions_root);
        let storage = repo.create(CreateSessionOptions::default()).await.unwrap();
        let session = Session::new(storage);

        let answer = router
            .run_request_with_session_cancel(
                Capability::Chat,
                session,
                run_request("read_only", "session-private", "Use my preference."),
                None,
            )
            .await
            .unwrap();
        assert_eq!(answer, "I will use a visual map.");

        let persisted = std::fs::read_dir(&sessions_root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .flat_map(|entry| {
                std::fs::read_dir(entry.path())
                    .into_iter()
                    .flatten()
                    .filter_map(Result::ok)
            })
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .collect::<String>();
        assert!(
            persisted.contains("knowledge_read"),
            "the bounded read receipt should remain durable"
        );
        assert!(!persisted.contains(private_tail));
        assert!(!persisted.contains(private_idempotency));
        assert!(!persisted.contains(LEARNER_MEMORY_PROFILE_ATTRIBUTE));
        assert!(!persisted.contains(LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE));
        println!(
            "{}",
            serde_json::json!({
                "durable_session_bytes": persisted.len(),
                "memory_read_receipt_persisted": true,
                "memory_body_persisted": false,
                "trusted_context_persisted": false,
            })
        );
    }
}
