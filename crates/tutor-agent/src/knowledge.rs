use std::sync::Arc;

use futures::future::BoxFuture;
use llm_harness_agent::Plugin;
use llm_harness_runtime_knowledge::{
    AuthorizationDecision, EvidenceAuthority, EvidenceProviderId, KnowledgeAccessContext,
    KnowledgeAccessControl, KnowledgeAction, KnowledgeAuthorizer, KnowledgeCitationPolicy,
    KnowledgePlugin, KnowledgePluginConfig, KnowledgeRegistry, KnowledgeResourceRef,
    KnowledgeToolConfig,
};
use tutor_rag::LanceDbKnowledgeSource;

use crate::error::{Result, TutorError};

pub const COURSE_EVIDENCE_PROVIDER_ID: &str = "llm-tutor.course-knowledge";

#[derive(Clone)]
pub struct KnowledgeRuntime {
    registry: Arc<KnowledgeRegistry>,
    authority: Arc<EvidenceAuthority>,
    provider_id: EvidenceProviderId,
    tool_config: KnowledgeToolConfig,
}

impl KnowledgeRuntime {
    pub fn plugin(&self) -> Arc<dyn Plugin> {
        Arc::new(self.build_plugin(KnowledgeCitationPolicy::RequireWhenEvidenceRead))
    }

    pub fn boxed_plugin(&self, citation_policy: KnowledgeCitationPolicy) -> Box<dyn Plugin> {
        Box::new(self.build_plugin(citation_policy))
    }

    fn build_plugin(&self, citation_policy: KnowledgeCitationPolicy) -> KnowledgePlugin {
        KnowledgePlugin::new(
            self.registry.clone(),
            self.authority.clone(),
            self.provider_id.clone(),
            KnowledgePluginConfig {
                tools: self.tool_config.clone(),
                citation_policy,
            },
        )
        .expect("course knowledge plugin configuration was validated during assembly")
    }
}

pub fn course_evidence_provider_id() -> EvidenceProviderId {
    EvidenceProviderId(COURSE_EVIDENCE_PROVIDER_ID.into())
}

pub fn assemble_course_knowledge(
    source: LanceDbKnowledgeSource,
    authority: Arc<EvidenceAuthority>,
) -> Result<KnowledgeRuntime> {
    let access_control = Arc::new(KnowledgeAccessControl::new(Arc::new(
        CourseKnowledgeAuthorizer,
    )));
    let registry = KnowledgeRegistry::builder(access_control)
        .source(Arc::new(source))
        .build()
        .map_err(|error| TutorError::Internal(error.to_string()))?;
    let registry = Arc::new(registry);
    let provider_id = course_evidence_provider_id();
    let tool_config = KnowledgeToolConfig::default();
    KnowledgePlugin::new(
        registry.clone(),
        authority.clone(),
        provider_id.clone(),
        KnowledgePluginConfig {
            tools: tool_config.clone(),
            citation_policy: KnowledgeCitationPolicy::RequireWhenEvidenceRead,
        },
    )
    .map_err(|error| TutorError::Internal(error.to_string()))?;
    Ok(KnowledgeRuntime {
        registry,
        authority,
        provider_id,
        tool_config,
    })
}

struct CourseKnowledgeAuthorizer;

impl KnowledgeAuthorizer for CourseKnowledgeAuthorizer {
    fn authorize<'a>(
        &'a self,
        access: &'a KnowledgeAccessContext,
        action: KnowledgeAction,
        resource: KnowledgeResourceRef<'a>,
    ) -> BoxFuture<
        'a,
        std::result::Result<AuthorizationDecision, llm_harness_runtime_knowledge::KnowledgeError>,
    > {
        Box::pin(async move {
            let scope_is_valid = access.scope.namespace == tutor_rag::COURSE_KNOWLEDGE_NAMESPACE
                && access
                    .scope
                    .attributes
                    .get(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE)
                    .is_some_and(|kb| !kb.trim().is_empty());
            let action_is_allowed = matches!(
                action,
                KnowledgeAction::Discover | KnowledgeAction::Search | KnowledgeAction::Read
            );
            let resource_is_allowed = match resource {
                KnowledgeResourceRef::Source { source_id, .. } => {
                    source_id == tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID
                }
                KnowledgeResourceRef::Item(reference) => {
                    reference.source_id == tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID
                }
            };
            Ok(
                if scope_is_valid && action_is_allowed && resource_is_allowed {
                    AuthorizationDecision::Allow
                } else {
                    AuthorizationDecision::Deny
                },
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use llm_harness_runtime_knowledge::{
        KNOWLEDGE_READ_TOOL_NAME, KNOWLEDGE_SEARCH_TOOL_NAME, KnowledgeScope, PrincipalRef,
    };
    use llm_harness_types::{
        AssistantMessage, AssistantMessageKind, RunContext, RunRequest, ToolContext, UnsupportedEnv,
    };
    use tokio::sync::mpsc;
    use tokio_util::sync::CancellationToken;
    use tutor_rag::{EmbeddingConfig, LanceDbRag};

    use super::*;

    fn access(namespace: &str, kb: Option<&str>) -> KnowledgeAccessContext {
        let mut scope = KnowledgeScope::new(namespace);
        if let Some(kb) = kb {
            scope
                .attributes
                .insert(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(), kb.into());
        }
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "test"))
    }

    fn tool_context(request: RunRequest) -> ToolContext {
        let (update_tx, _update_rx) = mpsc::channel(1);
        ToolContext {
            env: Arc::new(UnsupportedEnv::new()),
            run: Arc::new(RunContext::new(request)),
            abort: CancellationToken::new(),
            tool_use_id: "knowledge-test".into(),
            turn_index: 0,
            assistant_message: Arc::new(AssistantMessage {
                kind: AssistantMessageKind::Progress,
                message_id: "message-test".into(),
                turn_id: "turn-test".into(),
                content: vec![],
                usage: None,
                stop_reason: None,
                timestamp: Utc::now(),
                provider: None,
                api: None,
                model: None,
                error_message: None,
            }),
            update_tx,
        }
    }

    fn assembled_runtime(temp: &tempfile::TempDir) -> KnowledgeRuntime {
        let rag = LanceDbRag::new(
            temp.path(),
            EmbeddingConfig {
                provider: "hash".into(),
                model: "test".into(),
                api_key: String::new(),
                base_url: None,
                embeddings_path: None,
                dimensions: Some(32),
                send_dimensions: false,
            },
        );
        let provider_id = course_evidence_provider_id();
        let authority = Arc::new(EvidenceAuthority::new(vec![7; 32], [provider_id]).unwrap());
        assemble_course_knowledge(LanceDbKnowledgeSource::new(rag, "kb-a"), authority).unwrap()
    }

    #[tokio::test]
    async fn authorizer_only_allows_course_reads_in_a_trusted_scope() {
        let authorizer = CourseKnowledgeAuthorizer;
        let resource = KnowledgeResourceRef::Source {
            source_id: tutor_rag::COURSE_KNOWLEDGE_SOURCE_ID,
            domains: &[],
        };
        assert_eq!(
            authorizer
                .authorize(
                    &access(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE, Some("kb-a")),
                    KnowledgeAction::Search,
                    resource,
                )
                .await
                .unwrap(),
            AuthorizationDecision::Allow
        );
        assert_eq!(
            authorizer
                .authorize(
                    &access(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE, Some("kb-a")),
                    KnowledgeAction::Write,
                    resource,
                )
                .await
                .unwrap(),
            AuthorizationDecision::Deny
        );
        assert_eq!(
            authorizer
                .authorize(
                    &access("untrusted", Some("kb-a")),
                    KnowledgeAction::Search,
                    resource,
                )
                .await
                .unwrap(),
            AuthorizationDecision::Deny
        );
        assert_eq!(
            authorizer
                .authorize(
                    &access(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE, None),
                    KnowledgeAction::Search,
                    resource,
                )
                .await
                .unwrap(),
            AuthorizationDecision::Deny
        );
    }

    #[test]
    fn assembly_registers_runtime_owned_knowledge_tools() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = assembled_runtime(&temp);
        let mut tools = Vec::new();

        runtime.plugin().register_tools(&mut tools);

        let names = tools.iter().map(|tool| tool.name()).collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![KNOWLEDGE_SEARCH_TOOL_NAME, KNOWLEDGE_READ_TOOL_NAME]
        );
    }

    #[tokio::test]
    async fn runtime_search_fails_closed_without_trusted_access() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = assembled_runtime(&temp);
        let mut tools = Vec::new();
        runtime.plugin().register_tools(&mut tools);
        let search = tools
            .iter()
            .find(|tool| tool.name() == KNOWLEDGE_SEARCH_TOOL_NAME)
            .unwrap();

        let error = search
            .execute(
                serde_json::json!({"query": "refund"}),
                &tool_context(RunRequest::from_text("refund")),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code, "knowledge_unauthorized");
        assert_eq!(error.model_message, "knowledge access is unauthorized");
    }

    #[tokio::test]
    async fn runtime_search_rejects_a_forged_source_and_has_no_kb_argument() {
        let temp = tempfile::tempdir().unwrap();
        let runtime = assembled_runtime(&temp);
        let mut tools = Vec::new();
        runtime.plugin().register_tools(&mut tools);
        let search = tools
            .iter()
            .find(|tool| tool.name() == KNOWLEDGE_SEARCH_TOOL_NAME)
            .unwrap();
        let trusted_access = access(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE, Some("kb-a"));

        let error = search
            .execute(
                serde_json::json!({
                    "query": "refund",
                    "source_id": "forged-source"
                }),
                &tool_context(RunRequest::from_text("refund").with_extension(trusted_access)),
            )
            .await
            .unwrap_err();

        assert_eq!(error.code, "knowledge_not_found");
        assert!(
            search
                .parameters_schema()
                .pointer("/properties/kb")
                .is_none()
        );
    }
}
