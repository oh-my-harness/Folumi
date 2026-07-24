use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use llm_harness_runtime_knowledge::{
    EvidenceAuthority, KnowledgeAccessContext, KnowledgeScope, PrincipalRef,
};
use llm_harness_runtime_sandbox_os::OsEnv;
use llm_harness_types::ExecutionEnv;
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

use crate::knowledge_store::KnowledgeStore;
use crate::memory_store::{MemoryEventCategory, MemoryStore};
use crate::notebook_store::NotebookStore;
use crate::quiz_store::{
    QuizCitation, QuizConfig, QuizDifficulty, QuizOption, QuizQuestion, QuizQuestionType,
    QuizSession, QuizStore, QuizVerificationReport, QuizVerificationStatus,
};

#[derive(Clone)]
pub(crate) struct QuizState {
    pub(crate) store: Arc<QuizStore>,
    pub(crate) knowledge: Arc<KnowledgeStore>,
    pub(crate) notebook: Arc<NotebookStore>,
    pub(crate) memory: Arc<MemoryStore>,
    pub(crate) evidence_authority: Arc<EvidenceAuthority>,
    pub(crate) rag_root: PathBuf,
    pub(crate) workflow_root: PathBuf,
}

#[derive(Deserialize)]
pub(crate) struct CreateQuizRequest {
    pub(crate) title: Option<String>,
    pub(crate) kb_id: Option<String>,
    pub(crate) notebook_entry_id: Option<String>,
    pub(crate) source_text: Option<String>,
    pub(crate) source_label: Option<String>,
    pub(crate) topic: Option<String>,
    pub(crate) difficulty: Option<QuizDifficulty>,
    pub(crate) question_count: Option<usize>,
    pub(crate) llm: Option<CreateLlmConfig>,
    #[serde(skip)]
    pub(crate) knowledge_access: Option<KnowledgeAccessContext>,
}

#[derive(Debug, Deserialize)]
struct SubmitAnswerRequest {
    question_id: String,
    selected_option_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct CreateLlmConfig {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) api_key: Option<String>,
    pub(crate) base_url: Option<String>,
    pub(crate) chat_path: Option<String>,
}

async fn list_quizzes(State(state): State<QuizState>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "quizzes": state.store.list() })),
    )
}

async fn create_quiz(
    State(state): State<QuizState>,
    Json(req): Json<CreateQuizRequest>,
) -> impl IntoResponse {
    match create_quiz_for_request(&state, req).await {
        Ok(quiz) => (
            StatusCode::CREATED,
            Json(serde_json::json!({ "quiz": quiz })),
        )
            .into_response(),
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

pub(crate) async fn create_quiz_for_request(
    state: &QuizState,
    req: CreateQuizRequest,
) -> anyhow::Result<QuizSession> {
    let use_memory = should_use_memory_for_quiz(&req);
    let mut source_text = req
        .source_text
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut source_label = req
        .source_label
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("conversation")
        .to_string();
    let notebook_entry_id = req
        .notebook_entry_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    if let Some(entry_id) = &notebook_entry_id {
        let Some(entry) = state.notebook.get(entry_id) else {
            anyhow::bail!("notebook entry not found");
        };
        source_text = Some(entry.markdown);
        source_label = format!("notebook: {}", entry.title);
    }
    let kb_id = req
        .kb_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if kb_id.is_none() && source_text.is_none() {
        anyhow::bail!("quiz requires either kb_id or source_text");
    }

    let config = QuizConfig {
        topic: req.topic.clone(),
        difficulty: req.difficulty.unwrap_or(QuizDifficulty::Medium),
        question_count: req.question_count.unwrap_or(5).clamp(1, 10),
        question_type: QuizQuestionType::SingleChoice,
    };

    let quiz = match state.store.create(
        req.title.unwrap_or_default(),
        kb_id.clone().unwrap_or_else(|| {
            if notebook_entry_id.is_some() {
                "__notebook__".into()
            } else {
                "__conversation__".into()
            }
        }),
        config,
    ) {
        Ok(quiz) => quiz,
        Err(err) => anyhow::bail!("{err}"),
    };

    let memory_markdown = if use_memory {
        quiz_memory_markdown(&state.memory).ok()
    } else {
        None
    };

    match generate_questions(
        state,
        quiz,
        req.llm,
        source_text,
        source_label,
        memory_markdown.clone(),
        req.knowledge_access,
    )
    .await
    {
        Ok(quiz) => {
            let _ = state.memory.record_event(
                MemoryEventCategory::Quiz,
                "created",
                format!(
                    "Generated quiz: {} ({} questions)",
                    quiz.title,
                    quiz.questions.len()
                ),
                Some(quiz.id.clone()),
                serde_json::json!({
                    "kb_id": quiz.kb_id,
                    "topic": quiz.config.topic,
                    "difficulty": quiz.config.difficulty,
                    "question_count": quiz.questions.len(),
                    "notebook_entry_id": notebook_entry_id,
                    "memory_used": memory_markdown.as_deref().map(|value| !value.trim().is_empty()).unwrap_or(false),
                }),
            );
            Ok(quiz)
        }
        Err(err) => Err(err),
    }
}

async fn get_quiz(State(state): State<QuizState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.store.get(&id) {
        Some(quiz) => (StatusCode::OK, Json(serde_json::json!({ "quiz": quiz }))).into_response(),
        None => error_response(StatusCode::NOT_FOUND, "quiz not found"),
    }
}

async fn submit_answer(
    State(state): State<QuizState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitAnswerRequest>,
) -> impl IntoResponse {
    match state
        .store
        .submit_answer(&id, &req.question_id, &req.selected_option_id)
    {
        Ok(quiz) => {
            let answer = quiz
                .answers
                .iter()
                .rev()
                .find(|answer| answer.question_id == req.question_id);
            let question = quiz
                .questions
                .iter()
                .find(|question| question.id == req.question_id);
            let _ = state.memory.record_event(
                MemoryEventCategory::Quiz,
                "answered",
                format!(
                    "Answered quiz question {} in {}: {}",
                    req.question_id,
                    quiz.title,
                    answer
                        .map(|answer| if answer.correct {
                            "correct"
                        } else {
                            "incorrect"
                        })
                        .unwrap_or("submitted")
                ),
                Some(quiz.id.clone()),
                serde_json::json!({
                    "question_id": req.question_id,
                    "selected_option_id": req.selected_option_id,
                    "correct": answer.map(|answer| answer.correct),
                    "tags": question.map(|question| question.tags.clone()).unwrap_or_default(),
                }),
            );
            (StatusCode::OK, Json(serde_json::json!({ "quiz": quiz }))).into_response()
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn finish_quiz(State(state): State<QuizState>, Path(id): Path<String>) -> impl IntoResponse {
    match state.store.finish(&id) {
        Ok(quiz) => {
            let score = quiz.score.clone();
            let _ = state.memory.record_event(
                MemoryEventCategory::Quiz,
                "finished",
                format!(
                    "Finished quiz: {} ({}/{})",
                    quiz.title,
                    score.as_ref().map(|score| score.correct).unwrap_or(0),
                    score
                        .as_ref()
                        .map(|score| score.total)
                        .unwrap_or(quiz.questions.len())
                ),
                Some(quiz.id.clone()),
                serde_json::json!({ "score": score }),
            );
            (StatusCode::OK, Json(serde_json::json!({ "quiz": quiz }))).into_response()
        }
        Err(err) => error_response(StatusCode::BAD_REQUEST, err),
    }
}

async fn delete_quiz(State(state): State<QuizState>, Path(id): Path<String>) -> impl IntoResponse {
    if state.store.delete(&id) {
        StatusCode::NO_CONTENT.into_response()
    } else {
        error_response(StatusCode::NOT_FOUND, "quiz not found")
    }
}

pub fn quiz_router(
    store: Arc<QuizStore>,
    knowledge: Arc<KnowledgeStore>,
    notebook: Arc<NotebookStore>,
    memory: Arc<MemoryStore>,
    rag_root: impl Into<PathBuf>,
    workflow_root: impl Into<PathBuf>,
) -> Router {
    quiz_router_with_rag_root(store, knowledge, notebook, memory, rag_root, workflow_root)
}

fn quiz_router_with_rag_root(
    store: Arc<QuizStore>,
    knowledge: Arc<KnowledgeStore>,
    notebook: Arc<NotebookStore>,
    memory: Arc<MemoryStore>,
    rag_root: impl Into<PathBuf>,
    workflow_root: impl Into<PathBuf>,
) -> Router {
    let state = QuizState {
        store,
        knowledge,
        notebook,
        memory,
        evidence_authority: crate::knowledge_runtime::course_evidence_authority(),
        rag_root: rag_root.into(),
        workflow_root: workflow_root.into(),
    };
    Router::new()
        .route("/api/quizzes", get(list_quizzes).post(create_quiz))
        .route("/api/quizzes/{id}", get(get_quiz).delete(delete_quiz))
        .route("/api/quizzes/{id}/answers", post(submit_answer))
        .route("/api/quizzes/{id}/finish", post(finish_quiz))
        .with_state(state)
}

async fn generate_questions(
    state: &QuizState,
    quiz: QuizSession,
    llm: Option<CreateLlmConfig>,
    source_text: Option<String>,
    source_label: String,
    memory_markdown: Option<String>,
    knowledge_access: Option<KnowledgeAccessContext>,
) -> anyhow::Result<QuizSession> {
    if let Some(source_text) = source_text {
        let hits = source_hits_from_text(&quiz, &source_label, &source_text);
        let (questions, verification_method) =
            questions_for_hits(state, &quiz, llm, &hits, memory_markdown).await?;
        validate_quiz_questions_for_storage(&questions)?;
        let quiz = state.store.replace_questions(&quiz.id, questions)?;
        return state.store.set_verification(
            &quiz.id,
            quiz_verification_report(verification_method, Vec::new()),
        );
    }

    let Some(kb) = state.knowledge.get(&quiz.kb_id) else {
        anyhow::bail!("knowledge base not found");
    };
    if kb.documents.is_empty() {
        anyhow::bail!("knowledge base has no documents");
    }

    let query = quiz
        .config
        .topic
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&kb.name);
    let rag = tutor_rag::LanceDbRag::new(state.rag_root.clone(), kb.embedding);
    let runtime = tutor_agent::assemble_course_knowledge(
        tutor_rag::LanceDbKnowledgeSource::new(rag, &quiz.kb_id),
        state.evidence_authority.clone(),
    )?;
    let access =
        knowledge_access.unwrap_or_else(|| quiz_knowledge_access_context(&quiz.id, &quiz.kb_id));
    let verified = runtime
        .collect_verified_chunks(access, query, 10, CancellationToken::new())
        .await?;
    let hits = verified
        .into_iter()
        .map(|chunk| tutor_rag::SearchHit {
            id: chunk
                .metadata
                .get("chunk_id")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(&chunk.reference.item_id)
                .to_string(),
            kb: quiz.kb_id.clone(),
            source: chunk.title.clone(),
            raw_source: chunk.uri.unwrap_or_else(|| chunk.title.clone()),
            document_id: chunk
                .metadata
                .get("document_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            text: chunk.text,
            score: chunk.score,
        })
        .collect::<Vec<_>>();
    if hits.is_empty() {
        anyhow::bail!("no source chunks found for quiz generation");
    }
    let (questions, verification_method) =
        questions_for_hits(state, &quiz, llm, &hits, memory_markdown).await?;
    validate_quiz_questions_for_storage(&questions)?;
    let quiz = state.store.replace_questions(&quiz.id, questions)?;
    state.store.set_verification(
        &quiz.id,
        quiz_verification_report(verification_method, Vec::new()),
    )
}

fn quiz_knowledge_access_context(quiz_id: &str, kb_id: &str) -> KnowledgeAccessContext {
    let mut scope = KnowledgeScope::new(tutor_rag::COURSE_KNOWLEDGE_NAMESPACE);
    scope.project = Some(quiz_id.to_string());
    scope.attributes.insert(
        tutor_rag::KNOWLEDGE_BASE_SCOPE_ATTRIBUTE.into(),
        kb_id.to_string(),
    );
    let mut access =
        KnowledgeAccessContext::new(scope, PrincipalRef::new("local-user", "local_user"));
    access.authorization_version = Some("local-user:quiz-knowledge:v1".into());
    access
}

async fn questions_for_hits(
    state: &QuizState,
    quiz: &QuizSession,
    llm: Option<CreateLlmConfig>,
    hits: &[tutor_rag::SearchHit],
    memory_markdown: Option<String>,
) -> anyhow::Result<(Vec<QuizQuestion>, &'static str)> {
    if let Some(llm) = llm.and_then(llm_config_from_request) {
        let sources = hits
            .iter()
            .map(|hit| tutor_agent::quiz::QuizSourceChunk {
                source: hit.source.clone(),
                text: hit.text.clone(),
                score: hit.score,
            })
            .collect::<Vec<_>>();
        let cwd = std::env::current_dir()?;
        let env = Arc::new(OsEnv::new(cwd)) as Arc<dyn ExecutionEnv>;
        let client = llm.build_client();
        let engine_config = tutor_agent::runtime_engine::build_workflow_engine_config(
            client,
            llm.model.clone(),
            env,
            state.workflow_root.join("quiz"),
        );
        let generated = tutor_agent::quiz::generate_quiz_questions_with_workflow(
            &tutor_agent::quiz::QuizGenerationConfig {
                topic: quiz.config.topic.clone(),
                difficulty: format!("{:?}", quiz.config.difficulty).to_ascii_lowercase(),
                question_count: quiz.config.question_count,
                memory_markdown,
            },
            &sources,
            engine_config,
        )
        .await?;
        Ok((
            questions_from_generated(&quiz.config, hits, generated.questions),
            "llm_verifier_and_citation_check",
        ))
    } else {
        Ok((
            questions_from_hits(&quiz.config, hits),
            "deterministic_fallback_citation_check",
        ))
    }
}

fn source_hits_from_text(
    quiz: &QuizSession,
    source_label: &str,
    source_text: &str,
) -> Vec<tutor_rag::SearchHit> {
    split_source_text(source_text)
        .into_iter()
        .enumerate()
        .map(|(index, text)| tutor_rag::SearchHit {
            id: format!("conversation-{index}"),
            kb: quiz.kb_id.clone(),
            source: source_label.to_string(),
            raw_source: source_label.to_string(),
            document_id: None,
            text,
            score: None,
        })
        .collect()
}

fn split_source_text(source_text: &str) -> Vec<String> {
    let normalized = source_text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= 1800 {
        return vec![normalized];
    }

    let chars = normalized.chars().collect::<Vec<_>>();
    chars
        .chunks(1800)
        .take(12)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect()
}

fn llm_config_from_request(config: CreateLlmConfig) -> Option<tutor_agent::LlmConfig> {
    let api_key = config.api_key?.trim().to_string();
    if api_key.is_empty() || config.model.trim().is_empty() {
        return None;
    }
    let provider = match config.provider.trim().to_ascii_lowercase().as_str() {
        "anthropic" | "claude" => tutor_agent::LlmProviderKind::Anthropic,
        "deepseek" => tutor_agent::LlmProviderKind::DeepSeek,
        "openai" | "openai-compatible" => tutor_agent::LlmProviderKind::OpenAI,
        _ => return None,
    };
    Some(tutor_agent::LlmConfig::from_parts(
        provider,
        config.model.trim().to_string(),
        api_key,
        config.base_url.filter(|value| !value.trim().is_empty()),
        config.chat_path.filter(|value| !value.trim().is_empty()),
        None,
    ))
}

fn should_use_memory_for_quiz(req: &CreateQuizRequest) -> bool {
    let text = [
        req.title.as_deref().unwrap_or_default(),
        req.topic.as_deref().unwrap_or_default(),
        req.source_label.as_deref().unwrap_or_default(),
    ]
    .join(" ")
    .to_ascii_lowercase();
    let indicators = [
        "personalized",
        "personalised",
        "adaptive",
        "review",
        "practice",
        "follow-up",
        "follow up",
        "weak",
        "weakness",
        "mistake",
        "wrong",
        "\u{4e2a}\u{6027}\u{5316}",
        "\u{590d}\u{4e60}",
        "\u{7ec3}\u{4e60}",
        "\u{9519}\u{9898}",
        "\u{8584}\u{5f31}",
        "\u{5f31}\u{70b9}",
        "\u{8ddf}\u{8fdb}",
        "\u{5dee}\u{5f02}\u{5316}",
    ];
    indicators.iter().any(|indicator| text.contains(indicator))
}

fn quiz_memory_markdown(memory: &MemoryStore) -> anyhow::Result<String> {
    let mut sections = Vec::new();
    for path in ["L3/profile.md", "L3/recent.md", "L3/teaching_strategy.md"] {
        let file = memory.read(path)?;
        let markdown = file.markdown.trim();
        if !markdown.is_empty() {
            sections.push(format!("## {path}\n\n{markdown}"));
        }
    }
    Ok(sections.join("\n\n"))
}

fn questions_from_generated(
    config: &QuizConfig,
    hits: &[tutor_rag::SearchHit],
    generated: Vec<tutor_agent::quiz::GeneratedQuizQuestion>,
) -> Vec<QuizQuestion> {
    generated
        .into_iter()
        .enumerate()
        .map(|(index, question)| {
            let citations = question
                .citation_indices
                .iter()
                .filter_map(|source_index| hits.get(*source_index))
                .map(|hit| QuizCitation {
                    source: hit.source.clone(),
                    text: citation_excerpt(&hit.text, &question.supporting_quote),
                    score: hit.score,
                    kb: Some(hit.kb.clone()),
                    document_id: hit.document_id.clone(),
                    chunk_id: Some(hit.id.clone()),
                    title: Some(hit.source.clone()),
                })
                .collect::<Vec<_>>();
            let option_ids = ["A", "B", "C", "D", "E", "F"];
            QuizQuestion {
                id: format!("q{}", index + 1),
                question_type: QuizQuestionType::SingleChoice,
                stem: question.stem,
                options: question
                    .options
                    .into_iter()
                    .enumerate()
                    .map(|(option_index, text)| QuizOption {
                        id: option_ids
                            .get(option_index)
                            .copied()
                            .unwrap_or("Z")
                            .to_string(),
                        text,
                    })
                    .collect(),
                correct_option_id: option_ids
                    .get(question.correct_option_index)
                    .copied()
                    .unwrap_or("A")
                    .to_string(),
                explanation: question.explanation,
                citations,
                tags: question.tags,
                difficulty: config.difficulty.clone(),
            }
        })
        .collect()
}

fn validate_quiz_questions_for_storage(questions: &[QuizQuestion]) -> anyhow::Result<()> {
    if questions.is_empty() {
        anyhow::bail!("quiz generation produced no questions");
    }
    for (index, question) in questions.iter().enumerate() {
        if question.stem.trim().is_empty() {
            anyhow::bail!("quiz question {} has an empty stem", index + 1);
        }
        if question.options.len() < 2 {
            anyhow::bail!("quiz question {} has fewer than two options", index + 1);
        }
        if !question
            .options
            .iter()
            .any(|option| option.id == question.correct_option_id)
        {
            anyhow::bail!("quiz question {} correct option does not exist", index + 1);
        }
        if question.explanation.trim().is_empty() {
            anyhow::bail!("quiz question {} has an empty explanation", index + 1);
        }
        if question.citations.is_empty() {
            anyhow::bail!("quiz question {} has no citations", index + 1);
        }
        for citation in &question.citations {
            if citation.text.trim().is_empty() {
                anyhow::bail!("quiz question {} has an empty citation", index + 1);
            }
        }
    }
    Ok(())
}

fn quiz_verification_report(method: &str, issues: Vec<String>) -> QuizVerificationReport {
    QuizVerificationReport {
        status: if issues.is_empty() {
            QuizVerificationStatus::Verified
        } else {
            QuizVerificationStatus::Warning
        },
        method: method.to_string(),
        checked_at: chrono::Utc::now(),
        issues,
    }
}

fn citation_excerpt(source_text: &str, quote: &str) -> String {
    let normalized_quote = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized_quote.is_empty() {
        return compact_text(source_text, 360);
    }

    let normalized_source = source_text.split_whitespace().collect::<Vec<_>>().join(" ");
    let Some(byte_pos) = normalized_source.find(&normalized_quote) else {
        return compact_text(source_text, 360);
    };

    let char_pos = normalized_source[..byte_pos].chars().count();
    let quote_chars = normalized_quote.chars().count();
    let start = char_pos.saturating_sub(100);
    let end = (char_pos + quote_chars + 160).min(normalized_source.chars().count());
    let mut excerpt = normalized_source
        .chars()
        .skip(start)
        .take(end - start)
        .collect::<String>();
    if start > 0 {
        excerpt.insert_str(0, "...");
    }
    if end < normalized_source.chars().count() {
        excerpt.push_str("...");
    }
    excerpt
}

fn questions_from_hits(config: &QuizConfig, hits: &[tutor_rag::SearchHit]) -> Vec<QuizQuestion> {
    let count = config.question_count.clamp(1, 10).min(hits.len().max(1));
    let topic = config
        .topic
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("the selected material");
    (0..count)
        .filter_map(|index| {
            let hit = hits.get(index % hits.len())?;
            let snippet = compact_text(&hit.text, 220);
            let distractor = hits
                .iter()
                .find(|candidate| candidate.id != hit.id)
                .map(|candidate| compact_text(&candidate.text, 140))
                .unwrap_or_else(|| "A statement that is not supported by the cited source.".into());
            Some(QuizQuestion {
                id: format!("q{}", index + 1),
                question_type: QuizQuestionType::SingleChoice,
                stem: format!("According to the cited material, which statement is best supported about {topic}?"),
                options: vec![
                    QuizOption {
                        id: "A".into(),
                        text: snippet.clone(),
                    },
                    QuizOption {
                        id: "B".into(),
                        text: "The cited material says this topic is unrelated to the knowledge base.".into(),
                    },
                    QuizOption {
                        id: "C".into(),
                        text: distractor,
                    },
                    QuizOption {
                        id: "D".into(),
                        text: "The answer cannot be inferred from any retrieved source.".into(),
                    },
                ],
                correct_option_id: "A".into(),
                explanation: format!("Option A is grounded in the retrieved source chunk from {}.", hit.source),
                citations: vec![QuizCitation {
                    source: hit.source.clone(),
                    text: hit.text.clone(),
                    score: hit.score,
                    kb: Some(hit.kb.clone()),
                    document_id: hit.document_id.clone(),
                    chunk_id: Some(hit.id.clone()),
                    title: Some(hit.source.clone()),
                }],
                tags: vec![topic.to_string(), hit.source.clone()],
                difficulty: config.difficulty.clone(),
            })
        })
        .collect()
}

fn compact_text(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= max_chars {
        return normalized;
    }
    let mut out = normalized.chars().take(max_chars).collect::<String>();
    out.push_str("...");
    out
}

fn error_response(err_status: StatusCode, err: impl std::fmt::Display) -> axum::response::Response {
    (
        err_status,
        Json(serde_json::json!({ "error": err.to_string() })),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Method, Request};
    use chrono::Utc;
    use tower::ServiceExt;

    #[tokio::test]
    async fn creates_and_answers_quiz() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(QuizStore::new_with_path(dir.path().join("quizzes.json")));
        let knowledge = KnowledgeStore::new_with_path(dir.path().join("knowledge-bases.json"));
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let embedding = tutor_rag::EmbeddingConfig {
            provider: "local-test".into(),
            model: "hash".into(),
            api_key: "test-key".into(),
            base_url: None,
            embeddings_path: None,
            dimensions: Some(32),
            send_dimensions: false,
        };
        let kb = knowledge.create("Quiz KB", embedding.clone()).unwrap();
        let kb_id = kb.id.clone();
        knowledge
            .add_document(
                &kb_id,
                crate::knowledge_store::KnowledgeDocument {
                    id: "doc-1".into(),
                    name: "source.md".into(),
                    source: "source.md".into(),
                    index_source: None,
                    size_bytes: 64,
                    chunks: 1,
                    mime_type: Some("text/markdown".into()),
                    content_path: None,
                    file_path: None,
                    created_at: Utc::now(),
                },
            )
            .unwrap();
        let rag_root = dir.path().join("rag");
        tutor_rag::LanceDbRag::new(&rag_root, embedding)
            .ingest_text(
                &kb_id,
                "source.md",
                "OPC corrects lithography mask patterns before wafer exposure.",
            )
            .await
            .unwrap();
        let app = quiz_router_with_rag_root(
            store,
            knowledge,
            notebook,
            memory.clone(),
            rag_root,
            dir.path().join("workflow-sessions"),
        );

        let create = serde_json::json!({
            "kb_id": kb_id,
            "topic": "OPC",
            "question_count": 1
        });
        let response = app
            .clone()
            .oneshot(json_request(Method::POST, "/api/quizzes", create))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = body_json(response).await;
        let quiz_id = body["quiz"]["id"].as_str().unwrap();
        let question_id = body["quiz"]["questions"][0]["id"].as_str().unwrap();
        let citation = &body["quiz"]["questions"][0]["citations"][0];
        assert_eq!(citation["kb"], kb_id);
        assert!(citation["document_id"].as_str().is_some());
        assert!(citation["chunk_id"].as_str().is_some());
        assert_eq!(body["quiz"]["verification"]["status"], "verified");

        let answer = serde_json::json!({
            "question_id": question_id,
            "selected_option_id": "A"
        });
        let response = app
            .oneshot(json_request(
                Method::POST,
                &format!("/api/quizzes/{quiz_id}/answers"),
                answer,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["quiz"]["score"]["correct"], 1);
        assert!(memory.recent_events(10).unwrap().len() >= 2);
    }

    #[tokio::test]
    async fn creates_quiz_from_source_text_without_knowledge_base() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(QuizStore::new_with_path(dir.path().join("quizzes.json")));
        let knowledge = KnowledgeStore::new_with_path(dir.path().join("knowledge-bases.json"));
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let app = quiz_router_with_rag_root(
            store,
            knowledge,
            notebook,
            memory,
            dir.path().join("rag"),
            dir.path().join("workflow-sessions"),
        );

        let create = serde_json::json!({
            "topic": "element reactions",
            "source_text": "Element reactions are triggered by switching between one or two characters. Talents and weapons shape role builds.",
            "source_label": "current conversation",
            "question_count": 1
        });
        let response = app
            .clone()
            .oneshot(json_request(Method::POST, "/api/quizzes", create))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = body_json(response).await;
        assert_eq!(body["quiz"]["kb_id"], "__conversation__");
        let quiz_id = body["quiz"]["id"].as_str().unwrap();
        assert_eq!(body["quiz"]["questions"].as_array().unwrap().len(), 1);
        assert_eq!(
            body["quiz"]["questions"][0]["citations"][0]["source"],
            "current conversation"
        );

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/quizzes")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["quizzes"].as_array().unwrap().len(), 1);

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("/api/quizzes/{quiz_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = body_json(response).await;
        assert_eq!(body["quiz"]["id"], quiz_id);
    }

    #[tokio::test]
    async fn creates_quiz_from_notebook_entry() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(QuizStore::new_with_path(dir.path().join("quizzes.json")));
        let knowledge = KnowledgeStore::new_with_path(dir.path().join("knowledge-bases.json"));
        let notebook = Arc::new(NotebookStore::new_with_path(dir.path().join("notebook")));
        let memory = Arc::new(MemoryStore::new_with_root(dir.path().join("memory")));
        let entry = notebook
            .create(crate::notebook_store::NotebookEntryInput {
                space_id: None,
                entry_type: crate::notebook_store::NotebookEntryType::ResearchReport,
                title: "OPC report".into(),
                path: None,
                markdown: "OPC corrects lithography mask patterns before wafer exposure.".into(),
                metadata: None,
                source_session_id: None,
                source_message_id: None,
            })
            .unwrap();
        let app = quiz_router_with_rag_root(
            store,
            knowledge,
            notebook,
            memory,
            dir.path().join("rag"),
            dir.path().join("workflow-sessions"),
        );

        let create = serde_json::json!({
            "notebook_entry_id": entry.id,
            "topic": "OPC review",
            "question_count": 1
        });
        let response = app
            .oneshot(json_request(Method::POST, "/api/quizzes", create))
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
        let body = body_json(response).await;
        assert_eq!(body["quiz"]["kb_id"], "__notebook__");
        assert_eq!(
            body["quiz"]["questions"][0]["citations"][0]["source"],
            "notebook: OPC report"
        );
    }

    #[test]
    fn detects_memory_aware_quiz_intent() {
        let req = CreateQuizRequest {
            title: None,
            kb_id: None,
            notebook_entry_id: None,
            source_text: Some("source".into()),
            source_label: None,
            topic: Some(
                "\u{9488}\u{5bf9}\u{6211}\u{7684}\u{8584}\u{5f31}\u{70b9}\u{51fa}\u{9898}".into(),
            ),
            difficulty: None,
            question_count: None,
            llm: None,
            knowledge_access: None,
        };
        assert!(should_use_memory_for_quiz(&req));

        let req = CreateQuizRequest {
            title: None,
            kb_id: None,
            notebook_entry_id: None,
            source_text: Some("source".into()),
            source_label: None,
            topic: Some("element reactions".into()),
            difficulty: None,
            question_count: None,
            llm: None,
            knowledge_access: None,
        };
        assert!(!should_use_memory_for_quiz(&req));
    }

    fn json_request(method: Method, uri: &str, body: serde_json::Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    async fn body_json(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
}
