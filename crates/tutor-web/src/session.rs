use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use llm_harness_agent::{
    JsonlSessionRepo, Session, SessionRepo,
    session::{CreateSessionOptions, ListSessionOptions, SessionEntryPayload, SessionMetadata},
};
use llm_harness_types::{
    AgentMessage, AssistantMessageKind, ContentBlock, EntryId, SessionError, TokenUsage,
};
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::stream::TutorStream;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmSessionConfig {
    pub provider: String,
    pub model: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub chat_path: Option<String>,
    pub context_window_tokens: Option<u32>,
    pub budget_limit_usd: Option<f64>,
    pub require_approval: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchSessionConfig {
    pub provider: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub max_results: Option<usize>,
    pub fetch_timeout_secs: Option<u64>,
    pub max_fetch_chars: Option<usize>,
}

#[derive(Clone)]
pub struct SessionCreateConfig {
    pub capability: String,
    pub kb: Option<String>,
    pub notebook_enabled: bool,
    pub llm: Option<LlmSessionConfig>,
    pub search: Option<SearchSessionConfig>,
    pub embedding: Option<tutor_rag::EmbeddingConfig>,
}

/// Product metadata for an active tutor session.
#[derive(Clone)]
pub struct SessionEntry {
    pub id: String,
    pub tutor_id: Option<String>,
    pub capability: String,
    pub kb: Option<String>,
    pub notebook_enabled: bool,
    pub llm: Option<LlmSessionConfig>,
    pub search: Option<SearchSessionConfig>,
    pub embedding: Option<tutor_rag::EmbeddingConfig>,
    pub stream: TutorStream,
}

#[derive(Clone)]
pub struct RuntimeSessionSummary {
    pub id: String,
    pub name: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedTraceEntry {
    pub kind: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub payload: serde_json::Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedCompactSummary {
    pub summary: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub message_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedMessageMentions {
    pub user_message_index: usize,
    pub mentions: Vec<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedMessageCitations {
    pub assistant_message_index: usize,
    pub citations: Vec<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistedMessageArtifacts {
    pub assistant_message_index: usize,
    pub artifacts: Vec<serde_json::Value>,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ActiveRunSummary {
    pub run_id: String,
    pub session_id: String,
    pub capability: String,
    pub status: String,
    #[serde(default)]
    pub current_stage: Option<String>,
    pub started_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone)]
struct ActiveRunRecord {
    summary: ActiveRunSummary,
    cancel: CancellationToken,
}

/// Thread-safe pool of active web session metadata plus runtime session repo.
pub struct SessionPool {
    sessions: Mutex<HashMap<String, SessionEntry>>,
    active_runs: Mutex<HashMap<String, ActiveRunRecord>>,
    product_metadata: Mutex<HashMap<String, ProductSessionMetadata>>,
    product_metadata_path: PathBuf,
    repo: Arc<JsonlSessionRepo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProductSessionMetadata {
    #[serde(default)]
    tutor_id: Option<String>,
    capability: String,
    #[serde(default)]
    kb: Option<String>,
    #[serde(default)]
    notebook_enabled: bool,
    #[serde(default)]
    llm: Option<LlmSessionConfig>,
    #[serde(default)]
    search: Option<SearchSessionConfig>,
    #[serde(default)]
    embedding: Option<tutor_rag::EmbeddingConfig>,
}

impl SessionPool {
    #[allow(dead_code)]
    pub fn new() -> Arc<Self> {
        let root = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(".llm-tutor")
            .join("sessions");
        Self::new_with_root(root)
    }

    pub fn new_with_root(root: impl Into<PathBuf>) -> Arc<Self> {
        let root = root.into();
        std::fs::create_dir_all(&root).expect("failed to create runtime session directory");
        let product_metadata_path = root.join("product-metadata.json");
        let product_metadata = read_product_metadata(&product_metadata_path).unwrap_or_default();
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            active_runs: Mutex::new(HashMap::new()),
            product_metadata: Mutex::new(product_metadata),
            product_metadata_path,
            repo: Arc::new(JsonlSessionRepo::new(root)),
        })
    }

    /// Create a runtime-backed session and return its ID.
    #[allow(dead_code)]
    pub async fn create(
        &self,
        capability: &str,
        kb: Option<String>,
        notebook_enabled: bool,
        llm: Option<LlmSessionConfig>,
        search: Option<SearchSessionConfig>,
        embedding: Option<tutor_rag::EmbeddingConfig>,
    ) -> Result<String, llm_harness_types::SessionError> {
        self.create_with_tutor(
            None,
            SessionCreateConfig {
                capability: capability.to_string(),
                kb,
                notebook_enabled,
                llm,
                search,
                embedding,
            },
        )
        .await
    }

    pub async fn create_with_tutor(
        &self,
        tutor_id: Option<String>,
        config: SessionCreateConfig,
    ) -> Result<String, llm_harness_types::SessionError> {
        let SessionCreateConfig {
            capability,
            kb,
            notebook_enabled,
            llm,
            search,
            embedding,
        } = config;
        let storage = self
            .repo
            .create(CreateSessionOptions {
                name: None,
                initial_model: llm.as_ref().map(|config| config.model.clone()),
                initial_thinking_level: None,
                initial_tools: vec![],
            })
            .await?;
        let meta = storage.metadata().await?;
        let id = meta.id.clone();
        let entry = SessionEntry {
            id: id.clone(),
            tutor_id: tutor_id.clone(),
            capability: capability.clone(),
            kb: kb.clone(),
            notebook_enabled,
            llm: llm.clone(),
            search: search.clone(),
            embedding: embedding.clone(),
            stream: TutorStream::new(128),
        };
        self.sessions.lock().unwrap().insert(id.clone(), entry);
        self.upsert_product_metadata(
            &id,
            ProductSessionMetadata {
                tutor_id,
                capability,
                kb,
                notebook_enabled,
                llm,
                search,
                embedding,
            },
        );
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Option<SessionEntry> {
        self.sessions.lock().unwrap().get(id).cloned()
    }

    pub async fn ensure_entry(&self, id: &str) -> Option<SessionEntry> {
        if let Some(entry) = self.get(id) {
            return Some(entry);
        }

        let storage = self.repo.open(id).await.ok()?;
        let meta = storage.metadata().await.ok()?;
        let product = self.product_metadata.lock().unwrap().get(id).cloned();
        let entry = SessionEntry {
            id: meta.id.clone(),
            tutor_id: product.as_ref().and_then(|value| value.tutor_id.clone()),
            capability: product
                .as_ref()
                .map(|value| value.capability.clone())
                .unwrap_or_else(|| "chat".into()),
            kb: product.as_ref().and_then(|value| value.kb.clone()),
            notebook_enabled: product
                .as_ref()
                .map(|value| value.notebook_enabled)
                .unwrap_or(false),
            llm: product.as_ref().and_then(|value| value.llm.clone()),
            search: product.as_ref().and_then(|value| value.search.clone()),
            embedding: product.as_ref().and_then(|value| value.embedding.clone()),
            stream: TutorStream::new(128),
        };
        self.sessions
            .lock()
            .unwrap()
            .insert(meta.id.clone(), entry.clone());
        Some(entry)
    }

    pub async fn open_runtime_session(
        &self,
        id: &str,
    ) -> Result<Session, llm_harness_types::SessionError> {
        let storage = self.repo.open(id).await?;
        Ok(Session::new(storage))
    }

    pub fn try_start_active_run(
        &self,
        session_id: &str,
        capability: &str,
    ) -> Option<(String, CancellationToken)> {
        let mut active_runs = self.active_runs.lock().unwrap();
        if active_runs.contains_key(session_id) {
            return None;
        }

        let now = chrono::Utc::now();
        let run_id = uuid::Uuid::new_v4().to_string();
        let cancel = CancellationToken::new();
        active_runs.insert(
            session_id.to_string(),
            ActiveRunRecord {
                summary: ActiveRunSummary {
                    run_id: run_id.clone(),
                    session_id: session_id.to_string(),
                    capability: capability.to_string(),
                    status: "running".into(),
                    current_stage: None,
                    started_at: now,
                    updated_at: now,
                },
                cancel: cancel.clone(),
            },
        );
        Some((run_id, cancel))
    }

    pub fn active_run(&self, session_id: &str) -> Option<ActiveRunSummary> {
        self.active_runs
            .lock()
            .unwrap()
            .get(session_id)
            .map(|record| record.summary.clone())
    }

    pub fn cancel_active_run(&self, session_id: &str) -> Option<ActiveRunSummary> {
        let mut active_runs = self.active_runs.lock().unwrap();
        let record = active_runs.get_mut(session_id)?;
        record.cancel.cancel();
        record.summary.status = "cancelling".into();
        record.summary.updated_at = chrono::Utc::now();
        Some(record.summary.clone())
    }

    pub fn update_active_run_stage(
        &self,
        session_id: &str,
        run_id: &str,
        stage: &str,
    ) -> Option<ActiveRunSummary> {
        let stage = stage.trim();
        if stage.is_empty() {
            return None;
        }
        let mut active_runs = self.active_runs.lock().unwrap();
        let record = active_runs.get_mut(session_id)?;
        if record.summary.run_id != run_id || record.summary.current_stage.as_deref() == Some(stage)
        {
            return None;
        }
        record.summary.current_stage = Some(stage.to_string());
        record.summary.updated_at = chrono::Utc::now();
        Some(record.summary.clone())
    }

    pub fn terminal_active_run(
        &self,
        session_id: &str,
        run_id: &str,
        status: &str,
    ) -> Option<ActiveRunSummary> {
        let mut active_runs = self.active_runs.lock().unwrap();
        let record = active_runs.get_mut(session_id)?;
        if record.summary.run_id != run_id {
            return None;
        }
        record.summary.status = status.to_string();
        record.summary.updated_at = chrono::Utc::now();
        Some(record.summary.clone())
    }

    pub async fn append_run_state(
        &self,
        id: &str,
        run: &ActiveRunSummary,
    ) -> Result<(), llm_harness_types::SessionError> {
        self.open_runtime_session(id)
            .await?
            .append(SessionEntryPayload::Custom {
                custom_type: "run_state".into(),
                data: serde_json::to_value(run).unwrap_or_default(),
            })
            .await?;
        Ok(())
    }

    pub async fn latest_run_state(
        &self,
        id: &str,
    ) -> Result<Option<ActiveRunSummary>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;
        Ok(entries.into_iter().rev().find_map(|entry| {
            let SessionEntryPayload::Custom { custom_type, data } = entry.payload else {
                return None;
            };
            if custom_type != "run_state" {
                return None;
            }
            serde_json::from_value(data).ok()
        }))
    }

    pub async fn recovered_run_state(
        &self,
        id: &str,
    ) -> Result<Option<ActiveRunSummary>, llm_harness_types::SessionError> {
        if let Some(active) = self.active_run(id) {
            return Ok(Some(active));
        }
        let mut latest = self.latest_run_state(id).await?;
        if let Some(run) = latest.as_mut()
            && matches!(
                run.status.as_str(),
                "queued" | "running" | "waiting" | "cancelling"
            )
        {
            run.status = "interrupted".into();
        }
        Ok(latest)
    }

    pub fn finish_active_run(&self, session_id: &str, run_id: &str) {
        let mut active_runs = self.active_runs.lock().unwrap();
        if active_runs
            .get(session_id)
            .is_some_and(|record| record.summary.run_id == run_id)
        {
            active_runs.remove(session_id);
        }
    }

    pub async fn repair_incomplete_tool_call_context(
        &self,
        id: &str,
    ) -> Result<bool, SessionError> {
        let session = self.open_runtime_session(id).await?;
        let entries = session.read_active_path().await?;
        let mut last_valid_message_entry: Option<EntryId> = None;
        let mut pending_tool_calls: Option<(Vec<String>, Option<EntryId>)> = None;

        for entry in entries {
            let SessionEntryPayload::Message(message) = &entry.payload else {
                continue;
            };

            if let Some((_, rewind_to)) = &pending_tool_calls
                && !matches!(message, AgentMessage::ToolResult(_))
            {
                return rewind_incomplete_tool_call(&session, *rewind_to).await;
            }

            match message {
                AgentMessage::Assistant(message) => {
                    let ids = tool_use_ids(&message.content);
                    if ids.is_empty() {
                        last_valid_message_entry = Some(entry.id);
                    } else {
                        pending_tool_calls = Some((ids, last_valid_message_entry));
                    }
                }
                AgentMessage::ToolResult(message) => {
                    let Some((ids, rewind_to)) = pending_tool_calls.as_mut() else {
                        return rewind_incomplete_tool_call(&session, last_valid_message_entry)
                            .await;
                    };
                    let Some(index) = ids.iter().position(|id| id == &message.tool_use_id) else {
                        return rewind_incomplete_tool_call(&session, *rewind_to).await;
                    };
                    ids.remove(index);
                    if ids.is_empty() {
                        pending_tool_calls = None;
                        last_valid_message_entry = Some(entry.id);
                    }
                }
                _ => {
                    last_valid_message_entry = Some(entry.id);
                }
            }
        }

        if let Some((_, rewind_to)) = pending_tool_calls {
            return rewind_incomplete_tool_call(&session, rewind_to).await;
        }

        Ok(false)
    }

    pub async fn metadata(
        &self,
        id: &str,
    ) -> Result<SessionMetadata, llm_harness_types::SessionError> {
        self.repo.open(id).await?.metadata().await
    }

    pub async fn history_len(&self, id: &str) -> usize {
        match self.open_runtime_session(id).await {
            Ok(session) => session
                .build_context()
                .await
                .map(|ctx| ctx.messages.len())
                .unwrap_or(0),
            Err(_) => 0,
        }
    }

    pub async fn messages(
        &self,
        id: &str,
    ) -> Result<Vec<llm_harness_types::AgentMessage>, llm_harness_types::SessionError> {
        Ok(self
            .open_runtime_session(id)
            .await?
            .build_context()
            .await?
            .messages)
    }

    pub async fn fork_before_message(
        &self,
        id: &str,
        message_index: usize,
        label: Option<String>,
    ) -> Result<bool, llm_harness_types::SessionError> {
        let session = self.open_runtime_session(id).await?;
        let entries = session.read_active_path().await?;
        let mut displayed_message_index = 0usize;
        let mut previous_entry_id: Option<EntryId> = None;

        for entry in entries {
            if let SessionEntryPayload::Message(message) = &entry.payload
                && message_role(message).is_some()
            {
                if displayed_message_index == message_index {
                    if let Some(target) = previous_entry_id {
                        session.fork_branch(target, label).await?;
                        return Ok(true);
                    }
                    return Ok(false);
                }
                displayed_message_index += 1;
            }
            previous_entry_id = Some(entry.id);
        }

        Err(llm_harness_types::SessionError::EntryNotFound(
            EntryId::new(),
        ))
    }

    pub async fn latest_usage(
        &self,
        id: &str,
    ) -> Result<Option<TokenUsage>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;

        for entry in entries.into_iter().rev() {
            match entry.payload {
                SessionEntryPayload::Custom { custom_type, data }
                    if custom_type == "trace_event" =>
                {
                    if let Some(usage) = token_usage_from_runtime_trace(&data) {
                        return Ok(Some(usage));
                    }
                }
                SessionEntryPayload::Message(AgentMessage::Assistant(message))
                    if message.usage.is_some() =>
                {
                    return Ok(message.usage);
                }
                _ => {}
            }
        }

        Ok(None)
    }

    pub async fn append_trace(
        &self,
        id: &str,
        kind: &str,
        payload: serde_json::Value,
    ) -> Result<(), llm_harness_types::SessionError> {
        self.open_runtime_session(id)
            .await?
            .append(SessionEntryPayload::Custom {
                custom_type: "trace_event".into(),
                data: serde_json::json!({
                    "kind": kind,
                    "payload": payload,
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn append_message_mentions(
        &self,
        id: &str,
        user_message_index: usize,
        mentions: Vec<serde_json::Value>,
    ) -> Result<(), llm_harness_types::SessionError> {
        if mentions.is_empty() {
            return Ok(());
        }
        self.open_runtime_session(id)
            .await?
            .append(SessionEntryPayload::Custom {
                custom_type: "message_mentions".into(),
                data: serde_json::json!({
                    "user_message_index": user_message_index,
                    "mentions": mentions,
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn message_mentions(
        &self,
        id: &str,
    ) -> Result<Vec<PersistedMessageMentions>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let SessionEntryPayload::Custom { custom_type, data } = entry.payload else {
                    return None;
                };
                if custom_type != "message_mentions" {
                    return None;
                }
                let user_message_index = data.get("user_message_index")?.as_u64()? as usize;
                let mentions = data.get("mentions")?.as_array()?.clone();
                Some(PersistedMessageMentions {
                    user_message_index,
                    mentions,
                    timestamp: entry.timestamp,
                })
            })
            .collect())
    }

    pub async fn append_message_citations(
        &self,
        id: &str,
        assistant_message_index: usize,
        citations: Vec<serde_json::Value>,
    ) -> Result<(), llm_harness_types::SessionError> {
        if citations.is_empty() {
            return Ok(());
        }
        self.open_runtime_session(id)
            .await?
            .append(SessionEntryPayload::Custom {
                custom_type: "message_citations".into(),
                data: serde_json::json!({
                    "assistant_message_index": assistant_message_index,
                    "citations": citations,
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn message_citations(
        &self,
        id: &str,
    ) -> Result<Vec<PersistedMessageCitations>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let SessionEntryPayload::Custom { custom_type, data } = entry.payload else {
                    return None;
                };
                if custom_type != "message_citations" {
                    return None;
                }
                let assistant_message_index =
                    data.get("assistant_message_index")?.as_u64()? as usize;
                let citations = data.get("citations")?.as_array()?.clone();
                Some(PersistedMessageCitations {
                    assistant_message_index,
                    citations,
                    timestamp: entry.timestamp,
                })
            })
            .collect())
    }

    pub async fn append_message_artifacts(
        &self,
        id: &str,
        assistant_message_index: usize,
        artifacts: Vec<serde_json::Value>,
    ) -> Result<(), llm_harness_types::SessionError> {
        if artifacts.is_empty() {
            return Ok(());
        }
        self.open_runtime_session(id)
            .await?
            .append(SessionEntryPayload::Custom {
                custom_type: "message_artifacts".into(),
                data: serde_json::json!({
                    "assistant_message_index": assistant_message_index,
                    "artifacts": artifacts,
                }),
            })
            .await?;
        Ok(())
    }

    pub async fn message_artifacts(
        &self,
        id: &str,
    ) -> Result<Vec<PersistedMessageArtifacts>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let SessionEntryPayload::Custom { custom_type, data } = entry.payload else {
                    return None;
                };
                if custom_type != "message_artifacts" {
                    return None;
                }
                let assistant_message_index =
                    data.get("assistant_message_index")?.as_u64()? as usize;
                let artifacts = data.get("artifacts")?.as_array()?.clone();
                Some(PersistedMessageArtifacts {
                    assistant_message_index,
                    artifacts,
                    timestamp: entry.timestamp,
                })
            })
            .collect())
    }

    pub async fn assistant_message_count(
        &self,
        id: &str,
    ) -> Result<usize, llm_harness_types::SessionError> {
        let messages = self.messages(id).await?;
        Ok(messages
            .iter()
            .filter(|message| message_role(message) == Some("assistant"))
            .count())
    }

    pub async fn traces(
        &self,
        id: &str,
    ) -> Result<Vec<PersistedTraceEntry>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;
        Ok(entries
            .into_iter()
            .filter_map(|entry| {
                let SessionEntryPayload::Custom { custom_type, data } = entry.payload else {
                    return None;
                };
                if custom_type != "trace_event" {
                    return None;
                }
                let kind = data.get("kind")?.as_str()?.to_string();
                let payload = data.get("payload").cloned().unwrap_or_default();
                Some(PersistedTraceEntry {
                    kind,
                    timestamp: entry.timestamp,
                    payload,
                })
            })
            .collect())
    }

    pub async fn compact_summary(
        &self,
        id: &str,
    ) -> Result<Option<PersistedCompactSummary>, llm_harness_types::SessionError> {
        let entries = self
            .open_runtime_session(id)
            .await?
            .read_active_path()
            .await?;

        Ok(entries.iter().rev().find_map(|entry| {
            let SessionEntryPayload::Compaction(compaction) = &entry.payload else {
                return None;
            };
            let AgentMessage::CompactionSummary(message) = &compaction.summary_message else {
                return None;
            };
            Some(PersistedCompactSummary {
                summary: message.summary.clone(),
                timestamp: entry.timestamp,
                message_count: 0,
            })
        }))
    }

    pub async fn list(
        &self,
        limit: Option<usize>,
    ) -> Result<Vec<RuntimeSessionSummary>, llm_harness_types::SessionError> {
        let metas = self
            .repo
            .list(ListSessionOptions {
                limit,
                ..Default::default()
            })
            .await?;
        Ok(metas
            .into_iter()
            .map(|meta| RuntimeSessionSummary {
                id: meta.id,
                name: meta.name,
                created_at: meta.created_at,
                updated_at: meta.updated_at,
                model: meta.model,
            })
            .collect())
    }

    pub fn set_capability(&self, id: &str, capability: &str) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(entry) = sessions.get_mut(id) else {
            return false;
        };

        entry.capability = capability.to_string();
        drop(sessions);
        self.update_product_metadata(id, |metadata| {
            metadata.capability = capability.to_string();
        });
        true
    }

    pub fn set_knowledge(
        &self,
        id: &str,
        kb: Option<String>,
        embedding: Option<tutor_rag::EmbeddingConfig>,
    ) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(entry) = sessions.get_mut(id) else {
            return false;
        };

        let has_kb = kb.is_some();
        entry.kb = kb;
        entry.embedding = embedding;
        if has_kb {
            entry.notebook_enabled = false;
        }
        let kb = entry.kb.clone();
        let embedding = entry.embedding.clone();
        let notebook_enabled = entry.notebook_enabled;
        drop(sessions);
        self.update_product_metadata(id, |metadata| {
            metadata.kb = kb;
            metadata.embedding = embedding;
            metadata.notebook_enabled = notebook_enabled;
        });
        true
    }

    pub fn set_notebook_enabled(&self, id: &str, notebook_enabled: bool) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(entry) = sessions.get_mut(id) else {
            return false;
        };

        entry.notebook_enabled = notebook_enabled;
        if notebook_enabled {
            entry.kb = None;
            entry.embedding = None;
        }
        drop(sessions);
        self.update_product_metadata(id, |metadata| {
            metadata.notebook_enabled = notebook_enabled;
            if notebook_enabled {
                metadata.kb = None;
                metadata.embedding = None;
            }
        });
        true
    }

    pub fn set_llm(&self, id: &str, llm: Option<LlmSessionConfig>) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(entry) = sessions.get_mut(id) else {
            return false;
        };

        entry.llm = llm;
        let llm = entry.llm.clone();
        drop(sessions);
        self.update_product_metadata(id, |metadata| {
            metadata.llm = llm;
        });
        true
    }

    pub fn set_search(&self, id: &str, search: Option<SearchSessionConfig>) -> bool {
        let mut sessions = self.sessions.lock().unwrap();
        let Some(entry) = sessions.get_mut(id) else {
            return false;
        };

        entry.search = search;
        let search = entry.search.clone();
        drop(sessions);
        self.update_product_metadata(id, |metadata| {
            metadata.search = search;
        });
        true
    }

    pub async fn rename(
        &self,
        id: &str,
        name: Option<String>,
    ) -> Result<(), llm_harness_types::SessionError> {
        let storage = self.repo.open(id).await?;
        storage.update_metadata_name(name).await
    }

    pub async fn delete(&self, id: &str) -> Result<(), llm_harness_types::SessionError> {
        self.repo.delete(id).await?;
        self.sessions.lock().unwrap().remove(id);
        {
            let mut metadata = self.product_metadata.lock().unwrap();
            metadata.remove(id);
            let _ = persist_product_metadata(&self.product_metadata_path, &metadata);
        }
        Ok(())
    }

    fn upsert_product_metadata(&self, id: &str, value: ProductSessionMetadata) {
        let mut metadata = self.product_metadata.lock().unwrap();
        metadata.insert(id.to_string(), value);
        let _ = persist_product_metadata(&self.product_metadata_path, &metadata);
    }

    fn update_product_metadata(&self, id: &str, update: impl FnOnce(&mut ProductSessionMetadata)) {
        let mut metadata = self.product_metadata.lock().unwrap();
        let entry = metadata
            .entry(id.to_string())
            .or_insert_with(|| ProductSessionMetadata {
                tutor_id: None,
                capability: "chat".into(),
                kb: None,
                notebook_enabled: false,
                llm: None,
                search: None,
                embedding: None,
            });
        update(entry);
        let _ = persist_product_metadata(&self.product_metadata_path, &metadata);
    }
}

async fn rewind_incomplete_tool_call(
    session: &Session,
    rewind_to: Option<EntryId>,
) -> Result<bool, SessionError> {
    let Some(entry_id) = rewind_to else {
        return Ok(false);
    };
    session.navigate_to(entry_id).await?;
    Ok(true)
}

fn tool_use_ids(content: &[ContentBlock]) -> Vec<String> {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolUse { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect()
}

pub fn message_text(message: &llm_harness_types::AgentMessage) -> String {
    let content = match message {
        llm_harness_types::AgentMessage::User(message) => &message.content,
        llm_harness_types::AgentMessage::Assistant(message) => &message.content,
        _ => return String::new(),
    };

    content
        .iter()
        .filter_map(|block| match block {
            llm_harness_types::ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn message_role(message: &llm_harness_types::AgentMessage) -> Option<&'static str> {
    match message {
        llm_harness_types::AgentMessage::User(_) => Some("user"),
        llm_harness_types::AgentMessage::Assistant(message)
            if message.kind == AssistantMessageKind::FinalAnswer =>
        {
            Some("assistant")
        }
        llm_harness_types::AgentMessage::Assistant(_) => None,
        _ => None,
    }
}

impl Default for SessionPool {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            active_runs: Mutex::new(HashMap::new()),
            product_metadata: Mutex::new(HashMap::new()),
            product_metadata_path: PathBuf::from(".llm-tutor/session-product-metadata.json"),
            repo: Arc::new(JsonlSessionRepo::new(".llm-tutor/sessions")),
        }
    }
}

fn read_product_metadata(
    path: &PathBuf,
) -> anyhow::Result<HashMap<String, ProductSessionMetadata>> {
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let text = std::fs::read_to_string(path)?;
    if text.trim().is_empty() {
        return Ok(HashMap::new());
    }
    Ok(serde_json::from_str(&text)?)
}

fn persist_product_metadata(
    path: &PathBuf,
    metadata: &HashMap<String, ProductSessionMetadata>,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(metadata)?)?;
    Ok(())
}

fn token_usage_from_runtime_trace(data: &serde_json::Value) -> Option<TokenUsage> {
    if data.get("kind")?.as_str()? != "runtime_usage" {
        return None;
    }
    let payload = data.get("payload")?;
    Some(TokenUsage {
        input_tokens: u32_from_json(payload.get("input_tokens")),
        output_tokens: u32_from_json(payload.get("output_tokens")),
        cache_read_tokens: u32_from_json(payload.get("cache_read_tokens")),
        cache_creation_tokens: u32_from_json(payload.get("cache_write_tokens")),
        reasoning_tokens: u32_from_json(payload.get("reasoning_tokens")),
    })
}

fn u32_from_json(value: Option<&serde_json::Value>) -> u32 {
    value
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pool() -> Arc<SessionPool> {
        SessionPool::new_with_root(
            std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4())),
        )
    }

    #[tokio::test]
    async fn session_pool_creates_and_retrieves() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let entry = pool.get(&id);
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.capability, "chat");
        assert_eq!(entry.id, id);
    }

    #[tokio::test]
    async fn runtime_session_persists_messages() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();

        session
            .append_message(tutor_agent::chat::user_message("hello"))
            .await
            .unwrap();

        let reopened = pool.open_runtime_session(&id).await.unwrap();
        let ctx = reopened.build_context().await.unwrap();
        assert_eq!(ctx.messages.len(), 1);
    }

    #[tokio::test]
    async fn repair_incomplete_tool_call_context_rewinds_active_cursor() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();
        session
            .append_message(tutor_agent::chat::user_message("hello"))
            .await
            .unwrap();
        let mut tool_call = tutor_agent::chat::assistant_message("");
        let AgentMessage::Assistant(assistant) = &mut tool_call else {
            panic!("expected assistant message");
        };
        assistant.content = vec![ContentBlock::ToolUse {
            id: "call_1".into(),
            name: "web_search".into(),
            input: serde_json::json!({"query": "rust"}),
        }];
        session.append_message(tool_call).await.unwrap();
        session
            .append_message(tutor_agent::chat::user_message("next turn"))
            .await
            .unwrap();

        assert!(pool.repair_incomplete_tool_call_context(&id).await.unwrap());
        let repaired = pool.open_runtime_session(&id).await.unwrap();
        let ctx = repaired.build_context().await.unwrap();

        assert_eq!(ctx.messages.len(), 1);
        assert_eq!(message_text(&ctx.messages[0]), "hello");
    }

    #[tokio::test]
    async fn latest_usage_uses_runtime_usage_trace() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();

        session
            .append_message(tutor_agent::chat::assistant_message(
                "answer without provider usage",
            ))
            .await
            .unwrap();
        pool.append_trace(
            &id,
            "runtime_usage",
            serde_json::json!({
                "capability": "chat",
                "input_tokens": 12,
                "output_tokens": 5,
                "cache_read_tokens": 3,
                "cache_write_tokens": 2,
                "cost_usd": 0.01,
            }),
        )
        .await
        .unwrap();

        let usage = pool.latest_usage(&id).await.unwrap().unwrap();
        assert_eq!(usage.input_tokens, 12);
        assert_eq!(usage.output_tokens, 5);
        assert_eq!(usage.cache_read_tokens, 3);
        assert_eq!(usage.cache_creation_tokens, 2);
        assert_eq!(usage.total_tokens(), 22);
    }

    #[test]
    fn progress_assistant_messages_are_not_chat_bubbles() {
        let mut message = tutor_agent::chat::assistant_message("thinking before a tool");
        let AgentMessage::Assistant(assistant) = &mut message else {
            panic!("expected assistant message");
        };
        assistant.kind = AssistantMessageKind::Progress;

        assert_eq!(message_role(&message), None);
        assert_eq!(message_text(&message), "thinking before a tool");
    }

    #[tokio::test]
    async fn fork_before_message_keeps_edited_branch_active() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();

        session
            .append_message(tutor_agent::chat::user_message("first"))
            .await
            .unwrap();
        session
            .append_message(tutor_agent::chat::assistant_message("first reply"))
            .await
            .unwrap();
        session
            .append_message(tutor_agent::chat::user_message("second"))
            .await
            .unwrap();
        session
            .append_message(tutor_agent::chat::assistant_message("old second reply"))
            .await
            .unwrap();

        let forked = pool
            .fork_before_message(&id, 2, Some("edit second".into()))
            .await
            .unwrap();
        assert!(forked);

        let session = pool.open_runtime_session(&id).await.unwrap();
        session
            .append_message(tutor_agent::chat::user_message("edited second"))
            .await
            .unwrap();
        session
            .append_message(tutor_agent::chat::assistant_message("new second reply"))
            .await
            .unwrap();

        let texts = pool
            .messages(&id)
            .await
            .unwrap()
            .iter()
            .map(message_text)
            .collect::<Vec<_>>();

        assert_eq!(
            texts,
            vec![
                "first".to_string(),
                "first reply".to_string(),
                "edited second".to_string(),
                "new second reply".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn runtime_session_persists_trace_entries() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();

        pool.append_trace(
            &id,
            "tool_call",
            serde_json::json!({ "tool": "knowledge_search", "capability": "chat" }),
        )
        .await
        .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let traces = reopened.traces(&id).await.unwrap();

        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].kind, "tool_call");
        assert_eq!(traces[0].payload["tool"], "knowledge_search");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn message_mentions_survive_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();

        pool.append_message_mentions(
            &id,
            1,
            vec![serde_json::json!({
                "id": "notebook_entry:note-1",
                "type": "notebook_entry",
                "target_id": "note-1",
                "title": "Note 1"
            })],
        )
        .await
        .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let mentions = reopened.message_mentions(&id).await.unwrap();
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].user_message_index, 1);
        assert_eq!(mentions[0].mentions[0]["title"], "Note 1");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn message_citations_survive_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();
        session
            .append_message(tutor_agent::chat::assistant_message("answer"))
            .await
            .unwrap();
        let assistant_count = pool.assistant_message_count(&id).await.unwrap();

        pool.append_message_citations(
            &id,
            assistant_count,
            vec![serde_json::json!({
                "index": 1,
                "source": "doc.pdf",
                "text": "quoted source",
                "kind": "rag",
            })],
        )
        .await
        .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let citations = reopened.message_citations(&id).await.unwrap();

        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].assistant_message_index, 1);
        assert_eq!(citations[0].citations[0]["source"], "doc.pdf");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn message_artifacts_survive_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("quiz", None, false, None, None, None)
            .await
            .unwrap();

        pool.append_message_artifacts(
            &id,
            1,
            vec![serde_json::json!({
                "type": "quiz_session",
                "quiz_id": "quiz-123",
            })],
        )
        .await
        .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let artifacts = reopened.message_artifacts(&id).await.unwrap();

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].assistant_message_index, 1);
        assert_eq!(artifacts[0].artifacts[0]["type"], "quiz_session");
        assert_eq!(artifacts[0].artifacts[0]["quiz_id"], "quiz-123");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn active_run_blocks_duplicates_until_finished() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("research", None, false, None, None, None)
            .await
            .unwrap();

        let (run_id, cancel) = pool.try_start_active_run(&id, "research").unwrap();
        assert!(pool.try_start_active_run(&id, "research").is_none());

        let active = pool.active_run(&id).unwrap();
        assert_eq!(active.run_id, run_id);
        assert_eq!(active.status, "running");

        let cancelling = pool.cancel_active_run(&id).unwrap();
        assert_eq!(cancelling.status, "cancelling");
        assert!(cancel.is_cancelled());
        assert!(pool.try_start_active_run(&id, "research").is_none());

        pool.finish_active_run(&id, &run_id);
        assert!(pool.active_run(&id).is_none());
        assert!(pool.try_start_active_run(&id, "research").is_some());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn persisted_running_run_recovers_as_interrupted_after_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("research", None, false, None, None, None)
            .await
            .unwrap();
        let (run_id, _) = pool.try_start_active_run(&id, "research").unwrap();
        let running = pool
            .update_active_run_stage(&id, &run_id, "read_sources")
            .unwrap();
        pool.append_run_state(&id, &running).await.unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let recovered = reopened.recovered_run_state(&id).await.unwrap().unwrap();

        assert_eq!(recovered.run_id, run_id);
        assert_eq!(recovered.status, "interrupted");
        assert_eq!(recovered.current_stage.as_deref(), Some("read_sources"));
        assert!(reopened.active_run(&id).is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn persisted_terminal_run_remains_terminal_after_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("deep_solve", None, false, None, None, None)
            .await
            .unwrap();
        let (run_id, _) = pool.try_start_active_run(&id, "deep_solve").unwrap();
        let completed = pool.terminal_active_run(&id, &run_id, "completed").unwrap();
        pool.append_run_state(&id, &completed).await.unwrap();
        pool.finish_active_run(&id, &run_id);

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let recovered = reopened.recovered_run_state(&id).await.unwrap().unwrap();

        assert_eq!(recovered.status, "completed");
        assert_eq!(recovered.run_id, run_id);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn deep_solve_trace_sequence_survives_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("deep_solve", None, false, None, None, None)
            .await
            .unwrap();

        let sequence = [
            (
                "deep_solve_stage_start",
                serde_json::json!({
                    "capability": "deep_solve",
                    "stage": "plan",
                    "title": "Create solve plan",
                }),
            ),
            (
                "deep_solve_plan",
                serde_json::json!({
                    "capability": "deep_solve",
                    "stage": "plan",
                    "analysis": "use arithmetic",
                    "steps": [{ "id": "s1", "goal": "compute 2 plus 2" }],
                }),
            ),
            (
                "deep_solve_step_done",
                serde_json::json!({
                    "capability": "deep_solve",
                    "stage": "solve",
                    "step_id": "s1",
                    "summary": "the answer is 4",
                }),
            ),
            (
                "deep_solve_final",
                serde_json::json!({
                    "capability": "deep_solve",
                    "stage": "synthesize",
                    "summary": "The final answer is 4.",
                }),
            ),
        ];

        for (kind, payload) in sequence {
            pool.append_trace(&id, kind, payload).await.unwrap();
        }

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let traces = reopened.traces(&id).await.unwrap();

        let kinds = traces
            .iter()
            .map(|trace| trace.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "deep_solve_stage_start",
                "deep_solve_plan",
                "deep_solve_step_done",
                "deep_solve_final",
            ]
        );
        assert_eq!(traces[1].payload["steps"][0]["id"], "s1");
        assert_eq!(traces[2].payload["step_id"], "s1");
        assert_eq!(traces[3].payload["stage"], "synthesize");
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn compact_summary_survives_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();
        let first_entry = session
            .append_message(tutor_agent::chat::user_message("what is lithography?"))
            .await
            .unwrap();
        session
            .append_message(tutor_agent::chat::assistant_message(
                "Lithography transfers patterns.",
            ))
            .await
            .unwrap();
        session
            .append(SessionEntryPayload::Compaction(
                llm_harness_agent::session::CompactionEntry {
                    summary_message: AgentMessage::CompactionSummary(
                        llm_harness_types::CompactionSummaryMessage {
                            summary: "lithography means pattern transfer".into(),
                            timestamp: chrono::Utc::now(),
                        },
                    ),
                    first_kept_entry: first_entry,
                    tokens_before: 42,
                    from_hook: false,
                    details: None,
                },
            ))
            .await
            .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let restored = reopened.compact_summary(&id).await.unwrap().unwrap();

        assert!(restored.summary.contains("lithography"));
        assert_eq!(restored.message_count, 0);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn session_pool_updates_capability_without_replacing_runtime_session() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        assert!(pool.set_capability(&id, "code_exec"));

        let updated = pool.get(&id).unwrap();
        assert_eq!(updated.capability, "code_exec");
        assert!(pool.open_runtime_session(&id).await.is_ok());
    }

    #[tokio::test]
    async fn session_pool_updates_knowledge_binding() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        assert!(pool.set_knowledge(
            &id,
            Some("kb-1".into()),
            Some(tutor_rag::EmbeddingConfig {
                provider: "openai".into(),
                model: "text-embedding-3-small".into(),
                api_key: "sk-test".into(),
                base_url: None,
                embeddings_path: None,
                dimensions: Some(1536),
                send_dimensions: false,
            }),
        ));

        let updated = pool.get(&id).unwrap();
        assert_eq!(updated.kb.as_deref(), Some("kb-1"));
        assert_eq!(
            updated
                .embedding
                .as_ref()
                .map(|config| config.model.as_str()),
            Some("text-embedding-3-small")
        );
    }

    #[tokio::test]
    async fn session_pool_updates_notebook_binding_and_clears_knowledge() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        assert!(pool.set_knowledge(
            &id,
            Some("kb-1".into()),
            Some(tutor_rag::EmbeddingConfig {
                provider: "openai".into(),
                model: "text-embedding-3-small".into(),
                api_key: "sk-test".into(),
                base_url: None,
                embeddings_path: None,
                dimensions: Some(1536),
                send_dimensions: false,
            }),
        ));
        assert!(pool.set_notebook_enabled(&id, true));

        let updated = pool.get(&id).unwrap();
        assert!(updated.notebook_enabled);
        assert!(updated.kb.is_none());
        assert!(updated.embedding.is_none());
    }

    #[tokio::test]
    async fn product_metadata_survives_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create(
                "chat",
                Some("kb-1".into()),
                false,
                Some(LlmSessionConfig {
                    provider: "deepseek".into(),
                    model: "deepseek-v4-flash".into(),
                    api_key: Some("sk-test".into()),
                    base_url: Some("https://api.deepseek.com".into()),
                    chat_path: Some("/chat/completions".into()),
                    context_window_tokens: Some(128_000),
                    budget_limit_usd: Some(2.0),
                    require_approval: false,
                }),
                None,
                Some(tutor_rag::EmbeddingConfig {
                    provider: "openai".into(),
                    model: "text-embedding-3-small".into(),
                    api_key: "sk-embed".into(),
                    base_url: None,
                    embeddings_path: None,
                    dimensions: Some(1536),
                    send_dimensions: false,
                }),
            )
            .await
            .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let entry = reopened.ensure_entry(&id).await.unwrap();

        assert_eq!(entry.kb.as_deref(), Some("kb-1"));
        assert_eq!(
            entry.llm.as_ref().map(|config| config.provider.as_str()),
            Some("deepseek")
        );
        assert_eq!(
            entry.embedding.as_ref().map(|config| config.model.as_str()),
            Some("text-embedding-3-small")
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn notebook_binding_survives_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("organize", None, true, None, None, None)
            .await
            .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let entry = reopened.ensure_entry(&id).await.unwrap();

        assert_eq!(entry.capability, "organize");
        assert!(entry.notebook_enabled);
        assert!(entry.kb.is_none());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn tutor_binding_survives_pool_reopen() {
        let root = std::env::temp_dir().join(format!("llm-tutor-test-{}", uuid::Uuid::new_v4()));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create_with_tutor(
                Some("general-tutor".into()),
                SessionCreateConfig {
                    capability: "chat".into(),
                    kb: None,
                    notebook_enabled: false,
                    llm: None,
                    search: None,
                    embedding: None,
                },
            )
            .await
            .unwrap();

        drop(pool);
        let reopened = SessionPool::new_with_root(&root);
        let entry = reopened.ensure_entry(&id).await.unwrap();

        assert_eq!(entry.tutor_id.as_deref(), Some("general-tutor"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn list_returns_runtime_sessions() {
        let pool = test_pool();
        pool.create("chat", None, false, None, None, None)
            .await
            .unwrap();

        let sessions = pool.list(Some(10)).await.unwrap();
        assert_eq!(sessions.len(), 1);
    }

    #[tokio::test]
    async fn rename_updates_runtime_metadata() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();

        pool.rename(&id, Some("Algebra review".into()))
            .await
            .unwrap();

        let meta = pool.metadata(&id).await.unwrap();
        assert_eq!(meta.name.as_deref(), Some("Algebra review"));
    }

    #[tokio::test]
    async fn delete_removes_runtime_session() {
        let pool = test_pool();
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();

        pool.delete(&id).await.unwrap();

        assert!(pool.get(&id).is_none());
        assert!(pool.open_runtime_session(&id).await.is_err());
    }

    #[test]
    fn get_nonexistent_returns_none() {
        let pool = test_pool();
        assert!(pool.get("nonexistent-id").is_none());
    }
}
