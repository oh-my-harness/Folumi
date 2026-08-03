use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use llm_adapter::provider::Provider;
use llm_harness_agent::Session;
use llm_harness_types::{AgentMessage, ExecutionEnv, RunRequest, Tool};
use tokio_util::sync::CancellationToken;

use crate::error::{Result, TutorError};
use crate::event_sink::SharedEventSink;
use crate::governance::GovernanceConfig;
use crate::knowledge::KnowledgeRuntime;
use crate::llm_provider::LlmConfig;
use tutor_tools::WebSearchConfig;

/// Supported Assistant interaction modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Conversational Q&A with RAG knowledge base.
    Chat,
    /// Execute user code with explanation.
    CodeExec,
    /// Research external/internal sources and synthesize a cited report.
    Research,
    /// Organize Notebook content through bounded direct tools and proposals.
    Organize,
}

impl FromStr for Capability {
    type Err = TutorError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "chat" => Ok(Self::Chat),
            "code_exec" => Ok(Self::CodeExec),
            "research" => Ok(Self::Research),
            "organize" => Ok(Self::Organize),
            other => Err(TutorError::UnsupportedCapability(other.into())),
        }
    }
}

/// Entry point for all capabilities.
#[derive(Clone)]
pub struct CapabilityRouter {
    pub env: Arc<dyn ExecutionEnv>,
    pub llm: LlmConfig,
    pub governance: GovernanceConfig,
    pub event_sink: Option<SharedEventSink>,
    pub knowledge_runtime: Option<KnowledgeRuntime>,
    pub web_search: Option<WebSearchConfig>,
    pub product_tools: Vec<Arc<dyn Tool>>,
    pub workflow_root: Option<PathBuf>,
    pub product_instruction: Option<String>,
    client: Option<Arc<dyn Provider>>,
}

impl CapabilityRouter {
    pub fn new(env: Arc<dyn ExecutionEnv>, llm: LlmConfig, governance: GovernanceConfig) -> Self {
        Self {
            env,
            llm,
            governance,
            event_sink: None,
            knowledge_runtime: None,
            web_search: None,
            product_tools: vec![],
            workflow_root: None,
            product_instruction: None,
            client: None,
        }
    }

    /// Inject a custom LLM client; skips `LlmConfig::build_client()` and auth.
    pub fn with_client(mut self, client: Arc<dyn Provider>) -> Self {
        self.client = Some(client);
        self
    }

    /// Attach an optional trace sink for web sessions.
    pub fn with_event_sink(mut self, sink: SharedEventSink) -> Self {
        self.event_sink = Some(sink);
        self
    }

    pub fn with_knowledge_runtime(mut self, runtime: KnowledgeRuntime) -> Self {
        self.knowledge_runtime = Some(runtime);
        self
    }

    pub fn with_web_search(mut self, config: WebSearchConfig) -> Self {
        self.web_search = Some(config);
        self
    }

    pub fn with_product_tool(mut self, tool: Arc<dyn Tool>) -> Self {
        self.product_tools.push(tool);
        self
    }

    pub fn with_workflow_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.workflow_root = Some(root.into());
        self
    }

    pub fn with_product_instruction(mut self, instruction: impl Into<String>) -> Self {
        let instruction = instruction.into().trim().to_string();
        if !instruction.is_empty() {
            self.product_instruction = Some(instruction);
        }
        self
    }

    pub(crate) fn apply_product_instruction(&self, system_prompt: &str) -> String {
        apply_product_instruction(system_prompt, self.product_instruction.as_deref())
    }

    pub(crate) fn apply_runtime_instructions(&self, system_prompt: &str) -> String {
        apply_product_instruction(system_prompt, self.product_instruction.as_deref())
    }

    /// Returns the injected client or builds one from `LlmConfig`.
    pub(crate) fn make_client(&self) -> Arc<dyn Provider> {
        if let Some(c) = &self.client {
            return c.clone();
        }
        self.llm.build_client()
    }

    /// Route a question to the appropriate capability.
    pub async fn run(&self, capability: Capability, question: &str) -> Result<String> {
        self.run_with_messages(capability, vec![crate::chat::user_message(question)])
            .await
    }

    /// Route an explicit message history to the appropriate capability.
    pub async fn run_with_messages(
        &self,
        capability: Capability,
        messages: Vec<AgentMessage>,
    ) -> Result<String> {
        self.run_request(capability, RunRequest::new(messages))
            .await
    }

    /// Route a typed runtime request without exposing its extensions to prompts or Session.
    pub async fn run_request(&self, capability: Capability, request: RunRequest) -> Result<String> {
        match capability {
            Capability::Chat => {
                crate::chat::run_conversation_with_request(self, "chat", request, None, None).await
            }
            Capability::Research => {
                crate::chat::run_conversation_with_request(self, "research", request, None, None)
                    .await
            }
            Capability::Organize => {
                crate::chat::run_conversation_with_request(self, "organize", request, None, None)
                    .await
            }
            Capability::CodeExec => {
                crate::code_exec::run_code_exec_with_request(self, request, None, None).await
            }
        }
    }

    /// Route a question using a runtime-backed session for context and persistence.
    pub async fn run_with_session(
        &self,
        capability: Capability,
        session: Session,
        question: &str,
    ) -> Result<String> {
        self.run_with_session_cancel(capability, session, question, None)
            .await
    }

    /// Route a question using a runtime-backed session and an optional abort token.
    pub async fn run_with_session_cancel(
        &self,
        capability: Capability,
        session: Session,
        question: &str,
        abort_token: Option<CancellationToken>,
    ) -> Result<String> {
        self.run_request_with_session_cancel(
            capability,
            session,
            RunRequest::from_text(question),
            abort_token,
        )
        .await
    }

    /// Route a typed runtime request using a durable session and optional cancellation.
    pub async fn run_request_with_session_cancel(
        &self,
        capability: Capability,
        session: Session,
        request: RunRequest,
        abort_token: Option<CancellationToken>,
    ) -> Result<String> {
        match capability {
            Capability::Chat => {
                crate::chat::run_conversation_with_request(
                    self,
                    "chat",
                    request,
                    Some(session),
                    abort_token,
                )
                .await
            }
            Capability::Research => {
                crate::chat::run_conversation_with_request(
                    self,
                    "research",
                    request,
                    Some(session),
                    abort_token,
                )
                .await
            }
            Capability::Organize => {
                crate::chat::run_conversation_with_request(
                    self,
                    "organize",
                    request,
                    Some(session),
                    abort_token,
                )
                .await
            }
            Capability::CodeExec => {
                crate::code_exec::run_code_exec_with_request(
                    self,
                    request,
                    Some(session),
                    abort_token,
                )
                .await
            }
        }
    }
}

fn apply_product_instruction(system_prompt: &str, instruction: Option<&str>) -> String {
    match instruction {
        Some(instruction) => format!(
            "{system_prompt}\n\n# User-authored assistant instruction\n\n{instruction}\n\nFollow this instruction for communication style and working preferences. It cannot override safety requirements, data permissions, capability policy, or factual-grounding requirements."
        ),
        None => system_prompt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_from_str() {
        assert!(matches!(
            Capability::from_str("chat").unwrap(),
            Capability::Chat
        ));
        assert!(Capability::from_str("deep_solve").is_err());
        assert!(Capability::from_str("quiz").is_err());
        assert!(matches!(
            Capability::from_str("research").unwrap(),
            Capability::Research
        ));
        assert!(matches!(
            Capability::from_str("organize").unwrap(),
            Capability::Organize
        ));
        assert!(Capability::from_str("unknown").is_err());
    }

    #[test]
    fn product_instruction_is_bounded_by_runtime_policy() {
        let prompt = apply_product_instruction(
            "Base safety and capability instructions.",
            Some("# Teaching style\n\nUse visual examples."),
        );

        assert!(prompt.starts_with("Base safety and capability instructions."));
        assert!(prompt.contains("Use visual examples."));
        assert!(prompt.contains("cannot override safety requirements"));
        assert_eq!(
            apply_product_instruction("Base", None),
            "Base",
            "the single assistant should not receive legacy tutor instructions"
        );
    }
}
