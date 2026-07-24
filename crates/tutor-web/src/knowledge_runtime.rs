use std::sync::Arc;

use llm_harness_runtime_knowledge::EvidenceAuthority;

pub(crate) fn course_evidence_authority() -> Arc<EvidenceAuthority> {
    let mut secret = Vec::with_capacity(32);
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    secret.extend_from_slice(uuid::Uuid::new_v4().as_bytes());
    Arc::new(
        EvidenceAuthority::new(secret, [tutor_agent::course_evidence_provider_id()])
            .expect("generated evidence secret and registered provider are valid"),
    )
}
