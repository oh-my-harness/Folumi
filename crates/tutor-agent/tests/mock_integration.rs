use std::sync::Arc;
use std::sync::Mutex;

use async_trait::async_trait;
use futures::{future::BoxFuture, stream};
use llm_adapter::{
    ChatRequest, ChatResponse, Message, Provider, ProviderCapabilities, RequestContent,
    StreamHandle,
    types::{ContentKind, StopReason, StreamEvent, Usage, UsageProvenance},
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
    AssistantMessage, AssistantMessageKind, DataBlock, ExecutionEnv, RunContext, RunRequest, Tool,
    ToolContext, ToolFailure, ToolResult,
};
use tempfile::TempDir;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tutor_agent::capability::Capability;
use tutor_agent::event_sink::EventSink;
use tutor_agent::governance::GovernanceConfig;
use tutor_agent::{
    CapabilityRouter, LlmConfig, agent_knowledge_evidence_provider_id, assemble_course_knowledge,
};
use tutor_rag::{EmbeddingConfig, LanceDbKnowledgeSource, LanceDbRag};

fn make_governance(audit: Option<Arc<dyn AuditSink>>) -> GovernanceConfig {
    GovernanceConfig::new(audit, false)
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
    final_usage: Usage,
}

impl CitationEchoMockClient {
    fn new(responses: Vec<MockResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
            final_usage: Usage::default(),
        }
    }

    fn with_final_usage(mut self, final_usage: Usage) -> Self {
        self.final_usage = final_usage;
        self
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
                text_delta_end_turn_response_with_usage(
                    &format!(
                        "Newton's laws are grounded in the selected course evidence. {handle}"
                    ),
                    self.final_usage.clone(),
                )
            } else {
                responses.remove(0)
            }
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

fn text_delta_end_turn_response_with_usage(text: &str, usage: Usage) -> MockResponse {
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
                usage,
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

    let authority = Arc::new(
        EvidenceAuthority::new(vec![7; 32], [agent_knowledge_evidence_provider_id()]).unwrap(),
    );
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
    let client = Arc::new(
        CitationEchoMockClient::new(vec![
            MockResponse::tool_use(
                "knowledge-search",
                KNOWLEDGE_SEARCH_TOOL_NAME,
                r#"{"query":"Newton"}"#,
            ),
            MockResponse::tool_use("knowledge-read", KNOWLEDGE_READ_TOOL_NAME, &read_args),
        ])
        .with_final_usage(Usage {
            input_tokens: 240,
            output_tokens: 36,
            cached_input_tokens: 12,
            cache_creation_input_tokens: 8,
            reasoning_tokens: 0,
            provenance: UsageProvenance::ReportedValid,
        }),
    );
    let router = CapabilityRouter::new(
        Arc::new(NoOpEnv),
        LlmConfig::anthropic("mock-model", ""),
        make_governance(None),
    )
    .with_client(client)
    .with_knowledge_runtime(knowledge_runtime)
    .with_event_sink(sink.clone());

    let sessions_root = dir.path().join("sessions");
    let repo = JsonlSessionRepo::new(&sessions_root);
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
    let runtime_usage = events
        .iter()
        .find_map(|(kind, data)| (kind == "runtime_usage").then_some(data))
        .expect("knowledge run should emit provider-reported usage");
    assert_eq!(runtime_usage["input_tokens"], 240);
    assert_eq!(runtime_usage["output_tokens"], 36);
    assert_eq!(runtime_usage["cache_read_tokens"], 12);
    assert_eq!(runtime_usage["cache_write_tokens"], 8);

    let context = inspect_session.build_context().await.unwrap();
    let persisted_context = format!("{:?}", context.messages);
    assert!(
        !persisted_context.contains(private_tail),
        "knowledge_read body leaked into durable Session context: {persisted_context}"
    );

    let persisted_session_dir = std::fs::read_dir(&sessions_root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.is_dir())
        .expect("runtime session directory should exist");
    let mut persisted_bytes = 0_u64;
    let mut persisted_text = String::new();
    for entry in std::fs::read_dir(&persisted_session_dir)
        .unwrap()
        .filter_map(Result::ok)
    {
        let path = entry.path();
        if path.is_file() {
            persisted_bytes += entry.metadata().unwrap().len();
            if let Ok(text) = std::fs::read_to_string(path) {
                persisted_text.push_str(&text);
            }
        }
    }
    assert!(
        !persisted_text.contains(private_tail),
        "knowledge_read body leaked into raw durable Session files"
    );
    println!(
        "{}",
        serde_json::json!({
            "provider": "deterministic_mock",
            "input_tokens": 240,
            "output_tokens": 36,
            "cache_read_tokens": 12,
            "cache_write_tokens": 8,
            "durable_session_bytes": persisted_bytes,
            "read_body_sentinel_persisted": false,
        })
    );
}

#[tokio::test]
async fn chat_rejects_a_valid_knowledge_citation_reused_across_runs() {
    let dir = TempDir::new().unwrap();
    let rag = LanceDbRag::new(dir.path().join("rag"), hash_embedding_config());
    rag.ingest_text(
        "kb-a",
        "document-a::Newton notes",
        "Newton's laws describe motion and force.",
    )
    .await
    .unwrap();

    let authority = Arc::new(
        EvidenceAuthority::new(vec![7; 32], [agent_knowledge_evidence_provider_id()]).unwrap(),
    );
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
    let read = knowledge_tools
        .iter()
        .find(|tool| tool.name() == KNOWLEDGE_READ_TOOL_NAME)
        .unwrap();
    let issued_context =
        tool_context(RunRequest::from_text("Newton").with_extension(access.clone()));
    let search_result = search
        .execute(serde_json::json!({"query": "Newton"}), &issued_context)
        .await
        .unwrap();
    let read_result = read
        .execute(
            serde_json::json!({
                "reference": search_result.details["hits"][0]["reference"].clone(),
                "selector": search_result.details["hits"][0]["suggested_selectors"][0].clone(),
            }),
            &issued_context,
        )
        .await
        .unwrap();
    let issued_handle = read_result.details["citation"]["handle"]
        .as_str()
        .expect("knowledge read should issue a citation handle")
        .to_string();

    let router = CapabilityRouter::new(
        Arc::new(NoOpEnv),
        LlmConfig::anthropic("mock-model", ""),
        make_governance(None),
    )
    .with_client(Arc::new(MockLlmClient::new(vec![MockResponse::text(
        &format!("Reused evidence {issued_handle}"),
    )])))
    .with_knowledge_runtime(knowledge_runtime);

    let error = router
        .run_request(
            Capability::Chat,
            RunRequest::from_text("Reuse a prior citation").with_extension(access),
        )
        .await
        .unwrap_err();

    assert!(
        error
            .to_string()
            .contains("citation that was not issued in this run"),
        "cross-run citation should be rejected at the final-answer boundary: {error}"
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
