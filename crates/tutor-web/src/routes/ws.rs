use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    Router,
    extract::ws::{Message, WebSocket},
    extract::{Path, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt, future::BoxFuture};
use llm_harness_runtime_audit_jsonl::JsonlAuditSink;
use llm_harness_runtime_knowledge::{
    EvidenceAuthority, KnowledgeAccessContext, KnowledgeScope, PrincipalRef,
};
use llm_harness_runtime_sandbox_os::OsEnv;
use llm_harness_types::RunRequest;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;
use tutor_agent::event_sink::{EventSink, SharedEventSink};
use tutor_agent::governance::GovernanceConfig;
use tutor_agent::{Capability, CapabilityRouter, LlmConfig, LlmProviderKind};

use crate::knowledge_store::KnowledgeStore;
use crate::memory_store::{MemoryEventCategory, MemoryStore};
use crate::notebook_store::NotebookStore;
use crate::quiz_store::QuizStore;
use crate::quiz_tool::{CreateQuizTool, ProposeQuizPlanTool};
use crate::research_tool::{CreateResearchReportTool, ProposeResearchPlanTool};
use crate::routes::quiz::{CreateLlmConfig, QuizState};
use crate::routes::space::{SpaceMention, resolve_space_mention_markdown};
use crate::session::{
    ActiveRunSummary, LlmSessionConfig, SearchSessionConfig, SessionEntry, SessionPool,
};
use crate::space_tool::{
    ListNotebookTreeTool, ProposeNotebookEditTool, ReadSpaceItemTool, SearchNotebookTool,
};
use crate::stream::StreamEvent;
use crate::tutor_memory_store::{TutorMemoryEntry, TutorMemoryKind, TutorMemoryStore};
use crate::tutor_memory_tool::{ReadTutorMemoryTool, RememberForLaterTool, ResolveTutorMemoryTool};
use crate::tutor_store::{TutorProfile, TutorStore};

#[derive(Clone)]
struct WsState {
    pool: Arc<SessionPool>,
    knowledge: Arc<KnowledgeStore>,
    memory: Arc<MemoryStore>,
    notebook: Arc<NotebookStore>,
    quizzes: Arc<QuizStore>,
    tutors: Arc<TutorStore>,
    tutor_memory: Arc<TutorMemoryStore>,
    evidence_authority: Arc<EvidenceAuthority>,
    rag_root: PathBuf,
}

#[derive(Clone)]
pub struct TutorRuntimeStores {
    profiles: Arc<TutorStore>,
    memory: Arc<TutorMemoryStore>,
}

impl TutorRuntimeStores {
    pub fn new(profiles: Arc<TutorStore>, memory: Arc<TutorMemoryStore>) -> Self {
        Self { profiles, memory }
    }
}

#[derive(Clone)]
struct PersistedEventSink {
    pool: Arc<SessionPool>,
    session_id: String,
    stream: crate::stream::TutorStream,
    streamed_content: Arc<AtomicBool>,
    research_report_started: Arc<AtomicBool>,
    run_id: String,
    tutor_id: Option<String>,
    pending_events: Arc<Mutex<Vec<PendingSessionEvent>>>,
}

struct PendingSessionEvent {
    kind: String,
    data: serde_json::Value,
    run_state: Option<ActiveRunSummary>,
    artifact: Option<serde_json::Value>,
}

impl EventSink for PersistedEventSink {
    fn trace(&self, kind: String, mut data: serde_json::Value) -> BoxFuture<'static, ()> {
        if trace_invokes_research_report(&kind, &data) {
            self.research_report_started.store(true, Ordering::SeqCst);
        }
        let pool = self.pool.clone();
        let session_id = self.session_id.clone();
        let stream = self.stream.clone();
        let run_id = self.run_id.clone();
        let tutor_id = self.tutor_id.clone();
        let pending_events = self.pending_events.clone();
        Box::pin(async move {
            if let Some(map) = data.as_object_mut() {
                map.insert("run_id".into(), serde_json::Value::String(run_id.clone()));
                if let Some(tutor_id) = tutor_id {
                    map.insert("tutor_id".into(), serde_json::Value::String(tutor_id));
                }
            }
            let run_state = run_stage_from_trace(&kind, &data)
                .and_then(|stage| pool.update_active_run_stage(&session_id, &run_id, &stage));
            let artifact = (kind == "tool_result")
                .then(|| message_artifact_from_tool_result(&data, &run_id))
                .flatten();
            pending_events.lock().unwrap().push(PendingSessionEvent {
                kind: kind.clone(),
                data: data.clone(),
                run_state,
                artifact,
            });
            stream.trace(&kind, data).await;
        })
    }

    fn content(&self, text: String, chunk: bool) -> BoxFuture<'static, ()> {
        let stream = self.stream.clone();
        let streamed_content = self.streamed_content.clone();
        Box::pin(async move {
            if chunk {
                streamed_content.store(true, Ordering::SeqCst);
            }
            stream.content(&text, chunk).await;
        })
    }

    fn progress_content(&self, text: String, chunk: bool) -> BoxFuture<'static, ()> {
        let stream = self.stream.clone();
        Box::pin(async move {
            stream.progress_content(&text, chunk).await;
        })
    }
}

async fn flush_pending_session_events(
    pool: &SessionPool,
    session_id: &str,
    assistant_message_index: usize,
    pending_events: &Mutex<Vec<PendingSessionEvent>>,
) -> Result<(), llm_harness_types::SessionError> {
    let events = std::mem::take(&mut *pending_events.lock().unwrap());
    for event in events {
        if let Some(run) = event.run_state {
            pool.append_run_state(session_id, &run).await?;
        }
        if let Some(artifact) = event.artifact {
            pool.append_message_artifacts(session_id, assistant_message_index, vec![artifact])
                .await?;
        }
        pool.append_trace(session_id, &event.kind, event.data)
            .await?;
    }
    Ok(())
}

fn trace_invokes_research_report(kind: &str, data: &serde_json::Value) -> bool {
    matches!(kind, "tool_call" | "tool_result")
        && data.get("tool").and_then(serde_json::Value::as_str) == Some("create_research_report")
}

fn run_stage_from_trace(kind: &str, data: &serde_json::Value) -> Option<String> {
    if let Some(stage) = data.get("stage").and_then(|value| value.as_str()) {
        return Some(stage.to_string());
    }
    match kind {
        "research_search" => Some("search".into()),
        "research_read" => Some("read_sources".into()),
        "research_report_done" => Some("report_complete".into()),
        "deep_solve_stage_start" => data
            .get("stage_id")
            .or_else(|| data.get("step_id"))
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        _ => None,
    }
}

fn message_artifact_from_tool_result(
    data: &serde_json::Value,
    run_id: &str,
) -> Option<serde_json::Value> {
    if let Some(artifact) = research_artifact_from_tool_result(data, run_id) {
        return Some(artifact);
    }
    quiz_artifact_from_tool_result(data)
}

fn quiz_artifact_from_tool_result(data: &serde_json::Value) -> Option<serde_json::Value> {
    if data.get("tool")?.as_str()? != "create_quiz" {
        return None;
    }
    if data.get("ok").and_then(|value| value.as_bool()) == Some(false) {
        return None;
    }
    let details = data.get("details")?.as_object()?;
    let quiz = details.get("quiz")?.as_object()?;
    let quiz_id = quiz.get("id")?.as_str()?;
    Some(serde_json::json!({
        "type": "quiz_session",
        "quiz_id": quiz_id,
    }))
}

fn research_artifact_from_tool_result(
    data: &serde_json::Value,
    run_id: &str,
) -> Option<serde_json::Value> {
    if data.get("tool")?.as_str()? != "create_research_report" {
        return None;
    }
    if data.get("ok").and_then(|value| value.as_bool()) == Some(false) {
        return None;
    }
    let details = data.get("details")?.as_object()?;
    let title = details.get("title")?.as_str()?.trim();
    let markdown = details.get("markdown")?.as_str()?.trim();
    if title.is_empty() || markdown.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "type": "research_report",
        "artifact_store": "runtime_trace",
        "artifact_id": run_id,
        "title": title,
    }))
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ClientMessage {
    #[serde(rename = "message")]
    Message {
        content: String,
        mentions: Option<Vec<SpaceMention>>,
    },
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "approval_response")]
    ApprovalResponse { request_id: String, approved: bool },
}

struct TutorMessageInput {
    entry: SessionEntry,
    content: String,
    mentions: Vec<SpaceMention>,
    run_id: String,
    cancel: CancellationToken,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<WsState>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id))
}

async fn handle_socket(socket: WebSocket, state: WsState, session_id: String) {
    let pool = state.pool.clone();
    let entry = match pool.ensure_entry(&session_id).await {
        Some(e) => e,
        None => return,
    };
    let (mut event_rx, snapshot) = entry.stream.subscribe_with_snapshot();

    let (mut ws_sink, mut ws_stream) = socket.split();
    let active_run = pool.active_run(&session_id);
    let mut initial_events = Vec::new();
    let should_acknowledge_completed = snapshot.completed;
    let snapshot_generation = snapshot.generation;
    if snapshot.completed {
        // The durable runtime history is authoritative once the turn has
        // settled. Asking the client to rehydrate also restores rich message
        // attachments that are not represented by the text-only snapshot.
        initial_events.push(StreamEvent::Status {
            kind: "history_sync".into(),
            data: serde_json::json!({}),
        });
    } else if let Some(run) = active_run {
        initial_events.push(StreamEvent::Status {
            kind: "running".into(),
            data: serde_json::json!({
                    "capability": run.capability,
                    "run_id": run.run_id,
                    "status": run.status,
                    "current_stage": run.current_stage,
                    "rejoined": true,
                    "started_at": run.started_at,
                    "updated_at": run.updated_at,
            }),
        });
        if !snapshot.content.is_empty() {
            initial_events.push(StreamEvent::Content {
                text: snapshot.content,
                chunk: true,
            });
        }
        if !snapshot.progress_content.is_empty() {
            initial_events.push(StreamEvent::ProgressContent {
                text: snapshot.progress_content,
                chunk: false,
            });
        }
    }
    for event in initial_events {
        let Ok(json) = serde_json::to_string(&event) else {
            continue;
        };
        if ws_sink.send(Message::Text(json.into())).await.is_err() {
            return;
        }
    }
    if should_acknowledge_completed {
        entry.stream.acknowledge_completed(snapshot_generation);
    }

    // Forward events from the agent harness to the WebSocket client
    let send_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(_) => continue,
            };
            if ws_sink.send(Message::Text(json.into())).await.is_err() {
                break;
            }
        }
    });

    while let Some(Ok(msg)) = ws_stream.next().await {
        match msg {
            Message::Text(text) => {
                let parsed = serde_json::from_str::<ClientMessage>(&text);
                match parsed {
                    Ok(ClientMessage::Message { content, mentions }) => {
                        let Some((run_id, cancel)) =
                            pool.try_start_active_run(&session_id, &entry.capability)
                        else {
                            let _ = entry
                                .stream
                                .status(
                                    "error",
                                    serde_json::json!({
                                        "message": "agent is already running"
                                    }),
                                )
                                .await;
                            continue;
                        };
                        entry.stream.begin_run();
                        if let Some(run) = pool.active_run(&session_id) {
                            let _ = pool.append_run_state(&session_id, &run).await;
                        }
                        let active_entry = pool
                            .ensure_entry(&session_id)
                            .await
                            .unwrap_or_else(|| entry.clone());
                        let run_pool = pool.clone();
                        let run_session_id = session_id.clone();
                        let run_state = state.clone();
                        tokio::spawn(async move {
                            let terminal_status = run_tutor_message(
                                run_state,
                                TutorMessageInput {
                                    entry: active_entry,
                                    content,
                                    mentions: mentions.unwrap_or_default(),
                                    run_id: run_id.clone(),
                                    cancel,
                                },
                            )
                            .await;
                            if let Some(run) = run_pool.terminal_active_run(
                                &run_session_id,
                                &run_id,
                                terminal_status,
                            ) {
                                let _ = run_pool.append_run_state(&run_session_id, &run).await;
                            }
                            run_pool.finish_active_run(&run_session_id, &run_id);
                        });
                    }
                    Ok(ClientMessage::Stop) => {
                        if let Some(run) = pool.cancel_active_run(&session_id) {
                            let _ = entry
                                .stream
                                .status(
                                    "stopping",
                                    serde_json::json!({
                                        "capability": run.capability,
                                        "run_id": run.run_id,
                                    }),
                                )
                                .await;
                        }
                    }
                    Ok(ClientMessage::ApprovalResponse {
                        request_id,
                        approved,
                    }) => {
                        let _ = entry
                            .stream
                            .status(
                                "approval_response_received",
                                serde_json::json!({
                                    "request_id": request_id,
                                    "approved": approved,
                                }),
                            )
                            .await;
                    }
                    Err(err) => {
                        let _ = entry
                            .stream
                            .status(
                                "error",
                                serde_json::json!({
                                    "message": format!("invalid websocket message: {err}"),
                                }),
                            )
                            .await;
                    }
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    send_task.abort();
}

fn course_knowledge_access_context(
    session_id: &str,
    knowledge_base_id: &str,
    tutor: Option<&TutorProfile>,
) -> KnowledgeAccessContext {
    let mut scope = KnowledgeScope::new(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE);
    scope.project = Some(session_id.to_string());
    scope.attributes.insert(
        tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(),
        knowledge_base_id.to_string(),
    );
    let mut access =
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "local_user"));
    access.authorization_version = Some(match tutor {
        Some(tutor) => {
            let mut knowledge_base_ids = tutor.resource_permissions.knowledge_base_ids.clone();
            knowledge_base_ids.sort();
            format!(
                "tutor:{}:course-knowledge:{}",
                tutor.id,
                knowledge_base_ids.join(",")
            )
        }
        None => "local-user:course-knowledge:v1".into(),
    });
    access
}

pub fn ws_router(
    pool: Arc<SessionPool>,
    knowledge: Arc<KnowledgeStore>,
    memory: Arc<MemoryStore>,
    notebook: Arc<NotebookStore>,
    quizzes: Arc<QuizStore>,
    tutor_runtime: TutorRuntimeStores,
    rag_root: impl Into<PathBuf>,
) -> Router {
    let state = WsState {
        pool,
        knowledge,
        memory,
        notebook,
        quizzes,
        tutors: tutor_runtime.profiles,
        tutor_memory: tutor_runtime.memory,
        evidence_authority: crate::knowledge_runtime::course_evidence_authority(),
        rag_root: rag_root.into(),
    };
    Router::new()
        .route("/ws/sessions/{session_id}", get(ws_handler))
        .with_state(state)
}

async fn run_tutor_message(state: WsState, input: TutorMessageInput) -> &'static str {
    let WsState {
        pool,
        knowledge,
        memory,
        notebook,
        quizzes,
        tutors,
        tutor_memory,
        evidence_authority,
        rag_root,
    } = state;
    let TutorMessageInput {
        entry,
        content,
        mentions,
        run_id,
        cancel,
    } = input;
    let history_len = pool.history_len(&entry.id).await + 1;
    let bound_tutor = match entry.tutor_id.as_deref() {
        Some(tutor_id) => match tutors.get_available(tutor_id) {
            Some(tutor) => Some(tutor),
            None => {
                let message = "bound tutor is unavailable";
                let _ = entry
                    .stream
                    .status("error", serde_json::json!({ "message": message }))
                    .await;
                return "error";
            }
        },
        None => None,
    };
    let user_message_index = next_user_message_index(&pool, &entry.id).await;
    if !mentions.is_empty() {
        let _ = pool
            .append_message_mentions(
                &entry.id,
                user_message_index,
                mentions
                    .iter()
                    .map(|mention| serde_json::to_value(mention).unwrap_or_default())
                    .collect(),
            )
            .await;
    }
    let _ = entry
        .stream
        .status(
            "running",
            serde_json::json!({
                "capability": entry.capability,
                "run_id": run_id,
                "history_len": history_len,
                "tutor_id": entry.tutor_id.clone(),
            }),
        )
        .await;

    let research_report_started = Arc::new(AtomicBool::new(false));
    let pending_events = Arc::new(Mutex::new(Vec::new()));
    let assistant_message_index = pool.assistant_message_count(&entry.id).await.unwrap_or(0) + 1;
    let work = async {
        let capability: Capability = entry.capability.parse()?;
        let repaired_context = pool
            .repair_incomplete_tool_call_context(&entry.id)
            .await
            .map_err(|err| tutor_agent::TutorError::Internal(err.to_string()))?;
        if repaired_context {
            let _ = entry
                .stream
                .status(
                    "context_repaired",
                    serde_json::json!({
                        "reason": "incomplete_tool_call",
                    }),
                )
                .await;
        }
        let runtime_session = pool
            .open_runtime_session(&entry.id)
            .await
            .map_err(|err| tutor_agent::TutorError::Internal(err.to_string()))?;
        let llm = llm_config_for_session(entry.llm.clone())?;
        let cwd = std::env::current_dir()
            .map_err(|err| tutor_agent::TutorError::Internal(err.to_string()))?;
        let env = Arc::new(OsEnv::new(cwd));
        let budget_limit = entry
            .llm
            .as_ref()
            .and_then(|config| config.budget_limit_usd)
            .unwrap_or(2.0);
        let audit_path = std::env::temp_dir().join(format!("tutor_web_{}.jsonl", entry.id));
        let audit = Arc::new(JsonlAuditSink::new(&audit_path));
        let require_approval = entry
            .llm
            .as_ref()
            .map(|config| config.require_approval)
            .unwrap_or(false);
        let governance = GovernanceConfig::new(budget_limit, Some(audit), require_approval);
        let streamed_content = Arc::new(AtomicBool::new(false));
        let sink: SharedEventSink = Arc::new(PersistedEventSink {
            pool: pool.clone(),
            session_id: entry.id.clone(),
            stream: entry.stream.clone(),
            streamed_content: streamed_content.clone(),
            research_report_started: research_report_started.clone(),
            run_id: run_id.clone(),
            tutor_id: entry.tutor_id.clone(),
            pending_events: pending_events.clone(),
        });
        let mut router = CapabilityRouter::new(env, llm, governance)
            .with_event_sink(sink)
            .with_workflow_root(rag_root.join("workflow-sessions"))
            .with_memory_root(memory.root_path().to_path_buf());
        let learner_memory_allowed = bound_tutor
            .as_ref()
            .is_none_or(|tutor| tutor.learner_memory_access);
        let notebook_allowed = bound_tutor
            .as_ref()
            .is_none_or(|tutor| tutor.resource_permissions.notebook);
        let space_allowed = bound_tutor
            .as_ref()
            .is_none_or(|tutor| tutor.resource_permissions.space);
        router = router.with_learner_memory_access(learner_memory_allowed);
        if space_allowed {
            router = router.with_product_tool(Arc::new(ReadSpaceItemTool::new(
                notebook.clone(),
                quizzes.clone(),
            )));
        }
        if let Some(tutor) = bound_tutor.as_ref() {
            if !tutor
                .allowed_capabilities
                .iter()
                .any(|allowed| allowed == &entry.capability)
            {
                return Err(tutor_agent::TutorError::UnsupportedCapability(
                    entry.capability.clone(),
                ));
            }
            let active_memory = tutor_memory
                .list(&tutor.id, false)
                .map_err(|error| tutor_agent::TutorError::Internal(error.to_string()))?;
            router =
                router.with_product_instruction(tutor_product_instruction(tutor, &active_memory));
            router = router.with_product_tool(Arc::new(ReadTutorMemoryTool::new(
                tutor_memory.clone(),
                tutor.id.clone(),
            )));
            if tutor.autonomous_memory {
                router = router
                    .with_product_tool(Arc::new(RememberForLaterTool::new(
                        tutor_memory.clone(),
                        tutor.id.clone(),
                        entry.id.clone(),
                    )))
                    .with_product_tool(Arc::new(ResolveTutorMemoryTool::new(
                        tutor_memory.clone(),
                        tutor.id.clone(),
                    )));
            }
        }
        if entry.capability == "quiz" {
            let quiz_tool = CreateQuizTool::new(
                QuizState {
                    store: quizzes.clone(),
                    knowledge: knowledge.clone(),
                    notebook: notebook.clone(),
                    memory: memory.clone(),
                    evidence_authority: evidence_authority.clone(),
                    rag_root: rag_root.clone(),
                    workflow_root: rag_root.join("workflow-sessions").join("quiz"),
                },
                entry.kb.clone(),
                create_quiz_llm_config_for_session(entry.llm.clone()),
            );
            let quiz_tool = match bound_tutor.as_ref() {
                Some(tutor) => quiz_tool.with_resource_policy(
                    tutor.resource_permissions.knowledge_base_ids.clone(),
                    tutor.resource_permissions.notebook,
                ),
                None => quiz_tool,
            };
            router = router
                .with_product_tool(Arc::new(ProposeQuizPlanTool))
                .with_product_tool(Arc::new(quiz_tool));
        }
        if entry.notebook_enabled && notebook_allowed {
            router = router
                .with_product_tool(Arc::new(ListNotebookTreeTool::new(notebook.clone())))
                .with_product_tool(Arc::new(SearchNotebookTool::new(notebook.clone())));
        }
        if entry.capability == "organize" && notebook_allowed {
            router =
                router.with_product_tool(Arc::new(ProposeNotebookEditTool::new(notebook.clone())));
        }
        if let Some(search) = web_search_config_for_session(entry.search.clone()) {
            router = router.with_web_search(search);
        }
        let knowledge_allowed = entry.kb.as_ref().is_none_or(|kb| {
            bound_tutor.as_ref().is_none_or(|tutor| {
                tutor
                    .resource_permissions
                    .knowledge_base_ids
                    .iter()
                    .any(|allowed| allowed == kb)
            })
        });
        if !knowledge_allowed {
            return Err(tutor_agent::TutorError::Internal(
                "bound tutor no longer has access to this Knowledge Base".into(),
            ));
        }
        if entry.notebook_enabled && !notebook_allowed {
            return Err(tutor_agent::TutorError::Internal(
                "bound tutor no longer has Notebook access".into(),
            ));
        }
        if !mentions.is_empty() && !space_allowed {
            return Err(tutor_agent::TutorError::Internal(
                "bound tutor does not have Space access".into(),
            ));
        }
        if let Some(embedding) = entry.embedding.clone() {
            let rag = tutor_rag::LanceDbRag::new(rag_root, embedding);
            if let Some(kb) = entry.kb.as_deref() {
                let knowledge_runtime = tutor_agent::assemble_course_knowledge(
                    tutor_rag::LanceDbKnowledgeSource::new(rag, kb),
                    evidence_authority.clone(),
                )?;
                router = router.with_knowledge_runtime(knowledge_runtime);
            }
        }
        if entry.capability == "research" {
            let workflow_router = router.clone();
            router = router
                .with_product_tool(Arc::new(ProposeResearchPlanTool))
                .with_product_tool(Arc::new(CreateResearchReportTool::new(workflow_router)));
        }
        let resolved_content =
            resolve_message_content_with_space_mentions(&notebook, &quizzes, &content, &mentions);
        if !mentions.is_empty() {
            let _ = entry
                .stream
                .status(
                    "space_context",
                    serde_json::json!({
                        "count": mentions.len(),
                        "resolved": resolved_content.resolved_count,
                    }),
                )
                .await;
        }
        let request = match entry.kb.as_deref() {
            Some(kb) => RunRequest::from_text(resolved_content.content).with_extension(
                course_knowledge_access_context(&entry.id, kb, bound_tutor.as_ref()),
            ),
            None => RunRequest::from_text(resolved_content.content),
        };
        let answer = router
            .run_request_with_session_cancel(
                capability,
                runtime_session,
                request,
                Some(cancel.clone()),
            )
            .await?;
        Ok((answer, streamed_content.load(Ordering::SeqCst)))
    };

    let result: tutor_agent::Result<(String, bool)> = work.await;
    let _ = flush_pending_session_events(
        &pool,
        &entry.id,
        assistant_message_index,
        pending_events.as_ref(),
    )
    .await;

    if cancel.is_cancelled() {
        let _ = entry
            .stream
            .status(
                "stopped",
                serde_json::json!({
                    "capability": entry.capability,
                }),
            )
            .await;
        let _ = entry.stream.content("", false).await;
        return "cancelled";
    }

    match result {
        Ok((answer, streamed)) => {
            if should_record_chat_memory(
                &entry.capability,
                research_report_started.load(Ordering::SeqCst),
            ) {
                let _ = memory.record_event(
                    MemoryEventCategory::Chat,
                    "answered",
                    summarize_exchange(&content, &answer),
                    Some(entry.id.clone()),
                    serde_json::json!({
                        "session_id": entry.id,
                        "capability": entry.capability,
                    "user": content,
                    "space_mentions": mentions.iter().map(|mention| serde_json::json!({
                        "id": mention.id,
                        "type": mention.mention_type,
                        "target_id": mention.target_id,
                        "question_id": mention.question_id,
                        "title": mention.title,
                    })).collect::<Vec<_>>(),
                        "assistant": answer,
                    }),
                );
            }
            let final_text = if streamed { "" } else { &answer };
            let _ = entry.stream.content(final_text, false).await;
            let history_len = pool.history_len(&entry.id).await;
            let latest_usage = pool.latest_usage(&entry.id).await.ok().flatten();
            let context_window_tokens = entry
                .llm
                .as_ref()
                .and_then(|config| config.context_window_tokens)
                .unwrap_or(200_000);
            let _ = entry
                .stream
                .status(
                    "done",
                    serde_json::json!({
                        "capability": entry.capability,
                        "history_len": history_len,
                        "context_window_tokens": context_window_tokens,
                        "usage": latest_usage.map(|usage| serde_json::json!({
                            "input_tokens": usage.input_tokens,
                            "output_tokens": usage.output_tokens,
                            "cache_read_tokens": usage.cache_read_tokens,
                            "cache_creation_tokens": usage.cache_creation_tokens,
                            "total_tokens": usage.total_tokens(),
                            "source": "provider",
                        })),
                    }),
                )
                .await;
            "completed"
        }
        Err(err) => {
            let _ = entry
                .stream
                .status(
                    "error",
                    serde_json::json!({
                        "message": err.to_string(),
                    }),
                )
                .await;
            let _ = entry.stream.content(&format!("Error: {err}"), false).await;
            "failed"
        }
    }
}

fn tutor_product_instruction(tutor: &TutorProfile, active_memory: &[TutorMemoryEntry]) -> String {
    let mut instruction = format!(
        "Tutor name: {}\n\n## Tutor Soul (user-authored Markdown)\n\n{}",
        tutor.name.trim(),
        tutor.soul_markdown.trim()
    );
    let memory_lines = active_memory
        .iter()
        .take(8)
        .map(|entry| {
            let mut line = format!(
                "- [{}:{}] {}",
                tutor_memory_kind_name(entry.kind),
                entry.id,
                bounded_text(&entry.text, 320)
            );
            if let Some(next_action) = entry.next_action.as_deref() {
                line.push_str("; next: ");
                line.push_str(&bounded_text(next_action, 180));
            }
            line
        })
        .collect::<Vec<_>>();
    if !memory_lines.is_empty() {
        instruction.push_str(
            "\n\n## Active private tutor continuity\n\nThese items belong only to this tutor. Apply them naturally and use read_tutor_memory for more detail. Do not treat them as general learner-profile facts.\n",
        );
        instruction.push_str(&memory_lines.join("\n"));
    }
    instruction
}

fn tutor_memory_kind_name(kind: TutorMemoryKind) -> &'static str {
    match kind {
        TutorMemoryKind::Commitment => "commitment",
        TutorMemoryKind::OpenLoop => "open_loop",
        TutorMemoryKind::LessonPlan => "lesson_plan",
        TutorMemoryKind::Reflection => "reflection",
        TutorMemoryKind::Strategy => "strategy",
    }
}

fn bounded_text(value: &str, max_chars: usize) -> String {
    let mut chars = value.trim().chars();
    let bounded = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{bounded}...")
    } else {
        bounded
    }
}

fn should_record_chat_memory(capability: &str, research_report_started: bool) -> bool {
    matches!(capability, "chat" | "organize")
        || (capability == "research" && !research_report_started)
}

async fn next_user_message_index(pool: &SessionPool, session_id: &str) -> usize {
    pool.messages(session_id)
        .await
        .map(|messages| {
            messages
                .iter()
                .filter(|message| matches!(crate::session::message_role(message), Some("user")))
                .count()
                + 1
        })
        .unwrap_or(1)
}

struct ResolvedMessageContent {
    content: String,
    resolved_count: usize,
}

fn resolve_message_content_with_space_mentions(
    notebook: &NotebookStore,
    quizzes: &QuizStore,
    content: &str,
    mentions: &[SpaceMention],
) -> ResolvedMessageContent {
    if mentions.is_empty() {
        return ResolvedMessageContent {
            content: content.to_string(),
            resolved_count: 0,
        };
    }

    let mut resolved_count = 0usize;
    let mut blocks = Vec::new();
    for mention in mentions.iter().take(8) {
        let Some((resolved_id, _markdown)) =
            resolve_space_mention_markdown(notebook, quizzes, mention)
        else {
            continue;
        };
        resolved_count += 1;
        let path = mention
            .metadata
            .get("path")
            .and_then(|value| value.as_str())
            .unwrap_or("");
        blocks.push(format!(
            "- id: {}; item_type: {}; target_id: {}; question_id: {}; title: {}; path: {}",
            resolved_id,
            mention_type_name(&mention.mention_type),
            mention.target_id.as_deref().unwrap_or(""),
            mention.question_id.as_deref().unwrap_or(""),
            mention.title,
            path
        ));
    }

    if blocks.is_empty() {
        return ResolvedMessageContent {
            content: content.to_string(),
            resolved_count,
        };
    }

    ResolvedMessageContent {
        content: format!(
            "The user explicitly referenced these Space artifacts. Use the read_space_item tool to inspect exact content before relying on a referenced item, and identify the artifact when you use it.\n\n{}\n\nUser message:\n{}",
            blocks.join("\n"),
            content
        ),
        resolved_count,
    }
}

fn mention_type_name(value: &crate::routes::space::SpaceMentionType) -> &'static str {
    match value {
        crate::routes::space::SpaceMentionType::NotebookEntry => "notebook_entry",
        crate::routes::space::SpaceMentionType::QuizSession => "quiz_session",
        crate::routes::space::SpaceMentionType::QuizQuestion => "quiz_question",
    }
}

fn summarize_exchange(user: &str, assistant: &str) -> String {
    let user = user.split_whitespace().collect::<Vec<_>>().join(" ");
    let assistant = assistant.split_whitespace().collect::<Vec<_>>().join(" ");
    format!(
        "User asked: {}; assistant answered: {}",
        user.chars().take(160).collect::<String>(),
        assistant.chars().take(220).collect::<String>()
    )
}

fn web_search_config_for_session(
    config: Option<SearchSessionConfig>,
) -> Option<tutor_tools::WebSearchConfig> {
    let config = config?;
    Some(tutor_tools::WebSearchConfig {
        provider: config.provider,
        base_url: config.base_url,
        api_key: config.api_key,
        max_results: config.max_results.unwrap_or(5).clamp(1, 10),
        fetch_timeout_secs: config.fetch_timeout_secs.unwrap_or(12).clamp(3, 60),
        max_fetch_chars: config
            .max_fetch_chars
            .unwrap_or(12_000)
            .clamp(1_000, 60_000),
    })
}

fn llm_config_for_session(config: Option<LlmSessionConfig>) -> tutor_agent::Result<LlmConfig> {
    let Some(config) = config else {
        return LlmConfig::from_env();
    };

    let provider = match config.provider.as_str() {
        "anthropic" | "claude" => LlmProviderKind::Anthropic,
        "deepseek" => LlmProviderKind::DeepSeek,
        "openai" | "openai-compatible" => LlmProviderKind::OpenAI,
        other => {
            return Err(tutor_agent::TutorError::Internal(format!(
                "unsupported LLM provider `{other}`"
            )));
        }
    };

    let api_key = config
        .api_key
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| tutor_agent::TutorError::Internal("LLM API key is not configured".into()))?;

    if config.model.trim().is_empty() {
        return Err(tutor_agent::TutorError::Internal(
            "LLM model is not configured".into(),
        ));
    }

    Ok(LlmConfig::from_parts(
        provider,
        config.model,
        api_key,
        config.base_url,
        config.chat_path,
        config.context_window_tokens,
    ))
}

fn create_quiz_llm_config_for_session(config: Option<LlmSessionConfig>) -> Option<CreateLlmConfig> {
    let config = config?;
    Some(CreateLlmConfig {
        provider: config.provider,
        model: config.model,
        api_key: config.api_key,
        base_url: config.base_url,
        chat_path: config.chat_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook_store::{NotebookEntryInput, NotebookEntryType};

    #[test]
    fn course_knowledge_access_is_scoped_to_the_session_and_selected_kb() {
        let access = course_knowledge_access_context("session-a", "kb-a", None);

        assert_eq!(
            access.scope.namespace,
            tutor_rag::COURSE_KNOWLEDGE_NAMESPACE
        );
        assert_eq!(access.scope.project.as_deref(), Some("session-a"));
        assert_eq!(
            access
                .scope
                .attributes
                .get(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE)
                .map(String::as_str),
            Some("kb-a")
        );
        assert_eq!(access.principal.subject, "local-user");
        assert_eq!(access.principal.principal_type, "local_user");
        assert_eq!(
            access.authorization_version.as_deref(),
            Some("local-user:course-knowledge:v1")
        );
    }

    #[test]
    fn resolves_space_mentions_into_turn_context() {
        let dir = tempfile::tempdir().unwrap();
        let notebook = NotebookStore::new_with_path(dir.path().join("notebook"));
        let quizzes = QuizStore::new_with_path(dir.path().join("quizzes.json"));
        let entry = notebook
            .create(NotebookEntryInput {
                space_id: None,
                entry_type: NotebookEntryType::Note,
                path: None,
                title: "Mask notes".into(),
                markdown: "Alignment marks are used during lithography.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();

        let resolved = resolve_message_content_with_space_mentions(
            &notebook,
            &quizzes,
            "summarize this",
            &[SpaceMention {
                id: format!("notebook_entry:{}", entry.id),
                mention_type: crate::routes::space::SpaceMentionType::NotebookEntry,
                target_id: Some(entry.id),
                question_id: None,
                title: "Mask notes".into(),
                preview: None,
                metadata: serde_json::json!({}),
            }],
        );

        assert_eq!(resolved.resolved_count, 1);
        assert!(resolved.content.contains("read_space_item"));
        assert!(resolved.content.contains("notebook_entry:"));
        assert!(!resolved.content.contains("Alignment marks"));
        assert!(resolved.content.contains("User message:\nsummarize this"));
    }

    #[test]
    fn creates_durable_research_artifact_from_structured_tool_result() {
        let artifact = message_artifact_from_tool_result(
            &serde_json::json!({
                "tool": "create_research_report",
                "ok": true,
                "details": {
                    "title": "Transformer Architecture",
                    "markdown": "# Report\n\n## Summary\nDetails."
                }
            }),
            "run-123",
        )
        .unwrap();

        assert_eq!(artifact["type"], "research_report");
        assert_eq!(artifact["artifact_store"], "runtime_trace");
        assert_eq!(artifact["artifact_id"], "run-123");
        assert_eq!(artifact["title"], "Transformer Architecture");
    }

    #[test]
    fn research_memory_only_records_conversation_before_report_workflow() {
        assert!(should_record_chat_memory("research", false));
        assert!(!should_record_chat_memory("research", true));
        assert!(should_record_chat_memory("chat", true));
        assert!(should_record_chat_memory("organize", true));
        assert!(!should_record_chat_memory("quiz", false));
    }

    #[test]
    fn research_report_boundary_is_detected_from_tool_trace() {
        let call = serde_json::json!({ "tool": "create_research_report" });
        assert!(trace_invokes_research_report("tool_call", &call));
        assert!(trace_invokes_research_report("tool_result", &call));
        assert!(!trace_invokes_research_report("content", &call));
        assert!(!trace_invokes_research_report(
            "tool_call",
            &serde_json::json!({ "tool": "propose_research_plan" })
        ));
    }

    #[tokio::test]
    async fn buffered_trace_flush_keeps_final_answer_on_active_path() {
        let root = std::env::temp_dir().join(format!(
            "llm-tutor-ws-session-test-{}",
            uuid::Uuid::new_v4()
        ));
        let pool = SessionPool::new_with_root(&root);
        let id = pool
            .create("chat", None, false, None, None, None)
            .await
            .unwrap();
        let session = pool.open_runtime_session(&id).await.unwrap();
        session
            .append_message(tutor_agent::chat::user_message("question"))
            .await
            .unwrap();

        let pending = Mutex::new(vec![PendingSessionEvent {
            kind: "final_answer".into(),
            data: serde_json::json!({ "text": "answer" }),
            run_state: None,
            artifact: None,
        }]);
        session
            .append_message(tutor_agent::chat::assistant_message("answer"))
            .await
            .unwrap();
        flush_pending_session_events(&pool, &id, 1, &pending)
            .await
            .unwrap();

        let messages = pool.messages(&id).await.unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(crate::session::message_text(&messages[1]), "answer");
        let traces = pool.traces(&id).await.unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].kind, "final_answer");

        drop(session);
        drop(pool);
        let _ = std::fs::remove_dir_all(root);
    }
}
