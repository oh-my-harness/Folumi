# Runtime Learner Memory B3 Migration Plan

> Status: in progress (Phase 2 complete; Phase 0 runtime gates open) |
> Date: 2026-07-28 | Tracks:
> [llm-tutor issue #3](https://github.com/oh-my-harness/llm-tutor/issues/3) |
> Upstream review:
> [llm-harness-runtime PR #82](https://github.com/oh-my-harness/llm-harness-runtime/pull/82) |
> Reviewed development revision:
> [`c4c0ddf`](https://github.com/oh-my-harness/llm-harness-runtime/commit/c4c0ddf029c1cb949290dc16ccc22f4ae14a3fc5)

## 1. Goal

Move Learner Memory from the product-owned `read_memory` / `write_memory`
Agent protocol to the runtime Knowledge and Memory composition:

```text
Tutor Agent
  ├─ KnowledgePlugin -> knowledge_search / knowledge_read
  │    -> LearnerMemoryKnowledgeSource
  │    -> FileMemoryBackend
  └─ MemoryPlugin -> memory_write / memory_forget
       -> MemoryService
       -> LearnerMemoryWritePolicy
       -> LearnerMemoryWriteStore
       -> FileMemoryBackend
```

The migration must preserve the current product behavior:

- the visible L1/L2/L3 Markdown and JSONL layout;
- Update, Check, and Dedupe maintenance flows;
- review before apply and partial acceptance;
- stale base-revision rejection;
- one atomic accepted change set and one-level undo;
- Tutor-specific resource permissions;
- natural use of memory without narrating internal reads.

After B3, `llm-tutor` shall not maintain a second Agent-facing Learner Memory
protocol. Product APIs and UI may continue to use product domain operations;
they must share the same file backend and mutation invariants as the runtime
adapters.

Tutor Memory (`TutorMemoryStore`, `remember_for_later`) is a separate ownership
and lifecycle model and is not part of B3.

## 2. Confirmed Baselines

### Product baseline

The product currently pins every runtime crate to `83bef164`.

The active Learner Memory paths are:

- `tutor-tools::ReadMemoryTool` accepts a model-selected scope and reads an
  entire L3 Markdown document.
- `tutor-tools::WriteMemoryTool` trusts a model-provided `approved: true` and
  appends directly to `L3/preferences.md`.
- `CapabilityRouter` manually installs both tools whenever
  `learner_memory_access` is true.
- the web `MemoryStore` owns L1 events, L2/L3 entries, document revisions,
  evidence references, review/apply, stale checks, and undo.
- the maintenance workflow already runs through runtime `WorkflowEngine`, but
  still mounts product-specific L1/L2 evidence Tools and calls `run()` without
  a trusted `WorkflowRunRequest`.
- the browser renders an approval dialog and sends `approval_response`, but
  the server currently only emits a trace for that message; no live approval
  waiter or `BeforeToolCallHook` consumes it.
- course Knowledge already uses `knowledge_search` / `knowledge_read` with
  strict runtime citation validation.

### Runtime baseline at `c4c0ddf`

Runtime PR #82 provides:

- `llm-harness-runtime-memory`;
- `MemoryService`, `MemoryStore`, `MemoryWritePolicy`, and `MemoryPlugin`;
- `memory_write` and `memory_forget`;
- trusted `MemoryProvenance` and optional `MemorySessionId`;
- `SecureMemoryWritePolicy` with normalization, size/kind limits, secret
  rejection, TTL selection, and HMAC idempotency keys;
- write/delete authorization through `KnowledgeAccessControl`;
- stable revisioned receipts and immediate/eventual visibility contracts;
- reusable Knowledge source and Memory store contract tests;
- `WorkflowRunRequest` propagation to every workflow LLM step.

The reviewed upstream PR is still draft. At planning time both Ubuntu and
Windows CI checks report failure and expose no job steps or downloadable
failure log. `c4c0ddf` is therefore a development API baseline, not the final
production pin.

### Confirmed protocol gaps

Three integration decisions must be closed before the destructive cutover:

1. `MemoryStore` exposes one `upsert` or `delete`; it has no batch,
   compare-and-swap, or transaction contract for an accepted maintenance
   change set.
2. `memory_write` describes content as user-approved, but
   `SecureMemoryWritePolicy` does not itself require a trusted approval
   extension. Authorization and/or a Tool approval hook must provide that
   boundary.
3. `KnowledgeCitationPolicy::RequireWhenEvidenceRead` is global to one
   `KnowledgePlugin`. A single registry containing course Knowledge and
   Learner Memory would require visible citations after a memory-only read,
   while two plugins would register duplicate `knowledge_search` /
   `knowledge_read` Tool names. Course evidence must remain strictly cited,
   while personalization memory must remain citation-optional.

These gaps are recorded in `docs/framework-feedback.md`. Phase 0 resolves them
through upstream issues, PR changes, or an explicit documented runtime contract
before the affected cutover phase.

## 3. Ownership Boundaries

### Runtime owns

- `RunRequest`, `RunContext`, and `WorkflowRunRequest`.
- generic `knowledge_search`, `knowledge_read`, `memory_write`, and
  `memory_forget` Tool schemas.
- Tool orchestration, cancellation, and Session projection.
- common Knowledge access-control and exact-reference contracts.
- `MemoryService` composition, provenance creation, policy invocation, and
  receipt validation.
- secret rejection, TTL limits, and idempotency-key generation through the
  runtime policy contract.
- source/store semantic contract tests.

### Product owns

- Learner Memory file paths and L1/L2/L3 meanings.
- Markdown/JSONL parsing and serialization.
- stable entry markers and the mapping to opaque runtime item IDs.
- the `LearnerMemoryKnowledgeSource` read adapter.
- the `LearnerMemoryWriteStore` mutation adapter.
- the policy that maps an allowed runtime memory kind to a product target.
- the server-created access profile and interactive user approval bridge.
- maintenance prompts, findings, change sets, review UI, and accepted IDs.
- one shared mutation primitive that preserves document CAS, atomic write, and
  undo for both runtime and product commands.
- TTL visibility filtering and expired-entry cleanup in the file backend.

### Product must not own

- compatibility wrappers for `read_memory` or `write_memory`;
- a second generic search/read or memory service protocol;
- model-provided user, tenant, raw path, access profile, approval, provenance,
  idempotency key, or final TTL;
- full memory bodies persisted in runtime Session or compaction;
- automatic capture or inferred-profile writes.

## 4. Fixed Design Decisions

### 4.1 One Agent Knowledge registry

An Agent run gets one registry and one pair of generic Knowledge Tools. The
current course-only `KnowledgeRuntime` becomes a source-composable runtime
assembly that may contain:

- the selected `LanceDbKnowledgeSource`;
- `LearnerMemoryKnowledgeSource`;
- both; or
- neither.

Do not install two `KnowledgePlugin` instances with duplicate Tool names.
The product authorizer becomes source-aware and evaluates the same trusted
access context for course and memory resources.

The final implementation waits for a source-aware citation policy or another
upstream-endorsed solution. It shall not weaken the existing course Knowledge
gate from `RequireWhenEvidenceRead` to `ValidateIfPresent`, and it shall not
copy runtime citation state into a product validator.

### 4.2 Trusted access profiles

The web layer constructs one non-serializable, run-scoped
`KnowledgeAccessContext`. Its product namespace contains server-derived course
and Learner Memory attributes. The model never receives or constructs it.

Learner Memory has three explicit profiles:

| Profile | Memory read source | Memory Plugin | Allowed actions |
| --- | --- | --- | --- |
| `Disabled` | absent | absent | none |
| `ReadOnly` | present | absent | discover/search/read |
| `InteractiveMutation` | present | present | read plus approved write/delete |

For a bound Tutor, `learner_memory_access = false` selects `Disabled`.
Ordinary enabled Tutor and unbound local-user runs select `ReadOnly` unless a
live approval coordinator is installed. `InteractiveMutation` is rejected at
assembly time if that coordinator is absent.

The CLI selects `ReadOnly` by default. It may opt into
`InteractiveMutation` only when `TerminalApprover` is explicitly installed;
non-interactive CLI runs never receive write/delete authority.

Generic course Knowledge Tools may still be present when Learner Memory is
disabled; the memory source and mutation Tools are what must be absent.

`MemorySessionId` is attached from the stable product/runtime session ID.
The access authorization version includes the current Tutor permission state
so a resumed run cannot retain broader memory access after permissions change.

### 4.3 Approval is a server event, not a Tool argument

`memory_write` and `memory_forget` always pass through a memory-specific
`BeforeToolCallHook` backed by the web approval coordinator:

1. the runtime emits the exact Tool name and bounded arguments to the hook;
2. the coordinator creates a random, run-bound request ID and sends an
   `approval_request` to the browser;
3. `approval_response` resolves only the matching active run/request;
4. approval is single-use, times out, and is cancelled on stop, disconnect, or
   run completion;
5. denial returns a typed Tool failure and performs no file mutation.

This gate is mandatory for Learner Memory mutation and is independent of the
optional global/code-execution approval setting. A model statement or Tool
argument cannot satisfy it. The legacy `approved` field is deleted.

The write policy still checks a trusted mutation marker in the run access
profile before delegating to `SecureMemoryWritePolicy`. This gives defense in
depth if a Tool is ever mounted without the hook. Delete is protected by the
same source-aware authorizer plus the mandatory hook.

### 4.4 Source, domain, and item mapping

Use source ID `llm-tutor.learner-memory`. Runtime references never expose an
absolute filesystem path.

| Layer | Item ID | Domain | Ordinary Chat | Maintenance |
| --- | --- | --- | --- | --- |
| L1 event | `l1/{surface}/{event-id}` | `learner-memory.event.{surface}` | hidden | read for L2; bounded read for `L3/recent` |
| L2 entry | `l2/{surface}/{marker}` | `learner-memory.summary.{surface}` | hidden | read for allowed L3 targets |
| L3 entry | `l3/{kind}/{marker}` | `learner-memory.profile.{kind}` | search/read | target/current-state reads |

Declared source filters are limited to `layer`, `surface`, and `kind`.
The backend intersects every request with the server profile and target
allowlist. A model-supplied source ID, domain, filter, item ID, or ref is only a
selector and never an authorization credential.

Search returns one lightweight hit per entry/event: exact ref, title, short
snippet, category metadata, and optional logical URI. Read returns one bounded
entry/event body. It never returns a whole L3 document merely because one entry
matched.

`*` is the source's documented catalog query for maintenance listing. Cursor
validation, result caps, cancellation, and deterministic ordering are covered
by source contract and boundary tests.

### 4.5 Two revision layers

Runtime entry refs and product maintenance documents intentionally use
different revisions:

- `KnowledgeRef.revision` is the hash of the normalized entry plus its durable
  metadata. Updating another entry does not make this ref stale.
- `MemoryChangeSet.base_revision` remains the hash of the complete target
  Markdown document. It protects an accepted multi-change apply from any
  concurrent document edit.

`knowledge_read` with an old entry revision returns `StaleReference` and may
include the latest ref. `memory_forget` requires an exact current entry ref.
A write receipt is immediately readable through the same source ID and entry
revision.

### 4.6 Durable metadata stays with the Markdown entry

L3 Markdown remains the human-readable canonical store. Each runtime-managed
entry receives a versioned hidden metadata envelope adjacent to its existing
marker. The envelope contains only the fields needed to reconstruct:

- stable item identity;
- runtime kind and product target;
- provenance;
- idempotency key;
- optional expiry;
- schema version.

Existing marker-only entries remain readable as legacy, non-expiring entries.
Do not introduce a sidecar database in B3: splitting text and metadata across
files would add a second atomicity and recovery problem. Secrets are never
stored; the HMAC idempotency output is not the HMAC key.

The first explicit write kind is `preference`, preserving the current
`L3/preferences.md` target. Additional kinds require a product schema and
allowlist change; the model cannot select an arbitrary file or section.

### 4.7 Idempotency, TTL, and forget

- A repeated idempotency key returns the existing entry receipt and does not
  append another Markdown line.
- Expired entries are excluded from search and read before scoring or content
  construction.
- Expired entries are removed by a deterministic product cleanup operation;
  expiry does not depend on an Agent run.
- `memory_forget` deletes only the exact authorized L3 item/revision and writes
  an undo snapshot.
- A forged source, hidden layer, missing item, or stale revision fails without
  revealing another entry.
- Runtime write/delete receipts use `MemoryVisibility::Visible` because the
  file backend is immediate.

### 4.8 Maintenance remains proposal first

Maintenance LLM steps receive their target-specific access through
`WorkflowRunRequest`. The generic Knowledge source replaces the product
`list/search/read_memory_event` and `list/search/read_memory_entry` Tool
families after equivalent evidence coverage is proven.

The LLM still emits product `MemoryWorkflowOutput`; it never directly mutates
files. The route still requires accepted change IDs and an unchanged document
base revision.

The final apply path must use the same backend transaction primitive as
runtime writes. Before cutover, upstream must confirm one of:

1. product-owned strong-consistency business/executor steps are the intended
   application transaction boundary, as suggested by the runtime design; or
2. runtime adds a formal batch/CAS mutation contract.

If option 1 is confirmed, `FileMemoryBackend::apply_change_set` is a product
domain command, not a parallel Agent protocol. If option 2 is selected, B3
waits for and consumes that API. Do not loop over `MemoryService::write/delete`
and call the result atomic.

## 5. Implementation Phases

### Phase 0: Close upstream and dependency gates

- [ ] Open or link one upstream issue for source-aware Knowledge citation
  policy.
- [ ] Open or link one upstream issue/decision for strong-consistency
  application batch/CAS semantics.
- [ ] Ask upstream to clarify the trusted approval expectation for
  `memory_write`; consume a runtime grant if one is added, otherwise document
  the required AccessControl + Tool-hook composition.
- [ ] Wait for runtime PR #82 to be non-draft, green, and merged.
- [ ] Pin all `llm-harness-*` crates, including
  `llm-harness-runtime-memory`, to one immutable merged revision.
- [ ] Align `llm_adapter` to the revision required by that runtime.
- [ ] Run Cargo metadata/tree checks and reject mixed runtime revisions.

Adapter and contract-test development may use `c4c0ddf`; Chat cutover and
legacy deletion may not.

### Phase 1: Isolate the product file backend

- [x] Rename web `MemoryStore` to `FileMemoryBackend` and update product
  callers without changing behavior.
- [x] Move entry identity, entry revision, metadata-envelope parsing, and
  related mutation helpers behind this backend.
- [x] Preserve legacy marker parsing.
- [x] Make writes use same-directory temporary files plus atomic replacement;
  keep an exact pre-write undo snapshot.
- [x] Serialize mutations per target and check the expected document or entry
  revision inside the critical section.
- [x] Add recovery coverage proving interrupted temporary files cannot replace
  canonical memory.
- [x] Add malformed metadata recovery tests with the metadata envelope.
- [x] Keep L1 event ingestion and UI DTOs product-owned.

### Phase 2: Implement `LearnerMemoryKnowledgeSource`

- [x] Advertise Search, Read, and Revisioned capabilities.
- [x] Implement the fixed L1/L2/L3 item mapping and target-specific visibility
  matrix.
- [x] Implement bounded snippets, exact reads, filters, cursors, cancellation,
  and sanitized backend failures.
- [x] Filter expired records before search/read.
- [x] Return stale rather than silently reading latest.
- [x] Run runtime `verify_source_contract`.
- [x] Add cross-profile, cross-principal, path traversal, forged-ref, and
  expired-entry tests.

### Phase 3: Implement runtime mutation adapters

- [ ] Implement `LearnerMemoryWriteStore` over `FileMemoryBackend`.
- [ ] Declare immediate consistency and the exact read source ID.
- [ ] Implement idempotent preference insertion and exact-revision delete.
- [ ] Persist runtime provenance, idempotency, and expiry metadata.
- [ ] Wrap `SecureMemoryWritePolicy` with the trusted product mutation gate and
  an allowlist containing only `preference`.
- [ ] Use a process secret of at least 32 bytes from product secret management;
  never put it in settings responses, logs, prompts, or Session entries.
- [ ] Assemble `MemoryService` with the same read source and access control.
- [ ] Run runtime `verify_memory_store_contract`.
- [ ] Add retry, concurrent write/delete, secret guard, TTL, receipt-readback,
  cancellation, and undo tests.

### Phase 4: Compose Knowledge and Memory runtime boundaries

- [ ] Generalize course `KnowledgeRuntime` into one source-composable Agent
  runtime.
- [ ] Replace `CourseKnowledgeAuthorizer` with a source-aware product
  authorizer.
- [ ] Build one trusted access context from session, selected KB, Tutor
  permissions, Learner Memory profile, and authorization version.
- [ ] Attach `MemorySessionId` to ordinary and workflow requests.
- [ ] Install exactly one `KnowledgePlugin`.
- [ ] Install `MemoryPlugin` only for `InteractiveMutation`.
- [ ] Fail assembly when mutation is requested without the approval
  coordinator.
- [ ] Make CLI memory access read-only by default and require an explicit
  `TerminalApprover` for mutation.
- [ ] Consume the upstream source-aware citation solution while keeping strict
  course citations and citation-optional memory personalization.

### Phase 5: Cut over ordinary conversations

- [ ] Implement the run-bound web approval coordinator and memory-specific
  `BeforeToolCallHook`.
- [ ] Wire `approval_response` to the pending request rather than trace-only
  handling.
- [ ] Propagate stop, disconnect, timeout, and terminal-run cancellation.
- [ ] Replace memory-root/tool fields on `CapabilityRouter` with the assembled
  runtime boundaries and access profile.
- [ ] Replace prompt references to `read_memory/write_memory` with natural
  generic Knowledge/Memory behavior.
- [ ] Update mocks to call `knowledge_search/read`,
  `memory_write`, and `memory_forget`.
- [ ] Verify projected receipts persist but memory read bodies do not.
- [ ] Verify disabled and read-only runs have no mutation Tools.

### Phase 6: Cut over maintenance evidence and apply

- [ ] Pass target-specific trusted access through `WorkflowRunRequest`.
- [ ] Replace L1/L2 product evidence Tools with the generic runtime Knowledge
  source in Update, Check, and Dedupe.
- [ ] Preserve read-before-cite validation using exact refs read in the current
  workflow step.
- [ ] Keep structured findings/change sets and one repair pass product-owned.
- [ ] Route accepted changes through the upstream-endorsed shared backend
  transaction boundary.
- [ ] Prove partial selection, stale rejection, all-or-nothing apply, and undo.
- [ ] Remove the superseded product maintenance evidence Tools only after all
  target matrices have equivalent tests.

### Phase 7: Remove the legacy Agent protocol

- [ ] Delete `tutor-tools::ReadMemoryTool`.
- [ ] Delete `tutor-tools::WriteMemoryTool`.
- [ ] Delete model-provided `approved`, scope, raw section, and path schemas.
- [ ] Remove `CapabilityRouter.memory_root`,
  `learner_memory_tools`, and manual Tool mounting.
- [ ] Remove legacy Tool names from prompts, mocks, README, specifications, and
  `docs/runtime-tool-projections.json`.
- [ ] Confirm active source contains no `read_memory` or `write_memory`.
- [ ] Keep product UI/store methods only where they represent product commands,
  not Agent compatibility.

### Phase 8: Quality and release gate

- [ ] Record representative Chat personalization quality before/after.
- [ ] Measure P50/P95 memory search, read, write, forget, and maintenance-run
  latency.
- [ ] Measure prompt tokens and durable Session size.
- [ ] Inspect raw runtime Session JSONL and compaction for leaked memory bodies,
  approval grants, access attributes, or policy secrets.
- [ ] Run concurrent-user isolation and cross-run ref/grant replay tests.
- [ ] Run the full Rust, projection, frontend test, and production build gates.
- [ ] Update the runtime audit, framework feedback resolutions, QA evidence,
  README, specs, and issue #3.

## 6. Test Matrix

| Surface | Required evidence |
| --- | --- |
| Source contract | search/read/revision/cursor/cancel/error behavior |
| Store contract | identity, visibility, idempotency, authorization, cancellation |
| Access | disabled, read-only, interactive, Tutor denial, cross-principal denial |
| Approval | approve, deny, timeout, disconnect, stop, stale response, replay |
| Security | secret rejection, forged ref/source/path/filter/grant, log/session redaction |
| TTL | boundary TTL, expiry invisibility, cleanup, expired forget |
| Concurrency | same-target writes, write/delete race, maintenance-vs-Chat stale CAS |
| Chat | personalization read, explicit remember, forget, no-memory mode |
| Knowledge mix | course-only, memory-only, both, strict course citation, silent memory use |
| Maintenance | Update, Check, Dedupe, repair, partial accept, stale reject, atomic failure |
| Recovery | malformed legacy/metadata entry, interrupted atomic replace, undo |
| Session | projected receipts only; no full read body or trusted context |

Release commands:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
powershell -ExecutionPolicy Bypass -File scripts/check-tool-projections.ps1
npm --prefix web-ui test
npm --prefix web-ui run build
```

## 7. Commit Sequence

Keep dependency, storage, adapters, product cutover, and deletion independently
reviewable:

1. `docs(memory): define runtime B3 migration`
2. `chore(runtime): align with memory foundation`
3. `refactor(memory): isolate file backend`
4. `feat(memory): implement runtime knowledge source`
5. `feat(memory): implement runtime write store and policy`
6. `refactor(knowledge): compose course and learner sources`
7. `feat(memory): add interactive mutation approval`
8. `feat(chat): use runtime learner memory`
9. `feat(memory): migrate maintenance evidence`
10. `refactor(memory): remove legacy agent tools`
11. `test(memory): complete B3 acceptance gate`
12. `docs(memory): complete runtime B3 migration`

Do not combine the file-format change, runtime dependency pin, Chat cutover, and
legacy deletion in one commit.

## 8. Definition of Done

B3 is complete only when:

- runtime PR #82 and all consumed follow-up contracts are merged and green;
- all runtime crates use one immutable reviewed revision;
- one Agent Knowledge registry safely supports course Knowledge and Learner
  Memory without weakening course citations or forcing memory narration;
- missing trusted context fails closed;
- mutation always requires a live server-mediated user approval;
- the model cannot forge identity, scope, approval, provenance, idempotency,
  TTL, path, or authorization;
- runtime source and store contract suites pass for the file adapters;
- immediate receipts read back through the same source at the exact revision;
- retries do not duplicate entries and expired entries are not visible;
- forget rejects forged, unauthorized, missing, and stale refs;
- maintenance preserves partial acceptance, stale rejection, atomic apply, and
  undo through an upstream-endorsed transaction boundary;
- raw Session and compaction contain no full memory body or trusted secret;
- active source contains no legacy `read_memory` / `write_memory` protocol;
- Chat, maintenance, projection, workspace, Clippy, and frontend release gates
  pass;
- measurements and security evidence are recorded in a B3 QA document.

## 9. Explicit Non-Goals

- automatic Memory capture or inferred learner-profile writes;
- runtime Context Attachments or automatic recall;
- redesigning the Memory management UI;
- replacing the L1/L2/L3 product model;
- using Learner Memory as factual course evidence;
- migrating Tutor Memory;
- multi-tenant account infrastructure beyond enforcing the runtime principal
  boundary in the current local-user product;
- preserving old Tool names as compatibility aliases.
