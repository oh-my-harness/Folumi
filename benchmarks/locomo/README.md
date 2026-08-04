# LoCoMo benchmark results

This directory keeps machine-readable, append-only History Recall benchmark results and generated comparison charts. The LoCoMo dataset itself is CC BY-NC 4.0 and is never copied into this repository.

## Record a run

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
