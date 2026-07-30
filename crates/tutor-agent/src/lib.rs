pub mod capability;
pub mod chat;
pub mod code_exec;
pub mod error;
pub mod event_sink;
pub mod governance;
pub mod knowledge;
pub mod llm_provider;
pub mod memory;
pub mod quiz;
pub mod research;
pub mod runtime_engine;
pub mod runtime_harness;
pub mod runtime_workflow;
pub mod terminal_approver;

pub use capability::{Capability, CapabilityRouter, LearnerMemoryMode, TutorMemoryMode};
pub use error::{Result, TutorError};
pub use knowledge::{
    KnowledgeRuntime, agent_knowledge_evidence_provider_id, assemble_course_knowledge,
    assemble_knowledge_runtime, required_course_citation_policy,
};
pub use llm_provider::{LlmConfig, LlmProviderKind};
