use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    AuthorizationDecision, EvidenceAuthority, KnowledgeAccessContext, KnowledgeAccessControl,
    KnowledgeAction, KnowledgeAuthorizer, KnowledgeCitationPolicy, KnowledgeCitationRequirement,
    KnowledgeError, KnowledgeResourceRef, KnowledgeSource,
};

use crate::memory_approval::WebMemoryApprovalCoordinator;
use crate::memory_runtime::{
    SavedMemoryKnowledgeSource, USER_MEMORY_PRINCIPAL_ATTRIBUTE, USER_MEMORY_PROFILE_ATTRIBUTE,
    USER_MEMORY_SOURCE_ID, assemble_saved_memory_service,
};
use crate::memory_store::MemoryStore;

pub(crate) struct UserMemoryRuntimeInput {
    pub store: Arc<MemoryStore>,
    pub approver: Arc<WebMemoryApprovalCoordinator>,
}

#[derive(Clone)]
pub struct AgentRuntimeSecurity {
    evidence_authority: Arc<EvidenceAuthority>,
}

impl AgentRuntimeSecurity {
    pub fn generate() -> Self {
        let mut secret = Vec::with_capacity(32);
        secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
        Self {
            evidence_authority: Arc::new(
                EvidenceAuthority::new(
                    secret,
                    [tutor_agent::agent_knowledge_evidence_provider_id()],
                )
                .expect("generated evidence secret and registered provider are valid"),
            ),
        }
    }

    pub fn evidence_authority(&self) -> Arc<EvidenceAuthority> {
        self.evidence_authority.clone()
    }
}

pub(crate) fn install_agent_knowledge_and_memory(
    mut router: tutor_agent::CapabilityRouter,
    course_source: Option<Arc<dyn KnowledgeSource>>,
    user_memory: Option<UserMemoryRuntimeInput>,
    security: &AgentRuntimeSecurity,
) -> tutor_agent::Result<tutor_agent::CapabilityRouter> {
    if course_source.is_none() && user_memory.is_none() {
        return Ok(router);
    }
    let access_control = agent_knowledge_access_control();
    let mut sources = Vec::new();
    let mut citation_builder = KnowledgeCitationPolicy::builder();
    if let Some(source) = course_source {
        sources.push(source);
        citation_builder = citation_builder.source(
            tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            KnowledgeCitationRequirement::Required,
        );
    }
    if let Some(input) = user_memory {
        let source = Arc::new(SavedMemoryKnowledgeSource::new(input.store.clone()));
        let service = assemble_saved_memory_service(
            source.clone(),
            input.store,
            access_control.clone(),
            input.approver,
        )
        .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))?;
        sources.push(source as Arc<dyn KnowledgeSource>);
        citation_builder = citation_builder.source(
            USER_MEMORY_SOURCE_ID,
            KnowledgeCitationRequirement::Optional,
        );
        router = router.with_memory_service(Arc::new(service));
    }
    let citation_policy = citation_builder
        .build()
        .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))?;
    let runtime = tutor_agent::assemble_knowledge_runtime(
        sources,
        access_control,
        security.evidence_authority(),
        tutor_agent::agent_knowledge_evidence_provider_id(),
        citation_policy,
    )?;
    router = router.with_knowledge_runtime(runtime);
    Ok(router)
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
            let course_allowed = source_id == tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID
                && matches!(
                    action,
                    KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read
                )
                && access
                    .scope
                    .attributes
                    .get(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE)
                    .is_some_and(|kb| !kb.trim().is_empty());
            let memory_profile = access
                .scope
                .attributes
                .get(USER_MEMORY_PROFILE_ATTRIBUTE)
                .map(String::as_str);
            let memory_allowed = source_id == USER_MEMORY_SOURCE_ID
                && access
                    .scope
                    .attributes
                    .get(USER_MEMORY_PRINCIPAL_ATTRIBUTE)
                    .is_some_and(|subject| subject == &access.principal.subject)
                && match action {
                    KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read => {
                        matches!(memory_profile, Some("read_only" | "interactive_mutation"))
                    }
                    KnowledgeAction::Write | KnowledgeAction::Delete => {
                        memory_profile == Some("interactive_mutation")
                    }
                };
            Ok(if course_allowed || memory_allowed {
                AuthorizationDecision::Allow
            } else {
                AuthorizationDecision::Deny
            })
        })
    }
}
