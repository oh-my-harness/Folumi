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
use tokio_util::sync::CancellationToken;

const SEARCH_LIMIT: usize = 3;
const SESSION_RECALL_NAMESPACE: &str = "folumi-session-history";
const LOCAL_USER_ID: &str = "local-user";

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

async fn create_session(pool: &Arc<SessionPool>, temporary: bool) -> String {
    pool.create_with_config(SessionCreateConfig {
        capability: "chat".into(),
        kb: None,
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
