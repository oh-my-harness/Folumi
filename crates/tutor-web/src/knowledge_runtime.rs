use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    AuthorizationDecision, EvidenceAuthority, KnowledgeAccessContext, KnowledgeAccessControl,
    KnowledgeAction, KnowledgeAuthorizer, KnowledgeCitationPolicy, KnowledgeCitationRequirement,
    KnowledgeError, KnowledgeResourceRef, KnowledgeSource,
};

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

pub(crate) fn install_agent_knowledge(
    mut router: tutor_agent::CapabilityRouter,
    course_source: Option<Arc<dyn KnowledgeSource>>,
    security: &AgentRuntimeSecurity,
) -> tutor_agent::Result<tutor_agent::CapabilityRouter> {
    let Some(source) = course_source else {
        return Ok(router);
    };
    let citation_policy = KnowledgeCitationPolicy::builder()
        .source(
            tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            KnowledgeCitationRequirement::Required,
        )
        .build()
        .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))?;
    let runtime = tutor_agent::assemble_knowledge_runtime(
        vec![source],
        agent_knowledge_access_control(),
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
            let allowed = source_id == tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID
                && matches!(
                    action,
                    KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read
                )
                && access
                    .scope
                    .attributes
                    .get(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE)
                    .is_some_and(|kb| !kb.trim().is_empty());
            Ok(if allowed {
                AuthorizationDecision::Allow
            } else {
                AuthorizationDecision::Deny
            })
        })
    }
}
