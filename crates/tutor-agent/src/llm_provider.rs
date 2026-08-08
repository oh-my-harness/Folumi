use std::sync::Arc;

use llm_adapter::{
    Provider,
    anthropic::AnthropicProvider,
    deepseek,
    openai::{OpenAIProvider, ThinkingScheme},
};
use llm_harness_agent::ModelInfo;
use llm_harness_types::ThinkingLevel;

use crate::error::{Result, TutorError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmProviderKind {
    Anthropic,
    DeepSeek,
    OpenAI,
}

#[derive(Debug, Clone)]
pub struct LlmConfig {
    pub provider: LlmProviderKind,
    pub model: String,
    pub api_key: String,
    pub api_key_env: Option<String>,
    pub base_url: Option<String>,
    pub chat_path: Option<String>,
    pub context_window_tokens: Option<u32>,
    pub thinking_level: ThinkingLevel,
}

impl LlmConfig {
    pub fn from_env() -> Result<Self> {
        let provider = std::env::var("LLM_PROVIDER")
            .unwrap_or_else(|_| "anthropic".into())
            .to_ascii_lowercase();

        match provider.as_str() {
            "anthropic" | "claude" => Self::from_env_for(
                LlmProviderKind::Anthropic,
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_BASE_URL",
                None,
                "claude-haiku-4-5-20251001",
            ),
            "deepseek" => Self::from_env_for(
                LlmProviderKind::DeepSeek,
                "DEEPSEEK_API_KEY",
                "DEEPSEEK_API_BASE",
                Some("DEEPSEEK_CHAT_PATH"),
                "deepseek-v4-flash",
            ),
            "openai" | "openai-compatible" => Self::from_env_for(
                LlmProviderKind::OpenAI,
                "OPENAI_API_KEY",
                "OPENAI_BASE_URL",
                Some("OPENAI_CHAT_PATH"),
                "gpt-4o-mini",
            ),
            other => Err(TutorError::Internal(format!(
                "unsupported LLM_PROVIDER `{other}`; expected anthropic, deepseek, or openai"
            ))),
        }
    }

    pub fn anthropic(model: impl Into<String>, api_key: impl Into<String>) -> Self {
        Self {
            provider: LlmProviderKind::Anthropic,
            model: model.into(),
            api_key: api_key.into(),
            api_key_env: Some("ANTHROPIC_API_KEY".into()),
            base_url: None,
            chat_path: None,
            context_window_tokens: Some(200_000),
            thinking_level: ThinkingLevel::Off,
        }
    }

    pub fn from_parts(
        provider: LlmProviderKind,
        model: impl Into<String>,
        api_key: impl Into<String>,
        base_url: Option<String>,
        chat_path: Option<String>,
        context_window_tokens: Option<u32>,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            api_key: api_key.into(),
            api_key_env: None,
            base_url,
            chat_path,
            context_window_tokens,
            thinking_level: ThinkingLevel::Off,
        }
    }

    pub fn with_thinking_level(mut self, level: ThinkingLevel) -> Self {
        self.thinking_level = level;
        self
    }

    fn from_env_for(
        provider: LlmProviderKind,
        api_key_env: &str,
        base_url_env: &str,
        chat_path_env: Option<&str>,
        default_model: &str,
    ) -> Result<Self> {
        let api_key = std::env::var(api_key_env)
            .map_err(|_| TutorError::Internal(format!("{api_key_env} not set")))?;
        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| default_model.into());
        let base_url = std::env::var(base_url_env)
            .ok()
            .filter(|v| !v.trim().is_empty());
        let chat_path = chat_path_env
            .and_then(|env| std::env::var(env).ok())
            .filter(|v| !v.trim().is_empty());

        Ok(Self {
            provider,
            model,
            api_key,
            api_key_env: Some(api_key_env.into()),
            base_url,
            chat_path,
            context_window_tokens: Some(default_context_window(provider)),
            thinking_level: ThinkingLevel::Off,
        })
    }

    pub fn build_client(&self) -> Arc<dyn Provider> {
        match self.provider {
            LlmProviderKind::Anthropic => {
                let mut builder = AnthropicProvider::builder(self.api_key.clone());
                if let Some(base_url) = &self.base_url {
                    builder = builder.base_url(base_url.clone());
                }
                Arc::new(builder.build())
            }
            LlmProviderKind::DeepSeek => {
                if self.base_url.is_none() && self.chat_path.is_none() {
                    Arc::new(deepseek::client(self.api_key.clone()))
                } else {
                    let mut builder = OpenAIProvider::builder(self.api_key.clone())
                        .parse_reasoning_content(true)
                        .tolerant_keepalive(true)
                        .thinking_scheme(ThinkingScheme::ThinkingToggle);
                    if let Some(base_url) = &self.base_url {
                        builder = builder.base_url(base_url.clone());
                    } else {
                        builder = builder.base_url("https://api.deepseek.com");
                    }
                    if let Some(chat_path) = &self.chat_path {
                        builder = builder.chat_path(chat_path.clone());
                    }
                    Arc::new(builder.build())
                }
            }
            LlmProviderKind::OpenAI => {
                let deepseek_compatible = self
                    .base_url
                    .as_deref()
                    .is_some_and(|url| url.to_ascii_lowercase().contains("deepseek"));
                let mut builder = OpenAIProvider::builder(self.api_key.clone())
                    .parse_reasoning_content(true)
                    .tolerant_keepalive(true)
                    .thinking_scheme(if deepseek_compatible {
                        ThinkingScheme::ThinkingToggle
                    } else {
                        ThinkingScheme::ReasoningEffort
                    });
                if let Some(base_url) = &self.base_url {
                    builder = builder.base_url(base_url.clone());
                }
                if let Some(chat_path) = &self.chat_path {
                    builder = builder.chat_path(chat_path.clone());
                }
                Arc::new(builder.build())
            }
        }
    }

    pub fn model_info(&self, max_tokens: u32) -> ModelInfo {
        ModelInfo {
            context_window: self.context_window_tokens.unwrap_or(200_000),
            max_tokens,
        }
    }
}

fn default_context_window(provider: LlmProviderKind) -> u32 {
    match provider {
        LlmProviderKind::Anthropic => 200_000,
        LlmProviderKind::DeepSeek | LlmProviderKind::OpenAI => 128_000,
    }
}
