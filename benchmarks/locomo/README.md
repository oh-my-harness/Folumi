# LoCoMo benchmark results

This directory keeps machine-readable, append-only retrieval and Agent answer benchmark results plus generated comparison charts. The LoCoMo dataset itself is CC BY-NC 4.0 and is never copied into this repository.

## Record a retrieval run

Run the existing benchmark with an explicit output path and provenance:

```powershell
$env:FOLUMI_LOCOMO_DATASET='C:\path\to\locomo\data\locomo10.json'
$env:FOLUMI_LOCOMO_OUTPUT='benchmarks\locomo\results\2026-08-04-runtime-66f983d-fts5-debug.json'
$env:FOLUMI_BENCHMARK_RUN_ID='runtime-66f983d-fts5-debug'
$env:FOLUMI_BENCHMARK_FOLUMI_REVISION=(git rev-parse HEAD)
$env:FOLUMI_BENCHMARK_RUNTIME_REVISION='66f983d0a4c024c34e70bff3587cd4c44fb3b26f'
$env:FOLUMI_BENCHMARK_LOCOMO_REVISION='3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376'
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib locomo_history_recall_retrieval_benchmark -- --ignored --nocapture
```

Use `--release` for a formal latency baseline. Debug and Release quality metrics are comparable when all retrieval settings and revisions are identical, but their latency metrics are not.

Every result contains raw hit/evidence counts as well as rates, configuration, category and per-conversation breakdowns, dataset counts, latency, and revision provenance. Do not edit an old result to represent a new implementation; add another JSON file with a new `run_id`.

## Regenerate the chart

```powershell
.\scripts\render-locomo-benchmarks.ps1
```

The renderer reads every schema-v1 JSON file in `results/`, sorts runs by timestamp, and writes `charts/retrieval-comparison.svg`. The upper chart compares overall runs; the lower chart shows category performance for the latest run.

Runtime changes such as hybrid lexical/vector retrieval, candidate fusion, temporal filtering, neighboring-turn expansion, diversity controls, and reranking belong in `llm-harness-runtime-session-recall`. Folumi should retain only this adapter, product policy, and regression baselines.

## Record an Agent answer run

The answer benchmark makes one online model request per selected question and runs the real Folumi Chat Agent with runtime History Recall. Start with a small smoke selection and inspect cost before attempting the complete 1,986-question dataset:

```powershell
$env:FOLUMI_LOCOMO_DATASET='C:\path\to\locomo\data\locomo10.json'
$env:FOLUMI_LOCOMO_MAX_SAMPLES='1'
$env:FOLUMI_LOCOMO_MAX_QUESTIONS='5'
$env:FOLUMI_LOCOMO_ANSWER_OUTPUT='benchmarks\locomo\answer-results\2026-08-04-smoke.json'
$env:FOLUMI_BENCHMARK_RUN_ID='answer-smoke'
$env:FOLUMI_BENCHMARK_FOLUMI_REVISION=(git rev-parse HEAD)
$env:FOLUMI_BENCHMARK_RUNTIME_REVISION='66f983d0a4c024c34e70bff3587cd4c44fb3b26f'
$env:FOLUMI_BENCHMARK_LOCOMO_REVISION='3eb6f2c585f5e1699204e3c3bdf7adc5c28cb376'
$env:LLM_PROVIDER='anthropic' # or openai / deepseek
$env:LLM_MODEL='your-fixed-model-id'
# Set the matching provider API-key variable without printing or committing it.
$env:CARGO_BUILD_JOBS='1'
cargo test -p tutor-web --lib locomo_agent_answer_accuracy_benchmark -- --ignored --nocapture
```

Clear `FOLUMI_LOCOMO_MAX_SAMPLES` and `FOLUMI_LOCOMO_MAX_QUESTIONS` for a formal full run. Each question uses a separate temporary answer Session so earlier benchmark answers never enter History Recall and leak into later questions.

By default the report excludes question, reference-answer, and prediction text so committed aggregate results do not redistribute the dataset. Set `FOLUMI_LOCOMO_INCLUDE_TEXT=true` only for a local diagnostic report, and do not commit that report.

The scorer follows LoCoMo's token-F1 category rules for categories 1–4. Category 5 uses free-form abstention (`No information available`) instead of the official paper's multiple-choice presentation, so its abstention accuracy is a Folumi product metric and must not be compared directly with paper scores.

Generate the answer comparison chart after saving at least one run:

```powershell
.\scripts\render-locomo-answer-benchmarks.ps1
```

The answer renderer reads `answer-results/` and writes `charts/answer-comparison.svg`. Reports include overall and per-category answer scores, exact match, tool-use rates, errors, latency, provider token usage, and provider-reported cost.
