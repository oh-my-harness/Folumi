use std::sync::Arc;

use llm_adapter::types::{Message as AdapterMessage, ResponseContent};
use llm_harness_agent::{AgentHarness, HarnessHooks, ModelInfo, Plugin, Session};
use llm_harness_loop::{ConvertToLlmHook, DefaultConvertToLlm, FinalAnswerMode, LlmClient};
use llm_harness_runtime::builder::HarnessBuilder;
use llm_harness_types::{
    AgentError, AgentMessage, BeforeToolCallHook, ExecutionEnv, PrepareNextTurnHook, ThinkingLevel,
    Tool,
};

use crate::error::{Result, TutorError};

pub struct RuntimeHarnessConfig {
    pub model: String,
    pub model_info: ModelInfo,
    pub tools: Vec<Arc<dyn Tool>>,
    pub plugins: Vec<Arc<dyn Plugin>>,
    pub system_prompt: String,
    pub final_answer_mode: FinalAnswerMode,
    pub thinking_level: ThinkingLevel,
    pub before_tool_call: Vec<Arc<dyn BeforeToolCallHook>>,
    pub prepare_next_turn: Vec<Arc<dyn PrepareNextTurnHook>>,
}

pub async fn build_runtime_harness(
    client: Arc<dyn LlmClient>,
    env: Arc<dyn ExecutionEnv>,
    session: Option<Session>,
    config: RuntimeHarnessConfig,
) -> Result<AgentHarness> {
    let mut hooks = HarnessHooks::none();
    hooks.before_tool_call = config.before_tool_call;
    hooks.prepare_next_turn = config.prepare_next_turn;

    let hook_plugin = HookPlugin { hooks };
    let mut builder = HarnessBuilder::new(config.model.clone())
        .provider(config.model.clone(), client)
        .install(&hook_plugin)
        .system_prompt(Some(config.system_prompt))
        .model_info(Some(config.model_info))
        .convert_to_llm(Some(Arc::new(OpenAiSafeContextConverter::default())))
        .final_answer_mode(config.final_answer_mode);
    builder = builder.thinking_level(config.thinking_level);

    for tool in config.tools {
        builder = builder.tool(tool);
    }

    // Runtime-owned plugin tools are installed last so their trusted boundaries
    // win over any accidental product-tool name collision.
    for plugin in config.plugins {
        builder = builder.install(plugin.as_ref());
    }

    match session {
        Some(session) => builder
            .build_with_session(env, session)
            .map_err(|err| TutorError::Internal(err.to_string())),
        None => builder
            .build(env)
            .await
            .map_err(|err| TutorError::Internal(err.to_string())),
    }
}

#[derive(Default)]
struct OpenAiSafeContextConverter {
    inner: DefaultConvertToLlm,
}

impl ConvertToLlmHook for OpenAiSafeContextConverter {
    fn convert<'a>(
        &'a self,
        messages: &'a [AgentMessage],
    ) -> futures::future::BoxFuture<'a, std::result::Result<Vec<AdapterMessage>, AgentError>> {
        Box::pin(async move {
            let converted = self.inner.convert(messages).await?;
            Ok(converted
                .into_iter()
                .filter(|message| match message {
                    AdapterMessage::Assistant(content) => assistant_has_openai_payload(content),
                    _ => true,
                })
                .collect())
        })
    }
}

fn assistant_has_openai_payload(content: &[ResponseContent]) -> bool {
    content.iter().any(|item| match item {
        ResponseContent::Text(text) => !text.trim().is_empty(),
        ResponseContent::ToolInvocation(_) => true,
        ResponseContent::Reasoning { .. } => false,
    })
}

struct HookPlugin {
    hooks: HarnessHooks,
}

impl Plugin for HookPlugin {
    fn name(&self) -> &str {
        "tutor-runtime-hooks"
    }

    fn register_hooks(&self, target: &mut HarnessHooks) {
        target
            .before_tool_call
            .extend(self.hooks.before_tool_call.clone());
        target
            .prepare_next_turn
            .extend(self.hooks.prepare_next_turn.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llm_harness_types::{AssistantMessage, AssistantMessageKind, ContentBlock, StopReason};

    #[tokio::test]
    async fn openai_safe_converter_drops_reasoning_only_assistant_messages() {
        let converter = OpenAiSafeContextConverter::default();
        let messages = vec![AgentMessage::Assistant(AssistantMessage {
            kind: AssistantMessageKind::Progress,
            message_id: "msg_reasoning".into(),
            turn_id: "turn_reasoning".into(),
            content: vec![ContentBlock::Thinking {
                thinking: "internal reasoning only".into(),
                signature: None,
            }],
            stop_reason: Some(StopReason::EndTurn),
            timestamp: chrono::Utc::now(),
            provider: None,
            api: None,
            model: None,
            usage: None,
            error_message: None,
        })];

        let converted = converter.convert(&messages).await.unwrap();

        assert!(converted.is_empty());
    }

    #[tokio::test]
    async fn openai_safe_converter_keeps_tool_call_assistant_messages() {
        let converter = OpenAiSafeContextConverter::default();
        let messages = vec![AgentMessage::Assistant(AssistantMessage {
            kind: AssistantMessageKind::Progress,
            message_id: "msg_tool".into(),
            turn_id: "turn_tool".into(),
            content: vec![
                ContentBlock::Thinking {
                    thinking: "plan".into(),
                    signature: None,
                },
                ContentBlock::ToolUse {
                    id: "call_1".into(),
                    name: "web_search".into(),
                    input: serde_json::json!({"query": "rust"}),
                },
            ],
            stop_reason: Some(StopReason::ToolUse),
            timestamp: chrono::Utc::now(),
            provider: None,
            api: None,
            model: None,
            usage: None,
            error_message: None,
        })];

        let converted = converter.convert(&messages).await.unwrap();

        assert_eq!(converted.len(), 1);
    }
}
