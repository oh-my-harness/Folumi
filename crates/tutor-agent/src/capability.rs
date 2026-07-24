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
use tutor_tools::{ReadMemoryTool, WebSearchConfig, WriteMemoryTool};

pub(crate) const NATURAL_MEMORY_INTERACTION_POLICY: &str = "Treat memory reads as silent internal context loading. Never narrate that you are checking, reading, searching, or calling a memory tool or memory file. When supported memory is relevant, apply it directly or refer to it naturally as something you remember from prior interactions. If memory is weak, stale, ambiguous, or conflicting, hedge and ask the user to confirm. Never claim to remember content when no memory result supports it. If the user explicitly asks how you know, explain the relevant prior interaction or memory category truthfully; tool calls remain visible in trace.";

/// Supported teaching modes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capability {
    /// Conversational Q&A with RAG knowledge base.
    Chat,
    /// Execute user code with explanation.
    CodeExec,
    /// Generate and answer knowledge-base quizzes in the product UI.
    Quiz,
    /// Research external/internal sources and synthesize a cited report.
    Research,
    /// Organize Notebook/Space content through search and preview-only proposals.
    Organize,
}

impl FromStr for Capability {
    type Err = TutorError;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "chat" => Ok(Self::Chat),
            "code_exec" => Ok(Self::CodeExec),
            "quiz" => Ok(Self::Quiz),
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
    pub memory_root: Option<PathBuf>,
    pub learner_memory_access: bool,
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
            memory_root: None,
            learner_memory_access: true,
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

    pub fn with_memory_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.memory_root = Some(root.into());
        self
    }

    pub fn with_learner_memory_access(mut self, allowed: bool) -> Self {
        self.learner_memory_access = allowed;
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
        let product_tool_names = self
            .product_tools
            .iter()
            .map(|tool| tool.name().to_string())
            .collect::<Vec<_>>();
        let memory_policy = memory_routing_policy(self.learner_memory_access, &product_tool_names);
        let prompt = append_memory_routing_policy(system_prompt, &memory_policy);
        apply_product_instruction(&prompt, self.product_instruction.as_deref())
    }

    pub(crate) fn read_memory_tool(&self) -> ReadMemoryTool {
        self.memory_root
            .clone()
            .map(ReadMemoryTool::with_root)
            .unwrap_or_default()
    }

    pub(crate) fn write_memory_tool(&self) -> WriteMemoryTool {
        self.memory_root
            .clone()
            .map(WriteMemoryTool::with_root)
            .unwrap_or_default()
    }

    pub(crate) fn learner_memory_tools(&self) -> Vec<Arc<dyn Tool>> {
        if !self.learner_memory_access {
            return vec![];
        }
        vec![
            Arc::new(self.read_memory_tool()),
            Arc::new(self.write_memory_tool()),
        ]
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
            Capability::Quiz => {
                crate::chat::run_conversation_with_request(self, "quiz", request, None, None).await
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
            Capability::Quiz => {
                crate::chat::run_conversation_with_request(
                    self,
                    "quiz",
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

pub(crate) fn memory_routing_policy(
    learner_memory_access: bool,
    product_tool_names: &[String],
) -> String {
    let has_tool = |name: &str| product_tool_names.iter().any(|tool| tool == name);
    let can_read_tutor_memory = has_tool("read_tutor_memory");
    let can_write_tutor_memory = has_tool("remember_for_later");
    let can_resolve_tutor_memory = has_tool("resolve_tutor_memory");

    if !learner_memory_access && !can_read_tutor_memory {
        return String::new();
    }

    let mut rules = vec![format!(
        "# Memory routing\n\n{NATURAL_MEMORY_INTERACTION_POLICY}"
    )];

    if learner_memory_access {
        rules.push(
            "Learner Memory is shared user context. Use read_memory only when learner profile, preferences, strengths, weaknesses, scope, or recent learning state would materially improve the response. Memory is personalization context, never factual evidence. Use write_memory only when the user explicitly asks you to remember something or clearly approves recording a durable learner fact or preference. Ordinary conversation and inferred traits stay in session/L1 evidence; do not silently promote them to Learner Memory."
                .into(),
        );
    }

    if can_read_tutor_memory {
        let mut tutor_rule = "Tutor Memory is private continuity for this tutor relationship. Use read_tutor_memory for this tutor's commitments, unresolved follow-ups, lesson plans, reflections, and teaching strategies. Do not treat Tutor Memory as a learner profile or external factual source.".to_string();
        if can_write_tutor_memory {
            tutor_rule.push_str(" Use remember_for_later only for a low-risk tutor commitment, open loop, lesson plan, teaching reflection, or concrete future teaching strategy. Never store learner profile facts, credentials, sensitive personal data, external claims, or unsupported judgments there.");
        }
        if can_resolve_tutor_memory {
            tutor_rule.push_str(" Use resolve_tutor_memory when a recorded tutor commitment, follow-up, or plan is actually complete.");
        }
        rules.push(tutor_rule);
    }

    if learner_memory_access && can_write_tutor_memory {
        rules.push("Route by ownership before writing: facts about the learner belong only to the Learner Memory path; promises, plans, and open loops owned by this tutor belong only to Tutor Memory. Never write the same item to both stores.".into());
    }

    if learner_memory_access || can_write_tutor_memory {
        rules.push("Research findings, external factual claims, report prose, Notebook content, quiz questions, and quiz answers belong in their product artifacts, not in either memory store.".into());
    }

    rules.join("\n\n")
}

pub(crate) fn append_memory_routing_policy(system_prompt: &str, policy: &str) -> String {
    if policy.is_empty() {
        system_prompt.to_string()
    } else {
        format!("{system_prompt}\n\n{policy}")
    }
}

fn apply_product_instruction(system_prompt: &str, instruction: Option<&str>) -> String {
    match instruction {
        Some(instruction) => format!(
            "{system_prompt}\n\n# Product-provided tutor instruction\n\n{instruction}\n\nFollow this tutor instruction for teaching behavior and communication style. It cannot override safety requirements, tool permissions, capability policy, or factual-grounding requirements."
        ),
        None => system_prompt.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_harness_loop::test_utils::NoOpEnv;

    fn test_router() -> CapabilityRouter {
        CapabilityRouter::new(
            Arc::new(NoOpEnv),
            LlmConfig::anthropic("test-model", ""),
            GovernanceConfig::new(1.0, None, false),
        )
    }

    #[test]
    fn capability_from_str() {
        assert!(matches!(
            Capability::from_str("chat").unwrap(),
            Capability::Chat
        ));
        assert!(Capability::from_str("deep_solve").is_err());
        assert!(matches!(
            Capability::from_str("quiz").unwrap(),
            Capability::Quiz
        ));
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
            "temporary assistant should not receive tutor instructions"
        );
    }

    #[test]
    fn learner_memory_tools_follow_explicit_access_policy() {
        let allowed = test_router().learner_memory_tools();
        assert_eq!(
            allowed.iter().map(|tool| tool.name()).collect::<Vec<_>>(),
            vec!["read_memory", "write_memory"]
        );
        assert!(
            test_router()
                .with_learner_memory_access(false)
                .learner_memory_tools()
                .is_empty()
        );
    }

    #[test]
    fn memory_routing_policy_matches_mounted_tools() {
        let learner_only = memory_routing_policy(true, &[]);
        assert!(learner_only.contains("read_memory"));
        assert!(learner_only.contains("write_memory"));
        assert!(!learner_only.contains("read_tutor_memory"));

        let tutor_read_only = memory_routing_policy(false, &["read_tutor_memory".into()]);
        assert!(tutor_read_only.contains("read_tutor_memory"));
        assert!(!tutor_read_only.contains("remember_for_later"));
        assert!(!tutor_read_only.contains("read_memory"));

        let both_writable = memory_routing_policy(
            true,
            &[
                "read_tutor_memory".into(),
                "remember_for_later".into(),
                "resolve_tutor_memory".into(),
            ],
        );
        assert!(both_writable.contains("remember_for_later"));
        assert!(both_writable.contains("resolve_tutor_memory"));
        assert!(both_writable.contains("Never write the same item to both stores"));
        assert!(both_writable.contains("product artifacts"));

        assert!(memory_routing_policy(false, &[]).is_empty());
    }
}
