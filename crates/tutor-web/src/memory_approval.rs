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

use crate::memory_runtime::SavedMemoryApprover;
use crate::stream::TutorStream;

const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(90);

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
        Self {
            stream,
            session_id: session_id.into(),
            product_run_id: product_run_id.into(),
            timeout: DEFAULT_APPROVAL_TIMEOUT,
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
        let ids = state.pending.keys().cloned().collect::<Vec<_>>();
        state.consumed.extend(ids);
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
        let mut state = self.state.lock().unwrap();
        state.pending.remove(&request_id);
        state.consumed.insert(request_id);
        result
    }
}

impl SavedMemoryApprover for WebMemoryApprovalCoordinator {
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
                "valid_until": write.expires_at,
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

    use chrono::Utc;
    use llm_harness_runtime_knowledge::KnowledgeRef;
    use llm_harness_runtime_memory::{MemoryMutationOrigin, MemoryProvenance, MemoryWrite};
    use llm_harness_types::RunId;

    use super::*;
    use crate::memory_runtime::USER_MEMORY_SOURCE_ID;

    #[test]
    fn write_approval_displays_the_exact_normalized_mutation() {
        let mutation = MemoryMutation::Write {
            write: MemoryWrite {
                content: "请记住我偏好中文。".into(),
                kind: Some("preference".into()),
                metadata: BTreeMap::new(),
                provenance: MemoryProvenance {
                    run_id: RunId::new(),
                    session_id: Some("session-a".into()),
                    origin: MemoryMutationOrigin::ExplicitTool {
                        tool_use_id: "tool-a".into(),
                    },
                    recorded_at: Utc::now(),
                },
                idempotency_key: "write-a".into(),
                expires_at: None,
            },
        };

        let (tool, args) = approval_presentation(&mutation);
        assert_eq!(tool, "memory_write");
        assert_eq!(args["content"], "请记住我偏好中文。");
        assert_eq!(args["kind"], "preference");
        assert!(args["valid_until"].is_null());
    }

    #[test]
    fn forget_approval_displays_the_exact_revision() {
        let mutation = MemoryMutation::Delete {
            reference: KnowledgeRef {
                source_id: USER_MEMORY_SOURCE_ID.into(),
                item_id: "memory-a".into(),
                revision: Some("revision-a".into()),
            },
        };

        let (tool, args) = approval_presentation(&mutation);
        assert_eq!(tool, "memory_forget");
        assert_eq!(args["reference"]["source_id"], USER_MEMORY_SOURCE_ID);
        assert_eq!(args["reference"]["item_id"], "memory-a");
        assert_eq!(args["reference"]["revision"], "revision-a");
    }
}
