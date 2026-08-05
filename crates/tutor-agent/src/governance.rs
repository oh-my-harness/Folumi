use std::sync::Arc;

use llm_harness_runtime::observability::audit::{AuditEntry, AuditEventType, AuditSink};
use llm_harness_types::BeforeToolCallHook;
use uuid::Uuid;

/// Session-wide governance configuration shared across all harnesses.
#[derive(Clone)]
pub struct GovernanceConfig {
    /// Optional audit sink for writing structured learning-trail events.
    pub audit: Option<Arc<dyn AuditSink>>,
    /// Optional human approval gate.
    pub approval: Option<Arc<dyn BeforeToolCallHook>>,
    /// When true, `code_exec` calls require human approval.
    pub require_code_exec_approval: bool,
}

impl GovernanceConfig {
    pub fn new(audit: Option<Arc<dyn AuditSink>>, require_code_exec_approval: bool) -> Self {
        Self {
            audit,
            approval: None,
            require_code_exec_approval,
        }
    }

    pub fn with_approval(mut self, approval: Arc<dyn BeforeToolCallHook>) -> Self {
        self.approval = Some(approval);
        self
    }
}

/// Helper to create an audit entry with reasonable defaults for v0.1.
/// The `hash` and `prev_hash` fields are overwritten by `JsonlAuditSink::record`.
pub fn make_audit_entry(event_type: AuditEventType, payload: serde_json::Value) -> AuditEntry {
    AuditEntry {
        timestamp: chrono::Utc::now(),
        trace_id: String::new(),
        session_id: llm_harness_types::EntryId(Uuid::new_v4()),
        task_id: None,
        principal: "system".into(),
        event_type,
        payload,
        decision: None,
        cost_delta: None,
        prev_hash: None,
        hash: String::new(),
    }
}

/// Record an audit event if the audit sink is configured.
pub async fn record_audit(
    audit: &Option<Arc<dyn AuditSink>>,
    event_type: AuditEventType,
    payload: serde_json::Value,
) {
    if let Some(sink) = audit {
        let entry = make_audit_entry(event_type, payload);
        let _ = sink.record(entry).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn governance_config_builds_without_approval() {
        let cfg = GovernanceConfig::new(None, false);
        assert!(!cfg.require_code_exec_approval);
    }
}
