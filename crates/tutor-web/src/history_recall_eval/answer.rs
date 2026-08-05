use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::future::BoxFuture;
use llm_harness_loop::test_utils::NoOpEnv;
use llm_harness_types::ExecutionEnv;
use nltk_porter::{Mode, PorterStemmer};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use tutor_agent::event_sink::{EventSink, SharedEventSink};
use tutor_agent::governance::GovernanceConfig;
use tutor_agent::{Capability, CapabilityRouter, LlmConfig};

use crate::session::AssistantSessionConfig;

use super::{
    BENCHMARK_RUN_ID_ENV, FOLUMI_REVISION_ENV, LOCOMO_DATASET_ENV, LOCOMO_MAX_QUESTIONS_ENV,
    LOCOMO_MAX_SAMPLES_ENV, LOCOMO_REVISION_ENV, LocomoQuestion, RUNTIME_REVISION_ENV,
    create_session, duration_ms, env_or_unknown, import_locomo_conversation, load_locomo,
    percentile, positive_env_limit,
};

const LOCOMO_ANSWER_OUTPUT_ENV: &str = "FOLUMI_LOCOMO_ANSWER_OUTPUT";
const LOCOMO_INCLUDE_TEXT_ENV: &str = "FOLUMI_LOCOMO_INCLUDE_TEXT";
const LOCOMO_ASSISTANT_NAME_ENV: &str = "FOLUMI_LOCOMO_ASSISTANT_NAME";
const LOCOMO_ASSISTANT_INSTRUCTIONS_ENV: &str = "FOLUMI_LOCOMO_ASSISTANT_INSTRUCTIONS";
const LOCOMO_ASSISTANT_PROFILE_SOURCE_ENV: &str = "FOLUMI_LOCOMO_ASSISTANT_PROFILE_SOURCE";
const ANSWER_PROMPT_REVISION: &str = "folumi-locomo-agent-answer-v2";
const ANSWER_BENCHMARK_INSTRUCTION: &str = "You are being evaluated on questions about earlier conversations. Use History Recall when needed and do not use web search, code execution, or outside factual sources. Answer with only a short phrase, using exact words from the recalled conversations whenever possible. Do not describe searches, tools, or reasoning. If the earlier conversations do not support an answer, reply exactly: No information available.";

#[derive(Clone, Default)]
struct UsageTotals {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    cost_usd: f64,
}

impl UsageTotals {
    fn merge(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.cost_usd += other.cost_usd;
    }
}

#[derive(Default)]
struct AnswerTrace {
    tool_calls: Vec<String>,
    usage: UsageTotals,
}

struct BenchmarkAssistantProfile {
    config: AssistantSessionConfig,
    source: &'static str,
}

#[derive(Default)]
struct TraceRecorder {
    events: Mutex<Vec<(String, Value)>>,
}

impl TraceRecorder {
    fn mark(&self) -> usize {
        self.events.lock().expect("trace recorder lock").len()
    }

    fn since(&self, mark: usize) -> AnswerTrace {
        let events = self.events.lock().expect("trace recorder lock");
        let mut trace = AnswerTrace::default();
        for (kind, data) in events.iter().skip(mark) {
            if kind == "tool_call"
                && let Some(tool) = data.get("tool").and_then(Value::as_str)
            {
                trace.tool_calls.push(tool.to_string());
            }
            if kind == "runtime_usage" {
                trace.usage.input_tokens += json_u64(data.get("input_tokens"));
                trace.usage.output_tokens += json_u64(data.get("output_tokens"));
                trace.usage.cache_read_tokens += json_u64(data.get("cache_read_tokens"));
                trace.usage.cache_write_tokens += json_u64(data.get("cache_write_tokens"));
                trace.usage.cost_usd += data.get("cost_usd").and_then(Value::as_f64).unwrap_or(0.0);
            }
        }
        trace
    }
}

impl EventSink for TraceRecorder {
    fn trace(&self, kind: String, data: Value) -> BoxFuture<'static, ()> {
        self.events
            .lock()
            .expect("trace recorder lock")
            .push((kind, data));
        Box::pin(async {})
    }
}

#[derive(Default)]
struct AnswerMetrics {
    questions: usize,
    official_score_sum: f64,
    exact_matches: usize,
    abstention_questions: usize,
    abstention_correct: usize,
    questions_with_search: usize,
    questions_with_read: usize,
    unexpected_tool_calls: usize,
    tool_narrations: usize,
    errors: usize,
}

impl AnswerMetrics {
    fn record(
        &mut self,
        category: u8,
        score: f64,
        exact_match: bool,
        trace: &AnswerTrace,
        prediction: &str,
        failed: bool,
    ) {
        self.questions += 1;
        self.official_score_sum += score;
        self.exact_matches += usize::from(exact_match);
        if category == 5 {
            self.abstention_questions += 1;
            self.abstention_correct += usize::from(score == 1.0);
        }
        self.questions_with_search += usize::from(
            trace
                .tool_calls
                .iter()
                .any(|tool| tool == "knowledge_search"),
        );
        self.questions_with_read +=
            usize::from(trace.tool_calls.iter().any(|tool| tool == "knowledge_read"));
        self.unexpected_tool_calls += trace
            .tool_calls
            .iter()
            .filter(|tool| !matches!(tool.as_str(), "knowledge_search" | "knowledge_read"))
            .count();
        self.tool_narrations += usize::from(contains_tool_narration(prediction));
        self.errors += usize::from(failed);
    }

    fn merge(&mut self, other: &Self) {
        self.questions += other.questions;
        self.official_score_sum += other.official_score_sum;
        self.exact_matches += other.exact_matches;
        self.abstention_questions += other.abstention_questions;
        self.abstention_correct += other.abstention_correct;
        self.questions_with_search += other.questions_with_search;
        self.questions_with_read += other.questions_with_read;
        self.unexpected_tool_calls += other.unexpected_tool_calls;
        self.tool_narrations += other.tool_narrations;
        self.errors += other.errors;
    }

    fn report(&self, label: &str) {
        println!(
            "locomo_answer {label} questions={} answer_f1={:.3} exact_match={:.3} abstention_accuracy={} search_rate={:.3} read_rate={:.3} errors={}",
            self.questions,
            self.official_score_sum / self.questions.max(1) as f64,
            ratio(self.exact_matches, self.questions),
            self.abstention_accuracy()
                .map(|value| format!("{value:.3}"))
                .unwrap_or_else(|| "n/a".into()),
            ratio(self.questions_with_search, self.questions),
            ratio(self.questions_with_read, self.questions),
            self.errors,
        );
    }

    fn abstention_accuracy(&self) -> Option<f64> {
        (self.abstention_questions > 0)
            .then(|| ratio(self.abstention_correct, self.abstention_questions))
    }
}

/// Runs LoCoMo through Folumi's real Chat Agent and runtime-owned Session Recall tools.
/// Online model access is deliberate, so this benchmark is ignored by default and never CI-gated.
#[tokio::test]
#[ignore = "online LoCoMo Agent benchmark; set provider credentials and run explicitly"]
async fn locomo_agent_answer_accuracy_benchmark() {
    let dataset_path = std::env::var(LOCOMO_DATASET_ENV).unwrap_or_else(|_| {
        panic!("set {LOCOMO_DATASET_ENV} to the official LoCoMo data/locomo10.json path")
    });
    let max_samples = positive_env_limit(LOCOMO_MAX_SAMPLES_ENV).unwrap_or(usize::MAX);
    let max_questions = positive_env_limit(LOCOMO_MAX_QUESTIONS_ENV).unwrap_or(usize::MAX);
    let include_text = env_flag(LOCOMO_INCLUDE_TEXT_ENV);
    let llm = LlmConfig::from_env().expect("load benchmark LLM configuration from environment");
    let provider = format!("{:?}", llm.provider).to_ascii_lowercase();
    let model = llm.model.clone();
    let context_window_tokens = llm.context_window_tokens;
    let client = llm.build_client();
    let assistant_profile = benchmark_assistant_profile();
    let samples = load_locomo(Path::new(&dataset_path));
    assert!(!samples.is_empty(), "LoCoMo dataset contained no samples");

    let benchmark_root = tempfile::tempdir().expect("create LoCoMo answer benchmark root");
    let security = crate::knowledge_runtime::AgentRuntimeSecurity::generate();
    let mut overall = AnswerMetrics::default();
    let mut by_category = BTreeMap::<u8, AnswerMetrics>::new();
    let mut sample_reports = Vec::new();
    let mut latencies = Vec::<Duration>::new();
    let mut usage = UsageTotals::default();
    let mut questions_seen = 0usize;
    let mut samples_run = 0usize;

    for (sample_index, sample) in samples.into_iter().take(max_samples).enumerate() {
        samples_run += 1;
        let sample_root = benchmark_root.path().join(format!("sample-{sample_index}"));
        let pool = super::SessionPool::new_with_root_and_history_recall(&sample_root, true);
        import_locomo_conversation(&pool, &sample).await;
        pool.synchronize_history_recall(true)
            .await
            .expect("synchronize imported LoCoMo sessions");

        let recorder = Arc::new(TraceRecorder::default());
        let sink: SharedEventSink = recorder.clone();
        let env = Arc::new(NoOpEnv) as Arc<dyn ExecutionEnv>;
        let instruction = [
            crate::assistant_profile::assistant_profile_instruction(&assistant_profile.config),
            crate::routes::ws::ASSISTANT_INTERACTION_STYLE_INSTRUCTION.into(),
            crate::routes::ws::HISTORY_RECALL_TOOL_INSTRUCTION.into(),
            ANSWER_BENCHMARK_INSTRUCTION.into(),
        ]
        .join("\n\n");
        let router = CapabilityRouter::new(
            env,
            llm.clone(),
            GovernanceConfig::new(f64::MAX, None, false),
        )
        .with_client(client.clone())
        .with_event_sink(sink)
        .with_product_instruction(instruction);
        let router = crate::knowledge_runtime::install_agent_knowledge_and_memory(
            router,
            None,
            None,
            Some(pool.history_recall_knowledge_source()),
            &security,
        )
        .expect("install Session Recall in benchmark Agent");

        let mut sample_metrics = AnswerMetrics::default();
        let mut question_reports = Vec::new();
        for (question_index, qa) in sample.qa.iter().take(max_questions).enumerate() {
            questions_seen += 1;
            let answer_session_id = create_session(&pool, true).await;
            let answer_session = pool
                .open_runtime_session(&answer_session_id)
                .await
                .expect("open isolated benchmark answer Session");
            let request = crate::routes::ws::agent_run_request(
                answer_prompt(qa),
                &answer_session_id,
                None,
                false,
                true,
            )
            .expect("build benchmark Agent request");
            let trace_mark = recorder.mark();
            let started = Instant::now();
            let result = router
                .run_request_with_session_cancel(Capability::Chat, answer_session, request, None)
                .await;
            let latency = started.elapsed();
            latencies.push(latency);
            let trace = recorder.since(trace_mark);
            usage.merge(&trace.usage);
            let (prediction, error) = match result {
                Ok(prediction) => (prediction.trim().to_string(), None),
                Err(error) => {
                    eprintln!(
                        "locomo_answer error sample={} question_index={} error={error}",
                        sample.sample_id, question_index
                    );
                    (String::new(), Some(error.to_string()))
                }
            };
            let reference = answer_text(&qa.answer);
            let score = official_question_score(qa.category, &prediction, &reference);
            let exact_match = official_exact_match(qa.category, &prediction, &reference);
            sample_metrics.record(
                qa.category,
                score,
                exact_match,
                &trace,
                &prediction,
                error.is_some(),
            );
            by_category.entry(qa.category).or_default().record(
                qa.category,
                score,
                exact_match,
                &trace,
                &prediction,
                error.is_some(),
            );
            question_reports.push(question_report(
                &sample.sample_id,
                question_index,
                qa,
                &reference,
                &prediction,
                score,
                exact_match,
                duration_ms(latency),
                &trace,
                error.as_deref(),
                include_text,
            ));
        }

        sample_metrics.report(&format!("sample={}", sample.sample_id));
        sample_reports.push(json!({
            "sample_id": sample.sample_id,
            "metrics": answer_metric_report(&sample_metrics),
            "questions": question_reports,
        }));
        overall.merge(&sample_metrics);
    }

    assert!(
        overall.questions > 0,
        "LoCoMo selection contained no questions"
    );
    overall.report(&format!("overall samples={samples_run}"));
    for (category, metrics) in &by_category {
        metrics.report(&format!("category={category}"));
    }
    latencies.sort_unstable();
    let p50_ms = duration_ms(percentile(&latencies, 50));
    let p95_ms = duration_ms(percentile(&latencies, 95));
    println!(
        "locomo_answer diagnostics latency_p50_ms={p50_ms:.1} latency_p95_ms={p95_ms:.1} input_tokens={} output_tokens={} cost_usd={:.6} unexpected_tool_calls={} tool_narrations={}",
        usage.input_tokens,
        usage.output_tokens,
        usage.cost_usd,
        overall.unexpected_tool_calls,
        overall.tool_narrations,
    );
    write_answer_report(AnswerReportInput {
        provider: &provider,
        model: &model,
        context_window_tokens,
        assistant_profile: &assistant_profile,
        overall: &overall,
        by_category: &by_category,
        samples: sample_reports,
        samples_run,
        questions_seen,
        p50_ms,
        p95_ms,
        usage: &usage,
        include_text,
    });
}

fn benchmark_assistant_profile() -> BenchmarkAssistantProfile {
    let name = std::env::var(LOCOMO_ASSISTANT_NAME_ENV).ok();
    let instructions = std::env::var(LOCOMO_ASSISTANT_INSTRUCTIONS_ENV).ok();
    let source = std::env::var(LOCOMO_ASSISTANT_PROFILE_SOURCE_ENV).ok();
    benchmark_assistant_profile_from_values(
        name.as_deref(),
        instructions.as_deref(),
        source.as_deref(),
    )
}

fn benchmark_assistant_profile_from_values(
    name: Option<&str>,
    instructions: Option<&str>,
    source: Option<&str>,
) -> BenchmarkAssistantProfile {
    let overridden = name.is_some_and(|value| !value.trim().is_empty())
        || instructions.is_some_and(|value| !value.trim().is_empty());
    let defaults = AssistantSessionConfig::default();
    BenchmarkAssistantProfile {
        config: crate::assistant_profile::normalize_assistant_profile(
            name.unwrap_or(&defaults.name),
            instructions.unwrap_or(&defaults.instructions),
        ),
        source: match source.map(str::trim) {
            Some("product_settings") => "product_settings",
            Some("benchmark_override") => "benchmark_override",
            Some("product_default") => "product_default",
            _ if overridden => "benchmark_override",
            _ => "product_default",
        },
    }
}

fn answer_prompt(qa: &LocomoQuestion) -> String {
    let temporal_hint = if qa.category == 2 {
        " Use the DATE attached to recalled conversation turns and give an approximate date when necessary."
    } else {
        ""
    };
    format!(
        "Answer this question about the earlier conversations.{temporal_hint}\n\nQuestion: {}\nShort answer:",
        qa.question
    )
}

fn answer_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(answer_text)
            .collect::<Vec<_>>()
            .join(", "),
        value => value.to_string(),
    }
}

fn official_question_score(category: u8, prediction: &str, reference: &str) -> f64 {
    match category {
        1 => multi_answer_f1(prediction, reference),
        2 | 4 => token_f1(prediction, reference),
        3 => token_f1(
            prediction,
            reference.split(';').next().unwrap_or(reference).trim(),
        ),
        5 => f64::from(is_abstention(prediction)),
        _ => panic!("unsupported LoCoMo category {category}"),
    }
}

fn official_exact_match(category: u8, prediction: &str, reference: &str) -> bool {
    if category == 5 {
        return is_abstention(prediction);
    }
    let reference = if category == 3 {
        reference.split(';').next().unwrap_or(reference).trim()
    } else {
        reference
    };
    normalized_tokens(prediction)
        .into_iter()
        .collect::<HashSet<_>>()
        == normalized_tokens(reference)
            .into_iter()
            .collect::<HashSet<_>>()
}

fn token_f1(prediction: &str, reference: &str) -> f64 {
    let stemmer = PorterStemmer::new(Mode::Nltk);
    let prediction = normalized_tokens(prediction)
        .into_iter()
        .map(|token| stemmer.stem(&token))
        .collect::<Vec<_>>();
    let reference = normalized_tokens(reference)
        .into_iter()
        .map(|token| stemmer.stem(&token))
        .collect::<Vec<_>>();
    let mut prediction_counts = HashMap::<String, usize>::new();
    let mut reference_counts = HashMap::<String, usize>::new();
    for token in &prediction {
        *prediction_counts.entry(token.clone()).or_default() += 1;
    }
    for token in &reference {
        *reference_counts.entry(token.clone()).or_default() += 1;
    }
    let common = prediction_counts
        .iter()
        .map(|(token, count)| count.min(reference_counts.get(token).unwrap_or(&0)))
        .sum::<usize>();
    if common == 0 {
        return 0.0;
    }
    let precision = common as f64 / prediction.len() as f64;
    let recall = common as f64 / reference.len() as f64;
    2.0 * precision * recall / (precision + recall)
}

fn multi_answer_f1(prediction: &str, reference: &str) -> f64 {
    let predictions = prediction.split(',').map(str::trim).collect::<Vec<_>>();
    let references = reference.split(',').map(str::trim).collect::<Vec<_>>();
    if references.is_empty() {
        return 0.0;
    }
    references
        .iter()
        .map(|reference| {
            predictions
                .iter()
                .map(|prediction| token_f1(prediction, reference))
                .fold(0.0, f64::max)
        })
        .sum::<f64>()
        / references.len() as f64
}

fn normalized_tokens(text: &str) -> Vec<String> {
    text.replace(',', "")
        .to_lowercase()
        .chars()
        .filter(|character| !character.is_ascii_punctuation())
        .collect::<String>()
        .split_whitespace()
        .filter(|token| !matches!(*token, "a" | "an" | "the" | "and"))
        .map(str::to_string)
        .collect()
}

fn is_abstention(prediction: &str) -> bool {
    let prediction = prediction.to_lowercase();
    prediction.contains("no information available") || prediction.contains("not mentioned")
}

fn contains_tool_narration(prediction: &str) -> bool {
    let prediction = prediction.to_lowercase();
    [
        "check my memory",
        "search our history",
        "search the history",
        "look through our history",
        "查一下记忆",
        "搜索一下历史",
        "检索一下历史",
    ]
    .iter()
    .any(|phrase| prediction.contains(phrase))
}

fn answer_metric_report(metrics: &AnswerMetrics) -> Value {
    json!({
        "questions": metrics.questions,
        "official_score_sum": metrics.official_score_sum,
        "answer_f1": metrics.official_score_sum / metrics.questions.max(1) as f64,
        "exact_match_count": metrics.exact_matches,
        "exact_match": ratio(metrics.exact_matches, metrics.questions),
        "abstention_questions": metrics.abstention_questions,
        "abstention_correct": metrics.abstention_correct,
        "abstention_accuracy": metrics.abstention_accuracy(),
        "questions_with_search": metrics.questions_with_search,
        "search_rate": ratio(metrics.questions_with_search, metrics.questions),
        "questions_with_read": metrics.questions_with_read,
        "read_rate": ratio(metrics.questions_with_read, metrics.questions),
        "unexpected_tool_calls": metrics.unexpected_tool_calls,
        "tool_narrations": metrics.tool_narrations,
        "errors": metrics.errors,
    })
}

#[allow(clippy::too_many_arguments)]
fn question_report(
    sample_id: &str,
    question_index: usize,
    qa: &LocomoQuestion,
    reference: &str,
    prediction: &str,
    score: f64,
    exact_match: bool,
    latency_ms: f64,
    trace: &AnswerTrace,
    error: Option<&str>,
    include_text: bool,
) -> Value {
    let mut report = Map::from_iter([
        ("sample_id".into(), Value::String(sample_id.into())),
        ("question_index".into(), json!(question_index)),
        ("category".into(), json!(qa.category)),
        ("official_score".into(), json!(score)),
        ("exact_match".into(), json!(exact_match)),
        ("latency_ms".into(), json!(latency_ms)),
        ("tool_calls".into(), json!(trace.tool_calls)),
        ("usage".into(), usage_report(&trace.usage)),
        ("error".into(), json!(error)),
    ]);
    if include_text {
        report.insert("question".into(), Value::String(qa.question.clone()));
        report.insert("reference_answer".into(), Value::String(reference.into()));
        report.insert("prediction".into(), Value::String(prediction.into()));
    }
    Value::Object(report)
}

struct AnswerReportInput<'a> {
    provider: &'a str,
    model: &'a str,
    context_window_tokens: Option<u32>,
    assistant_profile: &'a BenchmarkAssistantProfile,
    overall: &'a AnswerMetrics,
    by_category: &'a BTreeMap<u8, AnswerMetrics>,
    samples: Vec<Value>,
    samples_run: usize,
    questions_seen: usize,
    p50_ms: f64,
    p95_ms: f64,
    usage: &'a UsageTotals,
    include_text: bool,
}

fn write_answer_report(input: AnswerReportInput<'_>) {
    let Ok(output_path) = std::env::var(LOCOMO_ANSWER_OUTPUT_ENV) else {
        return;
    };
    let generated_at = chrono::Utc::now();
    let run_id = std::env::var(BENCHMARK_RUN_ID_ENV).unwrap_or_else(|_| {
        generated_at
            .format("locomo-answer-%Y%m%dT%H%M%SZ")
            .to_string()
    });
    let categories = input
        .by_category
        .iter()
        .map(|(category, metrics)| (category.to_string(), answer_metric_report(metrics)))
        .collect::<Map<_, _>>();
    let report = json!({
        "schema_version": 1,
        "benchmark": "locomo_agent_answer_accuracy",
        "run_id": run_id,
        "generated_at": generated_at.to_rfc3339(),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "provenance": {
            "folumi_revision": env_or_unknown(FOLUMI_REVISION_ENV),
            "runtime_revision": env_or_unknown(RUNTIME_REVISION_ENV),
            "dataset": "LoCoMo locomo10.json",
            "dataset_revision": env_or_unknown(LOCOMO_REVISION_ENV),
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "cpu": env_or_unknown("PROCESSOR_IDENTIFIER"),
        },
        "configuration": {
            "provider": input.provider,
            "model": input.model,
            "context_window_tokens": input.context_window_tokens,
            "max_output_tokens": 8192,
            "prompt_revision": ANSWER_PROMPT_REVISION,
            "assistant_profile": assistant_profile_report(
                input.assistant_profile,
                input.include_text,
            ),
            "scorer": "locomo_official_token_f1_compatible",
            "category_5_protocol": "free_form_abstention",
            "saved_memory_enabled": false,
            "history_recall_enabled": true,
            "history_recall_automatic": false,
            "isolated_temporary_answer_sessions": true,
            "conversation_import": "date_and_blip_caption_prefixed_utterances",
            "max_samples": positive_env_limit(LOCOMO_MAX_SAMPLES_ENV),
            "max_questions_per_sample": positive_env_limit(LOCOMO_MAX_QUESTIONS_ENV),
            "includes_dataset_text": input.include_text,
        },
        "dataset_counts": {
            "samples": input.samples_run,
            "questions_seen": input.questions_seen,
            "questions_scored": input.overall.questions,
            "question_errors": input.overall.errors,
        },
        "overall": answer_metric_report(input.overall),
        "categories": categories,
        "samples": input.samples,
        "diagnostics": {
            "answer_latency_p50_ms": input.p50_ms,
            "answer_latency_p95_ms": input.p95_ms,
            "usage": usage_report(input.usage),
        },
    });
    let output_path = Path::new(&output_path);
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            panic!(
                "failed to create LoCoMo answer report directory {}: {error}",
                parent.display()
            )
        });
    }
    let json = serde_json::to_vec_pretty(&report).expect("serialize LoCoMo answer report");
    fs::write(output_path, json).unwrap_or_else(|error| {
        panic!(
            "failed to write LoCoMo answer report {}: {error}",
            output_path.display()
        )
    });
    println!("locomo_answer report={}", output_path.display());
}

fn assistant_profile_report(profile: &BenchmarkAssistantProfile, include_text: bool) -> Value {
    let mut value = json!({
        "name": profile.config.name.as_str(),
        "source": profile.source,
        "instructions_sha256": sha256_hex(profile.config.instructions.as_bytes()),
        "revision": sha256_hex(
            format!("{}\0{}", profile.config.name, profile.config.instructions).as_bytes(),
        ),
    });
    if include_text {
        value["instructions"] = json!(profile.config.instructions.as_str());
    }
    value
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn usage_report(usage: &UsageTotals) -> Value {
    json!({
        "input_tokens": usage.input_tokens,
        "output_tokens": usage.output_tokens,
        "cache_read_tokens": usage.cache_read_tokens,
        "cache_write_tokens": usage.cache_write_tokens,
        "cost_usd": usage.cost_usd,
    })
}

fn json_u64(value: Option<&Value>) -> u64 {
    value
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|value| value as u64))
        })
        .unwrap_or(0)
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).ok().is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

#[test]
fn locomo_official_compatible_scorer_handles_categories() {
    assert_eq!(token_f1("The adoption agencies", "adoption agency"), 1.0);
    assert_eq!(token_f1("generously", "generous"), 1.0);
    assert_eq!(multi_answer_f1("painting, hiking", "paintings, hike"), 1.0);
    assert_eq!(
        official_question_score(3, "psychology", "Psychology; Counseling"),
        1.0
    );
    assert_eq!(
        official_question_score(5, "No information available.", "distractor"),
        1.0
    );
    assert!(official_exact_match(2, "7 May 2023", "May 7, 2023"));
}

#[test]
fn benchmark_assistant_profile_uses_product_default_or_explicit_override() {
    let defaults = benchmark_assistant_profile_from_values(None, None, None);
    assert_eq!(defaults.source, "product_default");
    assert_eq!(
        defaults.config.instructions,
        crate::assistant_profile::DEFAULT_ASSISTANT_INSTRUCTIONS
    );

    let custom = benchmark_assistant_profile_from_values(
        Some("Mori"),
        Some("Return only the requested answer."),
        None,
    );
    assert_eq!(custom.source, "benchmark_override");
    assert_eq!(custom.config.name, "Mori");
    assert_eq!(
        custom.config.instructions,
        "Return only the requested answer."
    );
}

#[test]
fn benchmark_assistant_profile_report_hides_text_by_default() {
    let profile =
        benchmark_assistant_profile_from_values(Some("Mori"), Some("Private style"), None);
    let public = assistant_profile_report(&profile, false);
    assert!(public.get("instructions").is_none());
    assert_eq!(public["instructions_sha256"].as_str().unwrap().len(), 64);

    let local = assistant_profile_report(&profile, true);
    assert_eq!(local["instructions"], "Private style");
}

#[test]
fn benchmark_assistant_profile_preserves_product_settings_source() {
    let profile = benchmark_assistant_profile_from_values(
        Some("My Folumi"),
        Some("Be practical."),
        Some("product_settings"),
    );
    assert_eq!(profile.source, "product_settings");
}

#[test]
fn answer_metric_distinguishes_search_read_and_narration() {
    let trace = AnswerTrace {
        tool_calls: vec![
            "knowledge_search".into(),
            "knowledge_read".into(),
            "web_search".into(),
        ],
        usage: UsageTotals::default(),
    };
    let mut metrics = AnswerMetrics::default();
    metrics.record(5, 1.0, true, &trace, "I'll check my memory first", false);
    assert_eq!(metrics.questions_with_search, 1);
    assert_eq!(metrics.questions_with_read, 1);
    assert_eq!(metrics.unexpected_tool_calls, 1);
    assert_eq!(metrics.tool_narrations, 1);
    assert_eq!(metrics.abstention_correct, 1);
}
