use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::{
    AuthorizationDecision, EvidenceAuthority, KnowledgeAccessContext, KnowledgeAccessControl,
    KnowledgeAction, KnowledgeAuthorizer, KnowledgeError, KnowledgeResourceRef,
};

use crate::learner_memory_source::{
    LEARNER_MEMORY_KINDS_ATTRIBUTE, LEARNER_MEMORY_LAYERS_ATTRIBUTE, LEARNER_MEMORY_NAMESPACE,
    LEARNER_MEMORY_PRINCIPAL_ATTRIBUTE, LEARNER_MEMORY_PROFILE_ATTRIBUTE, LEARNER_MEMORY_SOURCE_ID,
    LEARNER_MEMORY_SURFACES_ATTRIBUTE,
};

pub(crate) fn course_evidence_authority() -> Arc<EvidenceAuthority> {
    let mut secret = Vec::with_capacity(32);
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    Arc::new(
        EvidenceAuthority::new(secret, [tutor_agent::course_evidence_provider_id()])
            .expect("generated evidence secret and registered provider are valid"),
    )
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
            &["chat", "quiz", "notebook", "knowledge"],
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
    use llm_harness_runtime_knowledge::{KnowledgeRef, KnowledgeScope, PrincipalRef};

    use super::*;

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
            "chat,quiz,notebook,knowledge".into(),
        );
        scope.attributes.insert(
            LEARNER_MEMORY_KINDS_ATTRIBUTE.into(),
            "recent,profile,scope,preferences,teaching_strategy".into(),
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
}
