# Runtime Learner Memory B3 Acceptance

Date: 2026-07-29

Runtime revision:
`ee97890b00b6a549dfb3b94519997af613adf456`

This report closes the quality, security, persistence, and engineering
measurement gate for the Learner Memory runtime migration. Local benchmarks use
deterministic storage and model fixtures so they can run without credentials
and isolate the runtime/product boundary from provider latency.

## Environment

- Windows 11
- Intel Core i7-12700H, 14 cores / 20 logical processors
- Rust `1.97.1`, `x86_64-pc-windows-msvc`
- Cargo test profile with incremental compilation disabled
- Local Markdown/JSONL `FileMemoryBackend`
- Deterministic mock model for workflow measurements

## Representative Chat Quality

The acceptance fixture stores `Prefers diagrams.`, asks Chat to explain
something in the user's preferred way, and compares the required behavior
across the migration boundary.

| Quality invariant | Legacy expectation | Runtime result |
| --- | --- | --- |
| Retrieve only when personalization is relevant | on-demand read | `knowledge_search` then exact `knowledge_read` |
| Apply the learned preference naturally | visual explanation | `I will explain this with diagrams.` |
| Do not narrate internal memory access | no tool narration | no tool narration |
| Do not require a visible evidence citation for personalization | no citation marker | no citation marker |
| Respect disabled/read-only profiles | no unauthorized mutation | mutation Tools absent |

The runtime adapter reads the same canonical L3 entry as the legacy path, so
the migration changes orchestration and trust boundaries rather than retrieval
content. The behavior is enforced by
`read_only_chat_uses_memory_without_forcing_a_visible_citation`, the disabled
profile tests, and the runtime Tool projection checks.

## Local Boundary Latency

The adapter benchmark performs five warm-up rounds followed by 100 measured
rounds over the local file backend. The maintenance benchmark performs five
warm-up runs followed by 30 measured structured workflow runs.

| Operation | P50 | P95 |
| --- | ---: | ---: |
| Runtime Knowledge search | 5.734 ms | 6.424 ms |
| Runtime Knowledge exact read | 5.531 ms | 6.266 ms |
| Runtime Memory write | 3.738 ms | 4.338 ms |
| Runtime Memory forget | 3.727 ms | 4.615 ms |
| Maintenance workflow, deterministic mock | 5.519 ms | 6.569 ms |

These figures measure local runtime composition, authorization, parsing, and
storage overhead. A production model adds provider and network latency to the
maintenance workflow.

Reproduce the measurements with:

```powershell
cargo test -p tutor-web --lib `
  b3_search_read_write_forget_latency_baseline -- `
  --ignored --nocapture
cargo test -p tutor-agent --lib `
  b3_maintenance_workflow_latency_baseline -- `
  --ignored --nocapture
```

## Tokens and Durable Session

The existing deterministic Chat integration reports 240 input tokens, 36
output tokens, 12 cache-read tokens, and 8 cache-write tokens. Its durable
runtime Session occupies 5,572 bytes.

The Learner Memory-specific fixture executes `knowledge_search` and
`knowledge_read`, then scans every raw Session file:

| Field | Value |
| --- | ---: |
| Durable Learner Memory Session files | 5,086 bytes |
| `knowledge_read` receipt persisted | yes |
| Full read-body sentinel persisted | no |
| Idempotency-key sentinel persisted | no |
| Trusted principal/profile attributes persisted | no |

The unique full-body sentinel is placed beyond the bounded search snippet. Its
absence proves that the exact read body was not converted into a durable search
receipt. Access context and policy values remain run-scoped.

Reproduce with:

```powershell
cargo test -p tutor-web --lib `
  memory_read_persists_a_receipt_but_not_body_or_trusted_context -- `
  --nocapture
cargo test -p tutor-agent --test mock_integration `
  chat_uses_runtime_knowledge_tools_and_keeps_read_bodies_out_of_session -- `
  --nocapture
```

## Security and Isolation

Automated tests cover:

- missing trusted context and unknown sources fail closed;
- disabled and read-only profiles cannot mutate memory;
- interactive assembly fails without a live approval coordinator;
- write and exact-reference forget do not mutate before Web approval;
- denial, timeout, stop, disconnect, wrong-session response, stale response,
  and response replay fail closed;
- source access cannot be replayed for another principal;
- forged source, hidden layer, item, revision, filter, path, and target are
  rejected;
- the secure policy rejects secret-like content and derives the idempotency
  key from a process secret that is never exposed to the model;
- concurrent retries create one entry, stale delete does not mutate, expired
  entries are invisible, and receipts read back at the exact revision.

## Maintenance and Product Transactions

Update, Check, and Dedupe mount one runtime Knowledge plugin. A trusted
`WorkflowRunRequest` limits each run to the target's allowed layers and
surfaces. Search results are candidates only; the evidence tracker accepts a
`canonical_reference` after a successful exact runtime read.

The model returns a product `MemoryChangeSet` and never mutates the file
directly. Product acceptance tests prove:

- unread, forged, and wrong-layer evidence is rejected with one bounded repair
  opportunity;
- partial selection applies only accepted change IDs;
- a stale base document revision rejects the complete apply;
- one invalid selected change leaves the document unchanged;
- successful apply creates one exact undo snapshot.

This follows the runtime #92 decision: reviewed multi-change application is a
product domain transaction over the same backend mutation primitives, not a
second Agent-facing memory protocol.

## Release Gate

The final release gate is:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts/check-tool-projections.ps1
npm --prefix web-ui test
npm --prefix web-ui run build
```

## Conclusion

B3 preserves representative personalization behavior while moving discovery,
exact reads, writes, forget, approval, Session projection, and workflow request
propagation to the runtime. Product code retains only Learner Memory data
semantics, authorization mapping, review/apply, and the shared transactional
file backend. The legacy Agent-facing memory and maintenance evidence tools are
deleted.
