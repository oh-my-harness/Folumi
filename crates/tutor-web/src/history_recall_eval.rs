use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::session::{
    AssistantSessionConfig, HISTORY_RECALL_MAX_SNIPPET_BYTES, SessionCreateConfig, SessionPool,
};
use llm_harness_agent::SessionRecallScope;
use llm_harness_runtime_knowledge::{
    KnowledgeAccessContext, KnowledgeRequestContext, KnowledgeSource, PrincipalRef,
    SourceSearchPage, SourceSearchRequest,
};
use llm_harness_runtime_session_recall::SessionRecallAccessContext;
use llm_harness_types::{RunContext, RunRequest};
use serde::Deserialize;
use tokio_util::sync::CancellationToken;

const SEARCH_LIMIT: usize = 3;
const SESSION_RECALL_NAMESPACE: &str = "folumi-session-history";
const LOCAL_USER_ID: &str = "local-user";
const LOCOMO_DATASET_ENV: &str = "FOLUMI_LOCOMO_DATASET";
const LOCOMO_MAX_SAMPLES_ENV: &str = "FOLUMI_LOCOMO_MAX_SAMPLES";
const LOCOMO_MAX_QUESTIONS_ENV: &str = "FOLUMI_LOCOMO_MAX_QUESTIONS";
const LOCOMO_OUTPUT_ENV: &str = "FOLUMI_LOCOMO_OUTPUT";
const BENCHMARK_RUN_ID_ENV: &str = "FOLUMI_BENCHMARK_RUN_ID";
const FOLUMI_REVISION_ENV: &str = "FOLUMI_BENCHMARK_FOLUMI_REVISION";
const RUNTIME_REVISION_ENV: &str = "FOLUMI_BENCHMARK_RUNTIME_REVISION";
const LOCOMO_REVISION_ENV: &str = "FOLUMI_BENCHMARK_LOCOMO_REVISION";

struct PositiveFixture {
    user: &'static str,
    assistant: &'static str,
    query: &'static str,
}

const POSITIVE_FIXTURES: &[PositiveFixture] = &[
    PositiveFixture {
        user: "For project Atlas, use the cobalt dashboard with three columns for weekly planning.",
        assistant: "Understood. The Atlas weekly view uses the cobalt three-column dashboard.",
        query: "cobalt dashboard Atlas",
    },
    PositiveFixture {
        user: "The launch phrase for the demo is silver comet and it should remain lowercase.",
        assistant: "The demo launch phrase is silver comet in lowercase.",
        query: "silver comet launch phrase",
    },
    PositiveFixture {
        user: "For the Kyoto trip, the maple archive contains the confirmed hotel itinerary.",
        assistant: "I will treat the maple archive as the confirmed Kyoto itinerary source.",
        query: "Kyoto maple archive itinerary",
    },
    PositiveFixture {
        user: "Vendor Quartz is associated with invoice 47Q and the September renewal.",
        assistant: "Invoice 47Q is the September renewal for Vendor Quartz.",
        query: "Quartz invoice 47Q renewal",
    },
    PositiveFixture {
        user: "When explaining matrix calculus, start with geometric intuition before equations.",
        assistant: "I will begin matrix calculus explanations with geometric intuition.",
        query: "matrix calculus geometric intuition",
    },
    PositiveFixture {
        user: "On Windows Rust builds, keep CARGO_BUILD_JOBS set to one during linker-heavy checks.",
        assistant: "Windows linker-heavy Rust checks will use one Cargo build job.",
        query: "CARGO_BUILD_JOBS Windows linker",
    },
    PositiveFixture {
        user: "Save gardening drafts under the Notebook folder Garden Seeds before publishing them.",
        assistant: "Gardening drafts belong in the Garden Seeds Notebook folder.",
        query: "Garden Seeds Notebook drafts",
    },
    PositiveFixture {
        user: "项目 暗号 是 青铜 星图，交付窗口定在周四下午。",
        assistant: "已记录：青铜 星图 对应周四下午的交付窗口。",
        query: "青铜 星图 交付窗口",
    },
];

const NEGATIVE_QUERIES: &[&str] = &[
    "ultraviolet seahorse",
    "crimson elevator warranty",
    "vapor orchid",
    "granite pelican",
];

#[tokio::test]
#[ignore = "offline acceptance benchmark; run in release mode with --ignored --nocapture"]
async fn history_recall_offline_quality_latency_and_context_baseline() {
    let root = tempfile::tempdir().unwrap();
    let pool = SessionPool::new_with_root_and_history_recall(root.path(), true);
    let mut expected_session_ids = Vec::with_capacity(POSITIVE_FIXTURES.len());

    for fixture in POSITIVE_FIXTURES {
        let id = create_session(&pool, false).await;
        append_turn(&pool, &id, fixture.user, fixture.assistant).await;
        expected_session_ids.push(id);
    }

    let temporary_id = create_session(&pool, true).await;
    append_turn(
        &pool,
        &temporary_id,
        "The temporary-only phrase is vapor orchid.",
        "This phrase belongs only to the temporary conversation.",
    )
    .await;

    let deleted_id = create_session(&pool, false).await;
    append_turn(
        &pool,
        &deleted_id,
        "The disposable phrase is granite pelican.",
        "This conversation will be deleted before evaluation.",
    )
    .await;
    pool.delete(&deleted_id).await.unwrap();
    pool.synchronize_history_recall(true).await.unwrap();

    let source = pool.history_recall_knowledge_source();
    let access = recall_knowledge_access();
    let mut top_1_hits = 0usize;
    let mut top_3_hits = 0usize;
    let mut context_bytes = Vec::with_capacity(POSITIVE_FIXTURES.len());
    let mut context_tokens = Vec::with_capacity(POSITIVE_FIXTURES.len());

    for (fixture, expected_session_id) in POSITIVE_FIXTURES.iter().zip(&expected_session_ids) {
        let page = search(&source, &access, fixture.query).await;
        if page
            .hits
            .first()
            .is_some_and(|hit| belongs_to_session(hit.uri.as_deref(), expected_session_id))
        {
            top_1_hits += 1;
        }
        if page
            .hits
            .iter()
            .any(|hit| belongs_to_session(hit.uri.as_deref(), expected_session_id))
        {
            top_3_hits += 1;
        }
        let snippets = page
            .hits
            .iter()
            .map(|hit| hit.snippet.as_str())
            .collect::<Vec<_>>();
        context_bytes.push(snippets.iter().map(|snippet| snippet.len()).sum::<usize>());
        context_tokens.push(
            snippets
                .iter()
                .map(|snippet| approximate_tokens(snippet))
                .sum::<usize>(),
        );
    }

    let mut negative_false_positives = 0usize;
    for query in NEGATIVE_QUERIES {
        if !search(&source, &access, query).await.hits.is_empty() {
            negative_false_positives += 1;
        }
    }

    for fixture in POSITIVE_FIXTURES.iter().take(3) {
        let _ = search(&source, &access, fixture.query).await;
    }
    let mut latency = Vec::with_capacity(200);
    for index in 0..200 {
        let query = POSITIVE_FIXTURES[index % POSITIVE_FIXTURES.len()].query;
        let started = Instant::now();
        let _ = search(&source, &access, query).await;
        latency.push(started.elapsed());
    }
    latency.sort_unstable();
    context_bytes.sort_unstable();
    context_tokens.sort_unstable();

    let recall_at_1 = top_1_hits as f64 / POSITIVE_FIXTURES.len() as f64;
    let recall_at_3 = top_3_hits as f64 / POSITIVE_FIXTURES.len() as f64;
    let wrong_top_1_rate = 1.0 - recall_at_1;
    let negative_false_positive_rate =
        negative_false_positives as f64 / NEGATIVE_QUERIES.len() as f64;
    let p50_latency = percentile(&latency, 50);
    let p95_latency = percentile(&latency, 95);
    let p95_context_bytes = percentile(&context_bytes, 95);
    let p95_context_tokens = percentile(&context_tokens, 95);
    let max_context_bytes = SEARCH_LIMIT * HISTORY_RECALL_MAX_SNIPPET_BYTES;
    let p95_context_occupancy = p95_context_bytes as f64 / max_context_bytes as f64;

    println!(
        "history_recall_eval recall_at_1={recall_at_1:.3} recall_at_3={recall_at_3:.3} \
wrong_top_1_rate={wrong_top_1_rate:.3} negative_false_positive_rate={negative_false_positive_rate:.3} \
search_p50_ms={:.3} search_p95_ms={:.3} context_p95_bytes={} context_p95_approx_tokens={} \
context_p95_occupancy={p95_context_occupancy:.3}",
        duration_ms(p50_latency),
        duration_ms(p95_latency),
        p95_context_bytes,
        p95_context_tokens,
    );

    assert!(
        recall_at_1 >= 0.875,
        "Recall@1 fell below the accepted baseline"
    );
    assert_eq!(
        top_3_hits,
        POSITIVE_FIXTURES.len(),
        "every relevant Session must appear in the top three"
    );
    assert!(
        wrong_top_1_rate <= 0.125,
        "wrong top-1 rate exceeded the baseline"
    );
    assert_eq!(
        negative_false_positives, 0,
        "negative, temporary, or deleted fixtures must not be recalled"
    );
    assert!(
        p95_context_occupancy <= 1.0,
        "search snippets exceeded the configured bounded context"
    );
    assert!(
        p95_latency <= Duration::from_millis(50),
        "warm local search P95 exceeded 50 ms"
    );
}

#[derive(Deserialize)]
struct LocomoSample {
    sample_id: String,
    qa: Vec<LocomoQuestion>,
    conversation: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct LocomoQuestion {
    question: String,
    #[serde(default)]
    answer: serde_json::Value,
    #[serde(default)]
    evidence: Vec<String>,
    category: u8,
}

#[derive(Deserialize)]
struct LocomoTurn {
    speaker: String,
    dia_id: String,
    text: String,
    #[serde(default)]
    blip_caption: Option<String>,
}

#[derive(Default)]
struct RetrievalMetrics {
    questions: usize,
    hit_at_1: usize,
    hit_at_k: usize,
    evidence_total: usize,
    evidence_recalled: usize,
    reciprocal_rank: f64,
}

struct NormalizedEvidence {
    ids: HashSet<String>,
    malformed: usize,
}

impl RetrievalMetrics {
    fn record(&mut self, relevant: &HashSet<String>, hits: &[String]) {
        self.questions += 1;
        self.evidence_total += relevant.len();
        self.evidence_recalled += hits.iter().filter(|uri| relevant.contains(*uri)).count();
        if hits.first().is_some_and(|uri| relevant.contains(uri)) {
            self.hit_at_1 += 1;
        }
        if let Some(rank) = hits.iter().position(|uri| relevant.contains(uri)) {
            self.hit_at_k += 1;
            self.reciprocal_rank += 1.0 / (rank + 1) as f64;
        }
    }

    fn merge(&mut self, other: &Self) {
        self.questions += other.questions;
        self.hit_at_1 += other.hit_at_1;
        self.hit_at_k += other.hit_at_k;
        self.evidence_total += other.evidence_total;
        self.evidence_recalled += other.evidence_recalled;
        self.reciprocal_rank += other.reciprocal_rank;
    }

    fn report(&self, label: &str) {
        println!(
            "locomo_retrieval {label} questions={} hit_at_1={:.3} hit_at_{}={:.3} mrr_at_{}={:.3} evidence_recall_at_{}={:.3}",
            self.questions,
            ratio(self.hit_at_1, self.questions),
            SEARCH_LIMIT,
            ratio(self.hit_at_k, self.questions),
            SEARCH_LIMIT,
            self.reciprocal_rank / self.questions.max(1) as f64,
            SEARCH_LIMIT,
            ratio(self.evidence_recalled, self.evidence_total),
        );
    }
}

/// Runs LoCoMo as a retrieval benchmark against the exact runtime-owned Session Recall
/// source used by Folumi. The CC BY-NC 4.0 dataset is deliberately not vendored.
#[tokio::test]
#[ignore = "external LoCoMo benchmark; set FOLUMI_LOCOMO_DATASET and run explicitly"]
async fn locomo_history_recall_retrieval_benchmark() {
    let dataset_path = std::env::var(LOCOMO_DATASET_ENV).unwrap_or_else(|_| {
        panic!("set {LOCOMO_DATASET_ENV} to the official LoCoMo data/locomo10.json path")
    });
    let max_samples = positive_env_limit(LOCOMO_MAX_SAMPLES_ENV).unwrap_or(usize::MAX);
    let max_questions = positive_env_limit(LOCOMO_MAX_QUESTIONS_ENV).unwrap_or(usize::MAX);
    let samples = load_locomo(Path::new(&dataset_path));
    assert!(!samples.is_empty(), "LoCoMo dataset contained no samples");

    let benchmark_root = tempfile::tempdir().expect("create LoCoMo benchmark root");
    let access = recall_knowledge_access();
    let mut overall = RetrievalMetrics::default();
    let mut by_category = BTreeMap::<u8, RetrievalMetrics>::new();
    let mut no_evidence = 0usize;
    let mut annotation_issues = 0usize;
    let mut no_valid_evidence = 0usize;
    let mut latencies = Vec::new();
    let mut context_bytes = Vec::new();
    let mut samples_run = 0usize;
    let mut questions_seen = 0usize;
    let mut sample_reports = Vec::new();

    for (sample_index, sample) in samples.into_iter().take(max_samples).enumerate() {
        samples_run += 1;
        let sample_root = benchmark_root.path().join(format!("sample-{sample_index}"));
        let pool = SessionPool::new_with_root_and_history_recall(&sample_root, true);
        let evidence_uris = import_locomo_conversation(&pool, &sample).await;
        pool.synchronize_history_recall(true)
            .await
            .expect("synchronize imported LoCoMo sessions");
        let source = pool.history_recall_knowledge_source();
        let mut sample_metrics = RetrievalMetrics::default();

        for qa in sample.qa.iter().take(max_questions) {
            questions_seen += 1;
            let evidence = normalized_evidence(&qa.evidence);
            if qa.evidence.iter().all(|item| item.trim().is_empty()) {
                no_evidence += 1;
                continue;
            }
            annotation_issues += evidence.malformed;
            let relevant = evidence
                .ids
                .iter()
                .filter_map(|dia_id| match evidence_uris.get(dia_id) {
                    Some(uri) => Some(uri.clone()),
                    None => {
                        annotation_issues += 1;
                        eprintln!(
                            "locomo_retrieval annotation_issue sample={} question={:?} unmapped_evidence={dia_id}",
                            sample.sample_id, qa.question
                        );
                        None
                    }
                })
                .collect::<HashSet<_>>();
            if relevant.is_empty() {
                no_valid_evidence += 1;
                continue;
            }
            let started = Instant::now();
            let page = search(&source, &access, &qa.question).await;
            latencies.push(started.elapsed());
            context_bytes.push(page.hits.iter().map(|hit| hit.snippet.len()).sum::<usize>());
            let hits = page
                .hits
                .iter()
                .filter_map(|hit| hit.uri.clone())
                .collect::<Vec<_>>();
            sample_metrics.record(&relevant, &hits);
            by_category
                .entry(qa.category)
                .or_default()
                .record(&relevant, &hits);
        }

        sample_metrics.report(&format!("sample={}", sample.sample_id));
        sample_reports.push(serde_json::json!({
            "sample_id": sample.sample_id,
            "metrics": metric_report(&sample_metrics),
        }));
        overall.merge(&sample_metrics);
    }

    assert!(
        overall.questions > 0,
        "LoCoMo selection contained no evidence-backed questions"
    );
    overall.report(&format!("overall samples={samples_run}"));
    for (category, metrics) in &by_category {
        metrics.report(&format!("category={category}"));
    }
    latencies.sort_unstable();
    context_bytes.sort_unstable();
    let p50_latency = percentile(&latencies, 50);
    let p95_latency = percentile(&latencies, 95);
    let p95_context_bytes = percentile(&context_bytes, 95);
    println!(
        "locomo_retrieval diagnostics no_evidence={} no_valid_evidence={} annotation_issues={} search_p50_ms={:.3} search_p95_ms={:.3} context_p95_bytes={}",
        no_evidence,
        no_valid_evidence,
        annotation_issues,
        duration_ms(p50_latency),
        duration_ms(p95_latency),
        p95_context_bytes,
    );
    assert!(
        p95_context_bytes <= SEARCH_LIMIT * HISTORY_RECALL_MAX_SNIPPET_BYTES,
        "LoCoMo search snippets exceeded the configured context bound"
    );
    write_locomo_report(
        &overall,
        &by_category,
        sample_reports,
        LocomoDiagnostics {
            samples_run,
            questions_seen,
            no_evidence,
            no_valid_evidence,
            annotation_issues,
            search_p50_ms: duration_ms(p50_latency),
            search_p95_ms: duration_ms(p95_latency),
            context_p95_bytes: p95_context_bytes,
        },
    );
}

struct LocomoDiagnostics {
    samples_run: usize,
    questions_seen: usize,
    no_evidence: usize,
    no_valid_evidence: usize,
    annotation_issues: usize,
    search_p50_ms: f64,
    search_p95_ms: f64,
    context_p95_bytes: usize,
}

fn metric_report(metrics: &RetrievalMetrics) -> serde_json::Value {
    serde_json::json!({
        "questions": metrics.questions,
        "hit_at_1_count": metrics.hit_at_1,
        "hit_at_k_count": metrics.hit_at_k,
        "evidence_total": metrics.evidence_total,
        "evidence_recalled": metrics.evidence_recalled,
        "hit_at_1": ratio(metrics.hit_at_1, metrics.questions),
        "hit_at_k": ratio(metrics.hit_at_k, metrics.questions),
        "mrr_at_k": metrics.reciprocal_rank / metrics.questions.max(1) as f64,
        "evidence_recall_at_k": ratio(metrics.evidence_recalled, metrics.evidence_total),
    })
}

fn write_locomo_report(
    overall: &RetrievalMetrics,
    by_category: &BTreeMap<u8, RetrievalMetrics>,
    sample_reports: Vec<serde_json::Value>,
    diagnostics: LocomoDiagnostics,
) {
    let Ok(output_path) = std::env::var(LOCOMO_OUTPUT_ENV) else {
        return;
    };
    let generated_at = chrono::Utc::now();
    let run_id = std::env::var(BENCHMARK_RUN_ID_ENV)
        .unwrap_or_else(|_| generated_at.format("locomo-%Y%m%dT%H%M%SZ").to_string());
    let category_reports = by_category
        .iter()
        .map(|(category, metrics)| (category.to_string(), metric_report(metrics)))
        .collect::<serde_json::Map<_, _>>();
    let report = serde_json::json!({
        "schema_version": 1,
        "benchmark": "locomo_history_recall_retrieval",
        "run_id": run_id,
        "generated_at": generated_at.to_rfc3339(),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "provenance": {
            "folumi_revision": env_or_unknown(FOLUMI_REVISION_ENV),
            "runtime_revision": env_or_unknown(RUNTIME_REVISION_ENV),
            "dataset": "LoCoMo locomo10.json",
            "dataset_revision": env_or_unknown(LOCOMO_REVISION_ENV),
        },
        "configuration": {
            "search_limit": SEARCH_LIMIT,
            "max_snippet_bytes": HISTORY_RECALL_MAX_SNIPPET_BYTES,
            "conversation_import": "date_and_blip_caption_prefixed_utterances",
            "max_samples": positive_env_limit(LOCOMO_MAX_SAMPLES_ENV),
            "max_questions_per_sample": positive_env_limit(LOCOMO_MAX_QUESTIONS_ENV),
        },
        "dataset_counts": {
            "samples": diagnostics.samples_run,
            "questions_seen": diagnostics.questions_seen,
            "questions_scored": overall.questions,
            "no_evidence": diagnostics.no_evidence,
            "no_valid_evidence": diagnostics.no_valid_evidence,
            "annotation_issues": diagnostics.annotation_issues,
        },
        "overall": metric_report(overall),
        "categories": category_reports,
        "samples": sample_reports,
        "diagnostics": {
            "search_p50_ms": diagnostics.search_p50_ms,
            "search_p95_ms": diagnostics.search_p95_ms,
            "context_p95_bytes": diagnostics.context_p95_bytes,
        },
    });
    let output_path = Path::new(&output_path);
    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).unwrap_or_else(|error| {
            panic!(
                "failed to create LoCoMo report directory {}: {error}",
                parent.display()
            )
        });
    }
    let json = serde_json::to_vec_pretty(&report).expect("serialize LoCoMo benchmark report");
    fs::write(output_path, json).unwrap_or_else(|error| {
        panic!(
            "failed to write LoCoMo report {}: {error}",
            output_path.display()
        )
    });
    println!("locomo_retrieval report={}", output_path.display());
}

fn env_or_unknown(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| "unknown".into())
}

fn load_locomo(path: &Path) -> Vec<LocomoSample> {
    let bytes = fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read LoCoMo dataset {}: {error}", path.display())
    });
    serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        panic!("failed to parse LoCoMo dataset {}: {error}", path.display())
    })
}

async fn import_locomo_conversation(
    pool: &Arc<SessionPool>,
    sample: &LocomoSample,
) -> HashMap<String, String> {
    let mut sessions = sample
        .conversation
        .iter()
        .filter_map(|(key, value)| session_number(key).map(|number| (number, value)))
        .collect::<Vec<_>>();
    sessions.sort_unstable_by_key(|(number, _)| *number);
    let mut evidence_uris = HashMap::new();

    for (number, value) in sessions {
        let turns = serde_json::from_value::<Vec<LocomoTurn>>(value.clone())
            .expect("parse LoCoMo conversation session");
        let date = sample
            .conversation
            .get(&format!("session_{number}_date_time"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("unknown date");
        let session_id = create_session(pool, false).await;
        let session = pool.open_runtime_session(&session_id).await.unwrap();
        for turn in turns {
            // LoCoMo contains a conversation between two people, not user/assistant roles.
            // Importing each utterance as a user entry preserves every evidence turn in
            // runtime's existing turn projector without inventing a second recall index.
            // The official QA context also contains the Session date and any BLIP image
            // caption, both of which are required to answer temporal or image questions.
            let text = locomo_turn_text(date, &turn);
            let entry_id = session
                .append_message(tutor_agent::chat::user_message(&text))
                .await
                .expect("append LoCoMo turn");
            evidence_uris.insert(turn.dia_id, format!("chat:{session_id}:{entry_id}"));
        }
    }
    evidence_uris
}

fn locomo_turn_text(date: &str, turn: &LocomoTurn) -> String {
    let mut text = format!("DATE: {date}\n{}: {}", turn.speaker, turn.text);
    if let Some(caption) = turn
        .blip_caption
        .as_deref()
        .filter(|caption| !caption.trim().is_empty())
    {
        text.push_str("\nSHARED IMAGE: ");
        text.push_str(caption.trim());
    }
    text
}

fn session_number(key: &str) -> Option<usize> {
    key.strip_prefix("session_")?.parse().ok()
}

fn normalized_evidence(raw: &[String]) -> NormalizedEvidence {
    let mut ids = HashSet::new();
    let mut malformed = 0usize;
    for token in raw
        .iter()
        .flat_map(|item| item.split(|ch: char| ch == ';' || ch.is_whitespace()))
        .map(|item| {
            item.trim()
                .trim_matches(|ch| ch == '(' || ch == ')' || ch == ',')
        })
        .filter(|item| !item.is_empty())
    {
        if let Some(id) = canonical_dialog_id(token) {
            ids.insert(id);
        } else {
            malformed += 1;
        }
    }
    NormalizedEvidence { ids, malformed }
}

fn canonical_dialog_id(raw: &str) -> Option<String> {
    let repaired = raw
        .strip_prefix("D:")
        .map(|rest| format!("D{rest}"))
        .unwrap_or_else(|| raw.to_string());
    let (session, turn) = repaired.strip_prefix('D')?.split_once(':')?;
    let session = session.parse::<usize>().ok()?;
    let turn = turn.parse::<usize>().ok()?;
    Some(format!("D{session}:{turn}"))
}

fn positive_env_limit(name: &str) -> Option<usize> {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    numerator as f64 / denominator.max(1) as f64
}

#[test]
fn locomo_adapter_recognizes_sessions_and_normalizes_combined_evidence() {
    assert_eq!(session_number("session_17"), Some(17));
    assert_eq!(session_number("session_17_date_time"), None);
    assert_eq!(session_number("speaker_a"), None);
    let evidence = normalized_evidence(&["(D8:6); D9:17 D30:05".into(), "D:11:26 | D".into()]);
    assert_eq!(
        evidence.ids,
        HashSet::from([
            "D8:6".into(),
            "D9:17".into(),
            "D30:5".into(),
            "D11:26".into(),
        ])
    );
    assert_eq!(evidence.malformed, 2);
}

#[test]
fn locomo_import_preserves_date_and_image_caption() {
    let text = locomo_turn_text(
        "7 May 2023",
        &LocomoTurn {
            speaker: "Caroline".into(),
            dia_id: "D1:3".into(),
            text: "I went to the support group.".into(),
            blip_caption: Some("a sunrise painting".into()),
        },
    );
    assert_eq!(
        text,
        "DATE: 7 May 2023\nCaroline: I went to the support group.\nSHARED IMAGE: a sunrise painting"
    );
}

async fn create_session(pool: &Arc<SessionPool>, temporary: bool) -> String {
    pool.create_with_config(SessionCreateConfig {
        capability: "chat".into(),
        kb: None,
        knowledge_bases: vec![],
        notebook_enabled: false,
        llm: None,
        search: None,
        embedding: None,
        assistant: AssistantSessionConfig::default(),
        temporary,
    })
    .await
    .unwrap()
}

async fn append_turn(pool: &SessionPool, id: &str, user: &str, assistant: &str) {
    let session = pool.open_runtime_session(id).await.unwrap();
    session
        .append_message(tutor_agent::chat::user_message(user))
        .await
        .unwrap();
    session
        .append_message(tutor_agent::chat::assistant_message(assistant))
        .await
        .unwrap();
}

fn recall_knowledge_access() -> KnowledgeAccessContext {
    let mut scope =
        llm_harness_runtime_knowledge::KnowledgeScope::new(tutor_rag::AGENT_KNOWLEDGE_NAMESPACE);
    scope.tenant = Some(LOCAL_USER_ID.into());
    KnowledgeAccessContext::new(scope, PrincipalRef::new(LOCAL_USER_ID, "local_user"))
}

async fn search(
    source: &Arc<dyn KnowledgeSource>,
    access: &KnowledgeAccessContext,
    query: &str,
) -> SourceSearchPage {
    let recall = SessionRecallAccessContext::new(SessionRecallScope {
        namespace: SESSION_RECALL_NAMESPACE.into(),
        tenant: Some(LOCAL_USER_ID.into()),
        project: None,
        attributes: Default::default(),
    });
    let run = RunContext::new(RunRequest::from_text(query).with_extension(recall));
    source
        .search(
            KnowledgeRequestContext { run: &run, access },
            SourceSearchRequest {
                query: query.into(),
                filters: vec![],
                limit: SEARCH_LIMIT,
                cursor: None,
            },
            CancellationToken::new(),
        )
        .await
        .unwrap()
}

fn belongs_to_session(uri: Option<&str>, session_id: &str) -> bool {
    uri.is_some_and(|uri| uri.starts_with(&format!("chat:{session_id}:")))
}

fn approximate_tokens(text: &str) -> usize {
    let (ascii, non_ascii) = text
        .chars()
        .fold((0usize, 0usize), |(ascii, non_ascii), ch| {
            if ch.is_ascii() {
                (ascii + 1, non_ascii)
            } else {
                (ascii, non_ascii + 1)
            }
        });
    ((ascii as f64 / 4.0) + (non_ascii as f64 * 1.2)).ceil() as usize
}

fn percentile<T: Copy>(values: &[T], percentile: usize) -> T {
    let index = ((values.len() - 1) * percentile).div_ceil(100);
    values[index]
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

mod answer;
