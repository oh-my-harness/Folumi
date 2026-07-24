use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use futures::{future::BoxFuture, stream};
use llm_adapter::{
    ChatRequest, ChatResponse, Message, Provider, ProviderCapabilities, RequestContent,
    StreamHandle,
    types::{ContentKind, StopReason, StreamEvent, Usage},
};
use llm_harness_agent::{JsonlSessionRepo, Session, SessionRepo, session::CreateSessionOptions};
use llm_harness_loop::{
    LlmError,
    test_utils::{MockLlmClient, MockResponse, NoOpEnv},
};
use llm_harness_runtime::observability::audit::AuditSink;
use llm_harness_runtime_knowledge::{
    EvidenceAuthority, KNOWLEDGE_READ_TOOL_NAME, KNOWLEDGE_SEARCH_TOOL_NAME,
    KnowledgeAccessContext, KnowledgeScope, PrincipalRef,
};
use llm_harness_runtime_sandbox_os::OsEnv;
use llm_harness_types::{
    AgentMessage, AssistantMessage, AssistantMessageKind, DataBlock, ExecutionEnv, RunContext,
    RunRequest, Tool, ToolContext, ToolFailure, ToolResult,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tutor_agent::capability::Capability;
use tutor_agent::chat::{assistant_message, user_message};
use tutor_agent::event_sink::EventSink;
use tutor_agent::governance::GovernanceConfig;
use tutor_agent::research::{ResearchWorkflowInput, run_research_workflow_with_runtime};
use tutor_agent::{
    CapabilityRouter, LlmConfig, assemble_course_knowledge, course_evidence_provider_id,
};
use tutor_rag::{EmbeddingConfig, LanceDbKnowledgeSource, LanceDbRag};

fn make_governance(audit: Option<Arc<dyn AuditSink>>) -> GovernanceConfig {
    GovernanceConfig::new(100.0, audit, false)
}

fn make_router(responses: Vec<MockResponse>, governance: GovernanceConfig) -> CapabilityRouter {
    let client = Arc::new(MockLlmClient::new(responses));
    let env = Arc::new(NoOpEnv) as Arc<dyn ExecutionEnv>;
    let llm = LlmConfig::anthropic("mock-model", "");
    CapabilityRouter::new(env, llm, governance).with_client(client)
}

fn make_router_with_env(
    responses: Vec<MockResponse>,
    governance: GovernanceConfig,
    env: Arc<dyn ExecutionEnv>,
) -> CapabilityRouter {
    let client = Arc::new(MockLlmClient::new(responses));
    let llm = LlmConfig::anthropic("mock-model", "");
    CapabilityRouter::new(env, llm, governance).with_client(client)
}

struct CitationEchoMockClient {
    responses: Mutex<Vec<MockResponse>>,
}

impl CitationEchoMockClient {
    fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl Provider for CitationEchoMockClient {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new(false, false, false)
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.chat_stream(request).await?.collect().await
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<StreamHandle, LlmError> {
        let response = {
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                let handle = citation_handle_from_request(request)
                    .expect("knowledge_read tool result should contain a citation handle");
                MockResponse::text(&format!(
                    "Newton's laws are grounded in the selected course evidence. {handle}"
                ))
            } else {
                responses.remove(0)
            }
        };
        mock_stream(response)
    }
}

struct ResearchKnowledgeMockClient {
    call_count: AtomicUsize,
    reference: serde_json::Value,
    read_args: String,
}

impl ResearchKnowledgeMockClient {
    fn new(reference: serde_json::Value, read_args: String) -> Self {
        Self {
            call_count: AtomicUsize::new(0),
            reference,
            read_args,
        }
    }
}

#[async_trait]
impl Provider for ResearchKnowledgeMockClient {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::new(false, false, false)
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.chat_stream(request).await?.collect().await
    }

    async fn chat_stream(&self, request: &ChatRequest) -> Result<StreamHandle, LlmError> {
        let response = match self.call_count.fetch_add(1, Ordering::SeqCst) {
            0 => MockResponse::tool_use(
                "research-knowledge-search",
                KNOWLEDGE_SEARCH_TOOL_NAME,
                r#"{"query":"Newton"}"#,
            ),
            1 => MockResponse::text(
                &serde_json::json!({
                    "queries": ["Newton"],
                    "source_candidates": [{
                        "kind": "knowledge",
                        "title": "Newton notes",
                        "url": "knowledge://course/Newton-notes",
                        "snippet": "Newton's laws describe motion and force.",
                        "reference": self.reference.clone(),
                    }],
                    "failures": [],
                })
                .to_string(),
            ),
            2 => MockResponse::tool_use(
                "research-knowledge-read",
                KNOWLEDGE_READ_TOOL_NAME,
                &self.read_args,
            ),
            3 => {
                let handle = citation_handle_from_request(request)
                    .expect("read_sources should receive a runtime citation handle");
                MockResponse::text(
                    &serde_json::json!({
                        "sources": [{
                            "kind": "knowledge",
                            "title": "Newton notes",
                            "url": "knowledge://course/Newton-notes",
                            "summary": format!("Newton's laws describe motion and force. {handle}"),
                            "used_for": "grounding the explanation",
                            "reference": self.reference.clone(),
                            "citation": handle,
                        }],
                        "failures": [],
                    })
                    .to_string(),
                )
            }
            4 => MockResponse::text(r#"{"verdict":"pass","issues":[]}"#),
            5 => MockResponse::tool_use(
                "research-final-knowledge-read",
                KNOWLEDGE_READ_TOOL_NAME,
                &self.read_args,
            ),
            6 => {
                let handle = citation_handle_from_request(request)
                    .expect("write_report should receive a fresh runtime citation handle");
                MockResponse::text(
                    &serde_json::json!({
                        "markdown": format!(
                            "# Newton Report\n\n## Summary\n\nNewton's laws describe motion and force. {handle}\n\n## Key Findings\n\n- Motion changes under force. {handle}\n\n## Analysis\n\nThe course evidence supports the finding. {handle}\n\n## Limitations\n\nOne course document was used.\n\n## Follow-up Questions\n\n- How do the laws apply to orbiting bodies?\n\n## Sources\n\n- Newton notes {handle}"
                        ),
                        "sources": [{
                            "title": "Newton notes",
                            "url": "knowledge://course/Newton-notes",
                        }],
                    })
                    .to_string(),
                )
            }
            call => panic!("unexpected Research Knowledge mock call {call}"),
        };
        mock_stream(response)
    }
}

fn citation_handle_from_request(request: &ChatRequest) -> Option<String> {
    request
        .messages()
        .iter()
        .rev()
        .find_map(|message| match message {
            Message::Tool { content, .. } => content.iter().find_map(|block| {
                let RequestContent::Text(text) = block else {
                    return None;
                };
                let start = text.find("[K:")?;
                let suffix = &text[start..];
                let end = suffix.find(']')?;
                Some(suffix[..=end].to_string())
            }),
            _ => None,
        })
}

fn mock_stream(response: MockResponse) -> Result<StreamHandle, LlmError> {
    if let Some(error) = response.stream_error {
        return Err(error);
    }
    Ok(StreamHandle::from_raw_stream(
        response.model,
        Box::pin(stream::iter(response.events)),
    ))
}

fn progress_text_response(text: &str) -> MockResponse {
    MockResponse {
        model: "mock-model".into(),
        stream_error: None,
        events: vec![
            Ok(StreamEvent::ContentStart {
                index: 0,
                kind: ContentKind::Text,
            }),
            Ok(StreamEvent::TextDelta {
                index: 0,
                text: text.into(),
            }),
            Ok(StreamEvent::ContentStop {
                index: 0,
                signature: None,
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::ToolUse,
                usage: Usage::default(),
            }),
        ],
    }
}

fn text_delta_end_turn_response(text: &str) -> MockResponse {
    MockResponse {
        model: "mock-model".into(),
        stream_error: None,
        events: vec![
            Ok(StreamEvent::ContentStart {
                index: 0,
                kind: ContentKind::Text,
            }),
            Ok(StreamEvent::TextDelta {
                index: 0,
                text: text.into(),
            }),
            Ok(StreamEvent::ContentStop {
                index: 0,
                signature: None,
            }),
            Ok(StreamEvent::MessageStop {
                stop_reason: StopReason::EndTurn,
                usage: Usage::default(),
            }),
        ],
    }
}

fn hash_embedding_config() -> EmbeddingConfig {
    EmbeddingConfig {
        provider: "hash".into(),
        model: "test".into(),
        api_key: String::new(),
        base_url: None,
        embeddings_path: None,
        dimensions: Some(32),
        send_dimensions: false,
    }
}

fn knowledge_access(kb: &str) -> KnowledgeAccessContext {
    let mut scope = KnowledgeScope::new(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE);
    scope
        .attributes
        .insert(tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(), kb.into());
    KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "test"))
}

fn tool_context(request: RunRequest) -> ToolContext {
    let (update_tx, _update_rx) = mpsc::channel(1);
    ToolContext {
        env: Arc::new(NoOpEnv),
        run: Arc::new(RunContext::new(request)),
        abort: CancellationToken::new(),
        tool_use_id: "knowledge-setup".into(),
        turn_index: 0,
        assistant_message: Arc::new(AssistantMessage {
            kind: AssistantMessageKind::Progress,
            message_id: "knowledge-setup-message".into(),
            turn_id: "knowledge-setup-turn".into(),
            content: vec![],
            usage: None,
            stop_reason: None,
            timestamp: chrono::Utc::now(),
            provider: None,
            api: None,
            model: None,
            error_message: None,
        }),
        update_tx,
    }
}

#[derive(Default)]
struct TraceRecorder {
    events: Mutex<Vec<(String, serde_json::Value)>>,
}

impl TraceRecorder {
    fn events(&self) -> Vec<(String, serde_json::Value)> {
        self.events.lock().unwrap().clone()
    }
}

impl EventSink for TraceRecorder {
    fn trace(&self, kind: String, data: serde_json::Value) -> BoxFuture<'static, ()> {
        self.events.lock().unwrap().push((kind, data));
        Box::pin(async {})
    }
}

#[derive(Clone)]
struct TestAccessMarker(&'static str);

struct CaptureRunExtensionTool {
    captured: Arc<Mutex<Option<String>>>,
    schema: serde_json::Value,
}

impl Tool for CaptureRunExtensionTool {
    fn name(&self) -> &str {
        "capture_run_extension"
    }

    fn description(&self) -> &str {
        "Capture a typed run extension for an integration test."
    }

    fn parameters_schema(&self) -> &serde_json::Value {
        &self.schema
    }

    fn execute<'a>(
        &'a self,
        _args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> BoxFuture<'a, std::result::Result<ToolResult, ToolFailure>> {
        let captured = self.captured.clone();
        let marker = ctx
            .run
            .extension::<TestAccessMarker>()
            .map(|marker| marker.0.to_string());
        Box::pin(async move {
            *captured.lock().unwrap() = marker;
            let model_content = vec![DataBlock::text("captured")];
            Ok(ToolResult::projected(
                model_content.clone(),
                model_content,
                serde_json::Value::Null,
                false,
            ))
        })
    }
}

#[tokio::test]
async fn smoke_chat_text_only() {
    let responses = vec![MockResponse::text("Hello from mock tutor.")];
    let router = make_router(responses, make_governance(None));
    let answer = router.run(Capability::Chat, "what is 2+2?").await.unwrap();
    assert!(!answer.is_empty());
}

#[tokio::test]
async fn chat_run_request_extension_reaches_product_tool_context() {
    let captured = Arc::new(Mutex::new(None));
    let router = make_router(
        vec![
            MockResponse::tool_use("capture-1", "capture_run_extension", "{}"),
            MockResponse::text("done"),
        ],
        make_governance(None),
    )
    .with_product_tool(Arc::new(CaptureRunExtensionTool {
        captured: captured.clone(),
        schema: serde_json::json!({"type":"object","properties":{}}),
    }));

    router
        .run_request(
            Capability::Chat,
            RunRequest::from_text("capture access").with_extension(TestAccessMarker("course-kb-1")),
        )
        .await
        .unwrap();

    assert_eq!(captured.lock().unwrap().as_deref(), Some("course-kb-1"));
}

#[tokio::test]
async fn chat_uses_runtime_knowledge_tools_and_keeps_read_bodies_out_of_session() {
    let dir = TempDir::new().unwrap();
    let rag = LanceDbRag::new(dir.path().join("rag"), hash_embedding_config());
    let private_tail = "READ_BODY_PRIVATE_TAIL_MUST_NOT_BE_PERSISTED";
    let body = format!(
        "{} {private_tail}",
        "Newton's laws describe motion and force. ".repeat(16)
    );
    rag.ingest_text("kb-a", "document-a::Newton notes", &body)
        .await
        .unwrap();

    let authority =
        Arc::new(EvidenceAuthority::new(vec![7; 32], [course_evidence_provider_id()]).unwrap());
    let knowledge_runtime =
        assemble_course_knowledge(LanceDbKnowledgeSource::new(rag, "kb-a"), authority).unwrap();
    let access = knowledge_access("kb-a");
    let mut knowledge_tools = Vec::new();
    knowledge_runtime
        .plugin()
        .register_tools(&mut knowledge_tools);
    let search = knowledge_tools
        .iter()
        .find(|tool| tool.name() == KNOWLEDGE_SEARCH_TOOL_NAME)
        .unwrap();
    let setup_context =
        tool_context(RunRequest::from_text("Newton").with_extension(access.clone()));
    let search_result = search
        .execute(serde_json::json!({"query": "Newton"}), &setup_context)
        .await
        .unwrap();
    let reference = search_result.details["hits"][0]["reference"].clone();
    let selector = search_result.details["hits"][0]["suggested_selectors"][0].clone();
    let read_args = serde_json::json!({
        "reference": reference.clone(),
        "selector": selector,
    })
    .to_string();

    let sink = Arc::new(TraceRecorder::default());
    let client = Arc::new(CitationEchoMockClient::new(vec![
        MockResponse::tool_use(
            "knowledge-search",
            KNOWLEDGE_SEARCH_TOOL_NAME,
            r#"{"query":"Newton"}"#,
        ),
        MockResponse::tool_use("knowledge-read", KNOWLEDGE_READ_TOOL_NAME, &read_args),
    ]));
    let router = CapabilityRouter::new(
        Arc::new(NoOpEnv),
        LlmConfig::anthropic("mock-model", ""),
        make_governance(None),
    )
    .with_client(client)
    .with_knowledge_runtime(knowledge_runtime)
    .with_associated_kb("kb-a")
    .with_event_sink(sink.clone());

    let repo = JsonlSessionRepo::new(dir.path().join("sessions"));
    let storage = repo.create(CreateSessionOptions::default()).await.unwrap();
    let session = Session::new(storage.clone());
    let inspect_session = Session::new(storage);
    let answer = router
        .run_request_with_session_cancel(
            Capability::Chat,
            session,
            RunRequest::from_text("Explain Newton's laws").with_extension(access),
            None,
        )
        .await
        .unwrap();

    assert!(answer.contains("Newton"));
    let events = sink.events();
    assert!(events.iter().any(|(kind, data)| {
        kind == "tool_result" && data["tool"] == KNOWLEDGE_SEARCH_TOOL_NAME && data["ok"] == true
    }));
    assert!(events.iter().any(|(kind, data)| {
        kind == "tool_result"
            && data["tool"] == KNOWLEDGE_READ_TOOL_NAME
            && data["ok"] == true
            && data["details"]["citation"]["handle"]
                .as_str()
                .is_some_and(|handle| handle.starts_with("[K:"))
    }));

    let context = inspect_session.build_context().await.unwrap();
    let persisted_context = format!("{:?}", context.messages);
    assert!(
        !persisted_context.contains(private_tail),
        "knowledge_read body leaked into durable Session context: {persisted_context}"
    );
}

#[tokio::test]
async fn chat_returns_error_instead_of_no_response() {
    let responses = vec![MockResponse {
        events: vec![Err(LlmError::InvalidRequest("bad request".into()))],
        model: "mock-model".into(),
        stream_error: None,
    }];
    let router = make_router(responses, make_governance(None));

    let err = router
        .run(Capability::Chat, "trigger error")
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("bad request"),
        "expected provider error to be surfaced, got {err}"
    );
}

#[tokio::test]
async fn chat_web_tool_call_then_text() {
    let responses = vec![
        MockResponse::tool_use("use-1", "web_search", r#"{"query":"Newton"}"#),
        MockResponse::text("Newton's first law: an object at rest stays at rest."),
    ];
    let router = make_router(responses, make_governance(None));
    let answer = router
        .run(Capability::Chat, "explain Newton's first law")
        .await
        .unwrap();
    assert!(answer.contains("Newton"));
}

#[tokio::test]
async fn chat_can_call_read_memory_then_text() {
    let memory_dir = tempfile::tempdir().unwrap();
    let memory_root = memory_dir.path().join("memory");
    std::fs::create_dir_all(memory_root.join("L3")).unwrap();
    std::fs::write(
        memory_root.join("L3/profile.md"),
        "# Student profile\n\n- Learns best from worked examples.",
    )
    .unwrap();
    let sink = Arc::new(TraceRecorder::default());
    let responses = vec![
        MockResponse::tool_use("use-memory", "read_memory", r#"{"scope":"profile"}"#),
        MockResponse::text("I will adapt the next explanation to your profile."),
    ];
    let router = make_router(responses, make_governance(None))
        .with_memory_root(memory_root)
        .with_event_sink(sink.clone());
    let answer = router
        .run(Capability::Chat, "review this based on my profile")
        .await
        .unwrap();
    assert!(answer.contains("profile"));
    assert!(sink.events().iter().any(|(kind, data)| {
        kind == "tool_result"
            && data["tool"] == "read_memory"
            && data["details"]["empty"] == false
            && data["details"]["files"][0] == "L3/profile.md"
    }));
}

#[tokio::test]
async fn chat_returns_runtime_final_answer_not_progress_text() {
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![
            progress_text_response("checking context first"),
            MockResponse::text("final answer only"),
        ],
        make_governance(None),
    )
    .with_event_sink(sink.clone());

    let answer = router
        .run(Capability::Chat, "answer after progress")
        .await
        .unwrap();

    assert_eq!(answer, "final answer only");
    let events = sink.events();
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "assistant_progress"
                && data["summary"]
                    .as_str()
                    .is_some_and(|text| text.contains("checking context first"))
        }),
        "missing runtime progress trace: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "final_answer" && data["capability"] == "chat" }),
        "missing runtime final answer trace: {events:?}"
    );
}

#[tokio::test]
async fn code_exec_runs_tool_and_explains_result() {
    let dir = TempDir::new().unwrap();
    let responses = vec![
        MockResponse::tool_use(
            "exec-1",
            "code_exec",
            r#"{"language":"python","code":"print('hello code exec')"}"#,
        ),
        MockResponse::text("The script printed hello code exec."),
    ];
    let router = make_router_with_env(
        responses,
        make_governance(None),
        Arc::new(OsEnv::new(dir.path())) as Arc<dyn ExecutionEnv>,
    );
    let answer = router
        .run(Capability::CodeExec, "run python that prints hello")
        .await
        .unwrap();
    assert!(answer.contains("hello code exec"));
}

#[tokio::test]
async fn code_exec_returns_runtime_final_answer_not_progress_text() {
    let dir = TempDir::new().unwrap();
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router_with_env(
        vec![
            progress_text_response("checking code execution plan"),
            MockResponse::text("final code answer"),
        ],
        make_governance(None),
        Arc::new(OsEnv::new(dir.path())) as Arc<dyn ExecutionEnv>,
    )
    .with_event_sink(sink.clone());

    let answer = router
        .run(Capability::CodeExec, "answer after code progress")
        .await
        .unwrap();

    assert_eq!(answer, "final code answer");
    let events = sink.events();
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "assistant_progress"
                && data["summary"]
                    .as_str()
                    .is_some_and(|text| text.contains("checking code execution plan"))
        }),
        "missing runtime progress trace: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "final_answer" && data["capability"] == "code_exec" }),
        "missing runtime final answer trace: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "runtime_usage" && data["capability"] == "code_exec" }),
        "missing runtime usage trace: {events:?}"
    );
}

#[tokio::test]
async fn chat_emits_trace_events() {
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![MockResponse::text("traced answer")],
        make_governance(None),
    )
    .with_event_sink(sink.clone());

    router.run(Capability::Chat, "trace this").await.unwrap();

    let events = sink.events();
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "phase_start" && data["capability"] == "chat" }),
        "missing chat phase_start trace: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "phase_end" && data["capability"] == "chat" }),
        "missing chat phase_end trace: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "runtime_usage" && data["capability"] == "chat" }),
        "missing chat runtime_usage trace: {events:?}"
    );
}

#[tokio::test]
async fn research_emits_research_trace_events() {
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![MockResponse::text(
            "# Report\n\n## Summary\n\nShort summary.\n\n## Key Findings\n\n- Finding.\n\n## Analysis\n\nAnalysis.\n\n## Limitations\n\nLimited.\n\n## Follow-up Questions\n\n- Next?\n\n## Sources\n\n[1] Mock source",
        )],
        make_governance(None),
    )
    .with_event_sink(sink.clone());

    let answer = router
        .run(Capability::Research, "research a topic")
        .await
        .unwrap();
    assert!(answer.contains("Report"));

    let events = sink.events();
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "research_stage_start" && data["capability"] == "research"
        }),
        "missing research stage event: {events:?}"
    );
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "research_report_done" && data["capability"] == "research"
        }),
        "missing research report event: {events:?}"
    );
}

#[tokio::test]
async fn research_clarification_uses_chat_fallback_without_report_event() {
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![MockResponse::text(
            "Sure. What scope, time range, and output format should I use?",
        )],
        make_governance(None),
    )
    .with_event_sink(sink.clone());

    let answer = router
        .run(
            Capability::Research,
            "hi, can you help me research something?",
        )
        .await
        .unwrap();

    assert!(answer.contains("scope"));
    let events = sink.events();
    assert!(
        events
            .iter()
            .any(|(kind, data)| { kind == "final_answer" && data["capability"] == "research" }),
        "missing research final answer trace: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|(kind, _)| { kind == "research_report_done" }),
        "clarification should not be reported as a research report: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|(kind, data)| { kind == "tool_call" && data["tool"] == "web_search" }),
        "clarification should not call web_search: {events:?}"
    );
}

#[tokio::test]
async fn research_greeting_falls_back_to_unstreamed_text_delta() {
    let router = make_router(
        vec![text_delta_end_turn_response(
            "你好！我可以先帮你明确调研目标。",
        )],
        make_governance(None),
    );

    let answer = router.run(Capability::Research, "你好").await.unwrap();

    assert!(answer.contains("你好"));
}

#[tokio::test]
async fn research_explicit_start_enters_search_path() {
    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![
            MockResponse::text(
                r#"{"queries":["agent research workflow"],"source_candidates":[{"title":"Mock","url":"https://example.test","snippet":"Mock source"}],"failures":[]}"#,
            ),
            MockResponse::text(
                r#"{"sources":[{"title":"Mock","url":"https://example.test","summary":"Mock source summary","used_for":"workflow architecture"}],"failures":[]}"#,
            ),
            MockResponse::text(r#"{"verdict":"pass","issues":[]}"#),
            MockResponse::text(
                r##"{"markdown":"# Report\n\n## Summary\n\nSearched the topic.\n\n## Key Findings\n\n- Finding. [1]\n\n## Analysis\n\nAnalysis.\n\n## Limitations\n\nLimited.\n\n## Follow-up Questions\n\n- Next?\n\n## Sources\n\n[1] Mock - https://example.test","sources":[{"title":"Mock","url":"https://example.test"}]}"##,
            ),
        ],
        make_governance(None),
    )
    .with_event_sink(sink.clone());

    let answer = run_research_workflow_with_runtime(
        &router,
        ResearchWorkflowInput {
            request: "Start the detailed research workflow for agent research workflow.".into(),
        },
        None,
        None,
        None,
    )
    .await
    .unwrap()
    .markdown;

    assert!(answer.contains("Report"));
    let events = sink.events();
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "workflow_validated"
                && data["capability"] == "research"
                && data["workflow"] == "tutor.research"
        }),
        "explicit research start should enter runtime workflow: {events:?}"
    );
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "research_stage_start"
                && data["capability"] == "research"
                && data["stage"] == "search"
        }),
        "explicit research start should emit search stage: {events:?}"
    );
    assert!(
        events
            .iter()
            .any(|(kind, _)| { kind == "research_report_done" }),
        "explicit research start should emit report done: {events:?}"
    );
}

#[tokio::test]
async fn research_workflow_uses_runtime_knowledge_and_refreshes_final_citations() {
    let dir = TempDir::new().unwrap();
    let rag = LanceDbRag::new(dir.path().join("rag"), hash_embedding_config());
    rag.ingest_text(
        "kb-a",
        "document-a::Newton notes",
        &"Newton's laws describe motion and force. ".repeat(16),
    )
    .await
    .unwrap();

    let authority =
        Arc::new(EvidenceAuthority::new(vec![7; 32], [course_evidence_provider_id()]).unwrap());
    let knowledge_runtime =
        assemble_course_knowledge(LanceDbKnowledgeSource::new(rag, "kb-a"), authority).unwrap();
    let access = knowledge_access("kb-a");
    let mut knowledge_tools = Vec::new();
    knowledge_runtime
        .plugin()
        .register_tools(&mut knowledge_tools);
    let search = knowledge_tools
        .iter()
        .find(|tool| tool.name() == KNOWLEDGE_SEARCH_TOOL_NAME)
        .unwrap();
    let setup_context =
        tool_context(RunRequest::from_text("Newton").with_extension(access.clone()));
    let search_result = search
        .execute(serde_json::json!({"query": "Newton"}), &setup_context)
        .await
        .unwrap();
    let reference = search_result.details["hits"][0]["reference"].clone();
    let selector = search_result.details["hits"][0]["suggested_selectors"][0].clone();
    let read_args = serde_json::json!({
        "reference": reference,
        "selector": selector,
    })
    .to_string();

    let client = Arc::new(ResearchKnowledgeMockClient::new(reference, read_args));
    let router = CapabilityRouter::new(
        Arc::new(NoOpEnv),
        LlmConfig::anthropic("mock-model", ""),
        make_governance(None),
    )
    .with_client(client.clone())
    .with_knowledge_runtime(knowledge_runtime)
    .with_associated_kb("kb-a")
    .with_workflow_root(dir.path().join("workflow-sessions"));

    let report = run_research_workflow_with_runtime(
        &router,
        ResearchWorkflowInput {
            request: "Research Newton's laws using the selected course knowledge.".into(),
        },
        None,
        None,
        Some(access),
    )
    .await
    .unwrap();

    assert!(report.markdown.contains("Newton Report"));
    assert!(report.markdown.contains("[K:"));
    assert_eq!(client.call_count.load(Ordering::SeqCst), 7);
}

#[tokio::test]
async fn research_workflow_accepts_confirmed_chinese_context() {
    let dir = TempDir::new().unwrap();
    let repo = JsonlSessionRepo::new(dir.path().join("sessions"));
    let storage = repo.create(CreateSessionOptions::default()).await.unwrap();
    let session = Session::new(storage);
    session
        .append_message(user_message("帮我调研一下transformer架构，我想学习"))
        .await
        .unwrap();
    session
        .append_message(assistant_message(
            "这是调研计划。确认后我就启动详细研究工作流程。",
        ))
        .await
        .unwrap();

    let sink = Arc::new(TraceRecorder::default());
    let router = make_router(
        vec![
            MockResponse::text(
                r#"{"queries":["Transformer architecture"],"source_candidates":[{"title":"Mock","url":"https://example.test","snippet":"Mock source"}],"failures":[]}"#,
            ),
            MockResponse::text(
                r#"{"sources":[{"title":"Mock","url":"https://example.test","summary":"Mock source summary","used_for":"Transformer architecture"}],"failures":[]}"#,
            ),
            MockResponse::text(r#"{"verdict":"pass","issues":[]}"#),
            MockResponse::text(
                r##"{"markdown":"# Transformer Report\n\n## Summary\n\nConfirmed Chinese request.\n\n## Key Findings\n\n- Finding. [1]\n\n## Analysis\n\nAnalysis.\n\n## Limitations\n\nLimited.\n\n## Follow-up Questions\n\n- Next?\n\n## Sources\n\n[1] Mock - https://example.test","sources":[{"title":"Mock","url":"https://example.test"}]}"##,
            ),
        ],
        make_governance(None),
    )
    .with_event_sink(sink.clone())
    .with_workflow_root(dir.path().join("workflow-sessions"));

    let answer = run_research_workflow_with_runtime(
        &router,
        ResearchWorkflowInput {
            request: "Conversation context:\nUser: 帮我调研一下transformer架构，我想学习\n\nAssistant: 这是调研计划。确认后我就启动详细研究工作流程。\n\nConfirmed research instruction:\n可以".into(),
        },
        Some(session),
        None,
        None,
    )
        .await
        .unwrap()
        .markdown;

    assert!(answer.contains("Transformer Report"));
    let events = sink.events();
    assert!(
        events.iter().any(|(kind, data)| {
            kind == "workflow_validated"
                && data["capability"] == "research"
                && data["workflow"] == "tutor.research"
        }),
        "Chinese confirmation should enter runtime workflow: {events:?}"
    );
}

#[tokio::test]
async fn research_workflow_persists_final_report_to_session() {
    let dir = TempDir::new().unwrap();
    let repo = JsonlSessionRepo::new(dir.path().join("sessions"));
    let storage = repo.create(CreateSessionOptions::default()).await.unwrap();
    let session = Session::new(storage.clone());
    let inspect_session = Session::new(storage);
    let router = make_router(
        vec![
            MockResponse::text(
                r#"{"queries":["agent research workflow"],"source_candidates":[{"title":"Mock","url":"https://example.test","snippet":"Mock source"}],"failures":[]}"#,
            ),
            MockResponse::text(
                r#"{"sources":[{"title":"Mock","url":"https://example.test","summary":"Mock source summary","used_for":"workflow architecture"}],"failures":[]}"#,
            ),
            MockResponse::text(r#"{"verdict":"pass","issues":[]}"#),
            MockResponse::text(
                r##"{"markdown":"# Report\n\n## Summary\n\nPersisted report.\n\n## Key Findings\n\n- Finding. [1]\n\n## Analysis\n\nAnalysis.\n\n## Limitations\n\nLimited.\n\n## Follow-up Questions\n\n- Next?\n\n## Sources\n\n[1] Mock - https://example.test","sources":[{"title":"Mock","url":"https://example.test"}]}"##,
            ),
        ],
        make_governance(None),
    )
    .with_workflow_root(dir.path().join("workflow-sessions"));

    let answer = run_research_workflow_with_runtime(
        &router,
        ResearchWorkflowInput {
            request: "Start the detailed research workflow for agent research workflow.".into(),
        },
        Some(session),
        None,
        None,
    )
    .await
    .unwrap()
    .markdown;
    assert!(answer.contains("Persisted report"));

    let context = inspect_session.build_context().await.unwrap();
    assert!(
        context.messages.iter().any(|message| matches!(
            message,
            AgentMessage::Assistant(assistant)
                if assistant.kind == AssistantMessageKind::FinalAnswer
                    && assistant.text_content().contains("Persisted report")
        )),
        "research workflow report should be stored as FinalAnswer: {:?}",
        context.messages
    );
}

#[tokio::test]
async fn research_workflow_fails_clearly_after_citation_repair_attempt() {
    let router = make_router(
        vec![
            MockResponse::text(
                r#"{"queries":["agent research workflow"],"source_candidates":[],"failures":["search unavailable"]}"#,
            ),
            MockResponse::text(r#"{"sources":[],"failures":["no sources to fetch"]}"#),
            MockResponse::text(
                r#"{"verdict":"fail","issues":["no verified sources"],"repair":"search"}"#,
            ),
            MockResponse::text(
                r#"{"queries":["agent research workflow retry"],"source_candidates":[],"failures":["search still unavailable"]}"#,
            ),
            MockResponse::text(r#"{"sources":[],"failures":["fetch still unavailable"]}"#),
            MockResponse::text(
                r#"{"verdict":"fail","issues":["citations still unverified"],"repair":"search"}"#,
            ),
        ],
        make_governance(None),
    );

    let err = run_research_workflow_with_runtime(
        &router,
        ResearchWorkflowInput {
            request: "Start the detailed research workflow for agent research workflow.".into(),
        },
        None,
        None,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string()
            .contains("research citation check failed after repair attempt"),
        "research workflow failure should explain the failed repair attempt: {err}"
    );
}
