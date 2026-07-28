use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::Duration;

use futures::future::BoxFuture;
use llm_harness_runtime_knowledge::KnowledgeRequestContext;
use llm_harness_runtime_memory::{
    MemoryMutation, MemoryMutationGateError, MemoryMutationRequest, MemorySessionId,
};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use crate::learner_memory_write::LearnerMemoryApprover;
use crate::stream::TutorStream;

const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalResponseOutcome {
    Resolved,
    Replayed,
    Unknown,
}

struct PendingApproval {
    response: oneshot::Sender<bool>,
    _request: MemoryMutationRequest,
}

#[derive(Default)]
struct ApprovalState {
    pending: HashMap<String, PendingApproval>,
    consumed: HashSet<String>,
    closed: bool,
}

pub struct WebMemoryApprovalCoordinator {
    stream: TutorStream,
    session_id: String,
    product_run_id: String,
    timeout: Duration,
    disconnected: CancellationToken,
    state: Mutex<ApprovalState>,
}

impl WebMemoryApprovalCoordinator {
    pub fn new(
        stream: TutorStream,
        session_id: impl Into<String>,
        product_run_id: impl Into<String>,
        disconnected: CancellationToken,
    ) -> Self {
        Self::with_timeout(
            stream,
            session_id,
            product_run_id,
            disconnected,
            DEFAULT_APPROVAL_TIMEOUT,
        )
    }

    fn with_timeout(
        stream: TutorStream,
        session_id: impl Into<String>,
        product_run_id: impl Into<String>,
        disconnected: CancellationToken,
        timeout: Duration,
    ) -> Self {
        Self {
            stream,
            session_id: session_id.into(),
            product_run_id: product_run_id.into(),
            timeout,
            disconnected,
            state: Mutex::new(ApprovalState::default()),
        }
    }

    pub fn resolve(&self, request_id: &str, approved: bool) -> ApprovalResponseOutcome {
        let pending = {
            let mut state = self.state.lock().unwrap();
            if let Some(pending) = state.pending.remove(request_id) {
                state.consumed.insert(request_id.to_string());
                Some(pending)
            } else if state.consumed.contains(request_id) {
                return ApprovalResponseOutcome::Replayed;
            } else {
                return ApprovalResponseOutcome::Unknown;
            }
        };
        if let Some(pending) = pending {
            let _ = pending.response.send(approved);
        }
        ApprovalResponseOutcome::Resolved
    }

    pub fn close(&self) {
        self.disconnected.cancel();
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        let request_ids = state.pending.keys().cloned().collect::<Vec<_>>();
        state.consumed.extend(request_ids);
        state.pending.clear();
    }

    async fn request_approval(
        &self,
        ctx: KnowledgeRequestContext<'_>,
        request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> Result<(), MemoryMutationGateError> {
        if abort.is_cancelled() {
            return Err(MemoryMutationGateError::Aborted);
        }
        let Some(memory_session_id) = ctx.run.extension::<MemorySessionId>() else {
            return Err(MemoryMutationGateError::Unavailable);
        };
        if memory_session_id.as_str() != self.session_id {
            return Err(MemoryMutationGateError::Unavailable);
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let (response_tx, response_rx) = oneshot::channel();
        {
            let mut state = self.state.lock().unwrap();
            if state.closed || !state.pending.is_empty() {
                return Err(MemoryMutationGateError::Unavailable);
            }
            state.pending.insert(
                request_id.clone(),
                PendingApproval {
                    response: response_tx,
                    _request: request.clone(),
                },
            );
        }

        let (tool, args) = approval_presentation(&request.mutation);
        self.stream
            .status(
                "approval_request",
                serde_json::json!({
                    "request_id": request_id,
                    "run_id": self.product_run_id,
                    "tool": tool,
                    "args": args,
                }),
            )
            .await;

        let result = tokio::select! {
            _ = abort.cancelled() => Err(MemoryMutationGateError::Aborted),
            _ = self.disconnected.cancelled() => Err(MemoryMutationGateError::Unavailable),
            _ = tokio::time::sleep(self.timeout) => Err(MemoryMutationGateError::Denied),
            response = response_rx => match response {
                Ok(true) => Ok(()),
                Ok(false) => Err(MemoryMutationGateError::Denied),
                Err(_) => Err(MemoryMutationGateError::Unavailable),
            },
        };
        self.consume_if_pending(&request_id);
        result
    }

    fn consume_if_pending(&self, request_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.pending.remove(request_id);
        state.consumed.insert(request_id.to_string());
    }
}

impl LearnerMemoryApprover for WebMemoryApprovalCoordinator {
    fn authorize<'a>(
        &'a self,
        ctx: KnowledgeRequestContext<'a>,
        request: MemoryMutationRequest,
        abort: CancellationToken,
    ) -> BoxFuture<'a, Result<(), MemoryMutationGateError>> {
        Box::pin(self.request_approval(ctx, request, abort))
    }
}

fn approval_presentation(mutation: &MemoryMutation) -> (&'static str, serde_json::Value) {
    match mutation {
        MemoryMutation::Write { write } => (
            "memory_write",
            serde_json::json!({
                "content": write.content,
                "kind": write.kind,
                "expires_at": write.expires_at,
            }),
        ),
        MemoryMutation::Delete { reference } => (
            "memory_forget",
            serde_json::json!({
                "reference": {
                    "source_id": reference.source_id,
                    "item_id": reference.item_id,
                    "revision": reference.revision,
                }
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use chrono::Utc;
    use llm_harness_runtime_knowledge::{KnowledgeAccessContext, KnowledgeScope, PrincipalRef};
    use llm_harness_runtime_memory::{MemoryMutationOrigin, MemoryProvenance, MemoryWrite};
    use llm_harness_types::{RunContext, RunRequest};

    use super::*;
    use crate::stream::StreamEvent;

    fn request() -> MemoryMutationRequest {
        let run = RunContext::new(RunRequest::from_text("remember"));
        MemoryMutationRequest {
            mutation: MemoryMutation::Write {
                write: MemoryWrite {
                    content: "Prefers diagrams.".into(),
                    kind: Some("preference".into()),
                    metadata: BTreeMap::new(),
                    provenance: MemoryProvenance {
                        run_id: run.id(),
                        session_id: Some("session-a".into()),
                        origin: MemoryMutationOrigin::ExplicitTool {
                            tool_use_id: "tool-1".into(),
                        },
                        recorded_at: Utc::now(),
                    },
                    idempotency_key: "idempotency-key".into(),
                    expires_at: None,
                },
            },
            origin: MemoryMutationOrigin::ExplicitTool {
                tool_use_id: "tool-1".into(),
            },
        }
    }

    fn run(session_id: &str) -> Arc<RunContext> {
        Arc::new(RunContext::new(
            RunRequest::from_text("remember")
                .with_extension(MemorySessionId::new(session_id).unwrap()),
        ))
    }

    fn access() -> Arc<KnowledgeAccessContext> {
        Arc::new(KnowledgeAccessContext::new(
            KnowledgeScope::new("llm-tutor.agent-knowledge"),
            PrincipalRef::new("local-user", "user"),
        ))
    }

    async fn spawn_authorize(
        coordinator: Arc<WebMemoryApprovalCoordinator>,
        run: Arc<RunContext>,
        access: Arc<KnowledgeAccessContext>,
        abort: CancellationToken,
    ) -> tokio::task::JoinHandle<Result<(), MemoryMutationGateError>> {
        tokio::spawn(async move {
            coordinator
                .authorize(
                    KnowledgeRequestContext {
                        run: &run,
                        access: &access,
                    },
                    request(),
                    abort,
                )
                .await
        })
    }

    #[tokio::test]
    async fn approved_request_is_bound_single_use_and_presented_without_internal_metadata() {
        let stream = TutorStream::new(8);
        let mut events = stream.subscribe();
        let coordinator = Arc::new(WebMemoryApprovalCoordinator::with_timeout(
            stream,
            "session-a",
            "product-run-a",
            CancellationToken::new(),
            Duration::from_secs(1),
        ));
        let task = spawn_authorize(
            coordinator.clone(),
            run("session-a"),
            access(),
            CancellationToken::new(),
        )
        .await;
        let event = events.recv().await.unwrap();
        let StreamEvent::Status { kind, data } = event else {
            panic!("expected approval status");
        };
        assert_eq!(kind, "approval_request");
        assert_eq!(data["run_id"], "product-run-a");
        assert_eq!(data["tool"], "memory_write");
        assert_eq!(data["args"]["content"], "Prefers diagrams.");
        assert!(data["args"].get("idempotency_key").is_none());
        let request_id = data["request_id"].as_str().unwrap();

        assert_eq!(
            coordinator.resolve(request_id, true),
            ApprovalResponseOutcome::Resolved
        );
        assert!(task.await.unwrap().is_ok());
        assert_eq!(
            coordinator.resolve(request_id, true),
            ApprovalResponseOutcome::Replayed
        );
    }

    #[tokio::test]
    async fn denial_timeout_stop_disconnect_and_wrong_session_fail_closed() {
        let denied_stream = TutorStream::new(8);
        let mut denied_events = denied_stream.subscribe();
        let denied = Arc::new(WebMemoryApprovalCoordinator::with_timeout(
            denied_stream,
            "session-a",
            "run-denied",
            CancellationToken::new(),
            Duration::from_secs(1),
        ));
        let denied_task = spawn_authorize(
            denied.clone(),
            run("session-a"),
            access(),
            CancellationToken::new(),
        )
        .await;
        let StreamEvent::Status { data, .. } = denied_events.recv().await.unwrap() else {
            panic!("expected approval status");
        };
        let denied_id = data["request_id"].as_str().unwrap();
        assert_eq!(
            denied.resolve(denied_id, false),
            ApprovalResponseOutcome::Resolved
        );
        assert_eq!(
            denied_task.await.unwrap(),
            Err(MemoryMutationGateError::Denied)
        );

        let timeout_stream = TutorStream::new(8);
        let mut timeout_events = timeout_stream.subscribe();
        let timeout = Arc::new(WebMemoryApprovalCoordinator::with_timeout(
            timeout_stream,
            "session-a",
            "run-timeout",
            CancellationToken::new(),
            Duration::from_millis(10),
        ));
        let timeout_task = spawn_authorize(
            timeout.clone(),
            run("session-a"),
            access(),
            CancellationToken::new(),
        )
        .await;
        let StreamEvent::Status { data, .. } = timeout_events.recv().await.unwrap() else {
            panic!("expected approval status");
        };
        let timeout_id = data["request_id"].as_str().unwrap().to_string();
        assert_eq!(
            timeout_task.await.unwrap(),
            Err(MemoryMutationGateError::Denied)
        );
        assert_eq!(
            timeout.resolve(&timeout_id, true),
            ApprovalResponseOutcome::Replayed
        );

        let stop_stream = TutorStream::new(8);
        let mut stop_events = stop_stream.subscribe();
        let stopped = Arc::new(WebMemoryApprovalCoordinator::with_timeout(
            stop_stream,
            "session-a",
            "run-stopped",
            CancellationToken::new(),
            Duration::from_secs(1),
        ));
        let abort = CancellationToken::new();
        let stopped_task =
            spawn_authorize(stopped.clone(), run("session-a"), access(), abort.clone()).await;
        let StreamEvent::Status { data, .. } = stop_events.recv().await.unwrap() else {
            panic!("expected approval status");
        };
        let stopped_id = data["request_id"].as_str().unwrap().to_string();
        abort.cancel();
        assert_eq!(
            stopped_task.await.unwrap(),
            Err(MemoryMutationGateError::Aborted)
        );
        assert_eq!(
            stopped.resolve(&stopped_id, true),
            ApprovalResponseOutcome::Replayed
        );

        let disconnected = CancellationToken::new();
        let closed_stream = TutorStream::new(8);
        let mut closed_events = closed_stream.subscribe();
        let closed = Arc::new(WebMemoryApprovalCoordinator::with_timeout(
            closed_stream,
            "session-a",
            "run-disconnected",
            disconnected,
            Duration::from_secs(1),
        ));
        let closed_task = spawn_authorize(
            closed.clone(),
            run("session-a"),
            access(),
            CancellationToken::new(),
        )
        .await;
        let StreamEvent::Status { .. } = closed_events.recv().await.unwrap() else {
            panic!("expected approval status");
        };
        closed.close();
        assert_eq!(
            closed_task.await.unwrap(),
            Err(MemoryMutationGateError::Unavailable)
        );

        let wrong_session = WebMemoryApprovalCoordinator::with_timeout(
            TutorStream::new(8),
            "session-a",
            "run-wrong-session",
            CancellationToken::new(),
            Duration::from_secs(1),
        );
        let wrong = wrong_session
            .authorize(
                KnowledgeRequestContext {
                    run: &run("session-b"),
                    access: &access(),
                },
                request(),
                CancellationToken::new(),
            )
            .await;
        assert_eq!(wrong, Err(MemoryMutationGateError::Unavailable));
        assert_eq!(
            wrong_session.resolve("not-issued", true),
            ApprovalResponseOutcome::Unknown
        );
    }
}
