# Runtime Knowledge A6 Migration Plan

> Status: in progress (Phase 0-6 implemented; Phase 7 quality/release checks next) |
> Date: 2026-07-23 | Tracks:
> [llm-tutor issue #1](https://github.com/oh-my-harness/llm-tutor/issues/1) |
> Upstream baseline:
> [`llm-harness-runtime@83bef164`](https://github.com/oh-my-harness/llm-harness-runtime/commit/83bef164b36bd46ffa6f41cd6d3288a6b93cac4e)

## 1. Goal

Migrate course Knowledge Base access from the product-owned
`KnowledgeRetriever -> RagSearchTool` protocol to the runtime Knowledge
protocol delivered by Milestone A1-A5.

The migration must preserve the current product:

- LanceDB remains the vector store.
- Existing embedding configuration and document ingestion remain product
  responsibilities.
- Knowledge Base creation, deletion, reindexing, management APIs, and UI remain
  available.
- Chat, Research, and Quiz keep their current user-facing behavior.
- The existing `SourceReferences` UI remains the presentation and navigation
  layer for citations.

The migration changes the Agent-facing boundary:

```text
Chat / Research / Quiz run
  -> RunRequest + trusted KnowledgeAccessContext
  -> KnowledgePlugin
  -> knowledge_search / knowledge_read
  -> KnowledgeRegistry + KnowledgeAccessControl
  -> LanceDbKnowledgeSource
  -> tutor-rag / LanceDB
```

After migration, `llm-tutor` shall not maintain a second supported RAG Tool
protocol.

## 2. Confirmed Upstream Baseline

The `codex/session-projection` branch implements the runtime prerequisites:

| Milestone | Runtime commit | Capability consumed by this migration |
| --- | --- | --- |
| A1 | `c186d91` | Typed `RunRequest`, immutable `RunContext`, run-local state |
| A2 | `d318bef` | `ToolFailure`, typed result extensions, explicit `ToolSessionProjection` |
| A3 | `05b1d63` | `KnowledgeSource`, refs, registry, access context and authorization |
| A4 | `50d61d4` | Local source reference implementation and contract tests |
| A5 | `8f54607` | Knowledge tools, evidence receipts, citations and safe projections |
| Tracking | `8ab2a377` | Design status records A1-A5 complete and A6 pending |
| A6 gate | `bff00c1` | Runtime final-answer Knowledge citation validator and policy |
| A6 gate | `7aebdca` | Trusted workflow request extensions propagated to every LLM step |

The reviewed branch uses `llm-api-adapter` revision
`16a22ad284b8deb8c3a77664a0876f565f4a6eb9`.

Implementation shall pin every `llm-harness-*` crate to one immutable runtime
revision and align `llm_adapter` to the revision required by that runtime.
Mixing runtime revisions is not allowed.

The upstream `llm-harness-runtime-knowledge-local` crate is a reference
filesystem implementation. It shall inform contract behavior and tests but
shall not replace LanceDB in the product.

## 3. Current Product Baseline

The current implementation has the following migration surface:

- Root runtime dependencies are pinned to `83bef164`; `llm_adapter` is aligned
  to `16a22ad`.
- All 25 production Tools use `ToolFailure`, typed result data, and an explicit
  `Projected` or `Ephemeral` Session projection.
- Ordinary capability runs accept `RunRequest`; the legacy message/string
  methods construct one as a compatibility wrapper.
- `tutor-rag::LanceDbKnowledgeSource` implements the runtime source contract
  for one trusted, run-selected Knowledge Base.
- The active LanceDB table is `knowledge_chunks_v1`, with stable opaque item
  IDs, exact content revisions, document/chunk identity, display metadata, text,
  and vectors.
- Document reindex uses one LanceDB merge operation, and
  `POST /api/knowledge-bases/{kb}/reindex` rebuilds the new index from
  product-stored document text.
- Each Knowledge-enabled ordinary run now carries a fresh, typed
  `KnowledgeAccessContext` derived from its session, selected KB, and current
  Tutor permissions.
- A single process-held `EvidenceAuthority`, product authorizer, registry, and
  `KnowledgePlugin` are assembled without exposing the secret or trusted scope
  to prompts, Tool arguments, Session data, or frontend state.
- The Agent-facing `KnowledgeRetriever` / `RagSearchTool` protocol has been
  removed. Knowledge Base management keeps a direct product search API, while
  every Agent/Quiz evidence path uses runtime Knowledge contracts.
- `CapabilityRouter` carries only the assembled runtime Knowledge boundary.
- Chat and both Research paths install the runtime `KnowledgePlugin` and use
  `knowledge_search` / `knowledge_read`. Detailed Research passes trusted access
  through `WorkflowRunRequest` and validates run-local handles before accepting
  Knowledge-backed step output. Quiz source collection uses the same registry
  and evidence authority, then passes only verified bounded chunks into the
  generation workflow.
- The web layer derives a trusted run-scoped Knowledge context from the selected
  KB, session, and current Tutor resource permissions.

The A1/A2 API baseline, runtime-compatible LanceDB source, trusted access
assembly, Chat product wiring, and Research migration are complete. Both
upstream A6 gates are consumed from the unified `83bef164` baseline. Quiz
source collection and legacy Agent RAG cleanup are also complete.

## 4. Ownership Boundaries

### Runtime owns

- `RunRequest` and `RunContext`.
- `knowledge_search` and `knowledge_read` Tool schemas and execution flow.
- Common source discovery, search/read routing, and access-control boundary.
- Evidence receipt issuance and verification.
- Run-scoped citation handles.
- Tool result projection into durable Session history.
- Fail-closed behavior when `KnowledgeAccessContext` is absent.

### Product owns

- Knowledge Base and document records.
- Embedding provider configuration and vector generation.
- Chunking policy and LanceDB schema.
- The `LanceDbKnowledgeSource` adapter.
- Mapping the selected conversation KB and Tutor permissions into a trusted
  `KnowledgeAccessContext`.
- Product source metadata and `SourceReferences` navigation.
- User-facing error messages and controlled diagnostics.

### Product must not own

- A compatibility copy of `rag_search`.
- A second evidence receipt or citation trust store.
- Model-provided tenant, KB, backend filter, or authorization state.
- Full `knowledge_read` body persistence in product Session entries.

## 5. Fixed Design Decisions

### 5.1 One selected KB is one run-visible source

For the A6 slice, a run exposes at most one course Knowledge Base. The runtime
registry contains one `LanceDbKnowledgeSource` descriptor with stable source ID
`course_knowledge`; the source instance is bound by trusted server state to the
session's selected KB and embedding configuration.

This is deliberate:

- `KnowledgeRegistry::search` currently requires exactly one visible source.
- The model does not need to choose a KB.
- A forged `source_id` resolves to not-found or unauthorized.
- Multi-KB federation belongs to the later runtime router milestone, not A6.

No Knowledge Base selected means no course source is mounted. Direct boundary
tests must still prove that a mounted source without
`KnowledgeAccessContext` fails closed.

### 5.2 Trusted access context

The web service constructs `KnowledgeAccessContext` for every Knowledge-enabled
run:

- `scope.namespace`: `llm-tutor.course-knowledge`
- `scope.project`: current session ID
- `scope.attributes["knowledge_base_id"]`: selected KB ID
- `principal.subject`: stable local user/profile identity
- `principal.principal_type`: `local_user`
- `authorization_version`: a value derived from current Tutor/resource
  permissions

The context is attached through `RunRequest::with_extension`. It is never
serialized, logged generically, copied into prompts, or accepted from Tool
arguments.

The product authorizer and source both fail closed:

- source discovery/search/read require the selected KB to remain allowed;
- `read` verifies that the referenced row belongs to the bound KB;
- changed Tutor permissions invalidate new runs through
  `authorization_version`;
- backend predicates are built only from trusted product state.

### 5.3 Stable opaque refs and exact revisions

The LanceDB Knowledge schema shall provide:

- an opaque stable chunk `item_id`;
- the product KB and document IDs as internal columns;
- a stable chunk ordinal or selector ID;
- an exact content `revision`;
- source title/URI metadata;
- chunk text and vector.

The item ID shall be derived from stable product document identity and chunk
position using an opaque encoded digest or namespaced UUID. Authorization must
never rely on the opacity of the ID.

The revision shall include canonical chunk content, document identity, and the
chunking schema version. `knowledge_read` behavior is:

- exact item + revision exists: return that content;
- item exists but exact revision does not: return `StaleReference` and an
  optional latest ref;
- item does not exist or belongs to another KB: return safe not-found or
  unauthorized;
- never silently substitute the latest body for an exact stale ref.

The current index has no revision contract. A6 may perform a one-time rebuild
from product-stored document text. It shall not keep dual legacy/new readers.

### 5.4 Search/read split

`LanceDbKnowledgeSource::search` returns:

- `KnowledgeRef`,
- title,
- a bounded snippet,
- score,
- product URI,
- suggested chunk selector,
- allowlisted display metadata.

It does not return the complete chunk body.

`LanceDbKnowledgeSource::read` returns the selected full content up to the
runtime `max_read_bytes` limit. It honors cancellation and reports only
sanitized `KnowledgeError` values to the model.

### 5.5 Session projection policy

The A2 upgrade requires every product Tool to choose a projection explicitly.
The migration starts with this policy:

| Tool behavior | Projection |
| --- | --- |
| Search/list returning bounded public refs or snippets | `Projected` |
| Read/fetch returning private or potentially large source bodies | `Ephemeral` |
| Mutation/proposal/generation Tool | `Projected` receipt or artifact metadata |
| Code execution | `Projected` bounded result and execution metadata |
| `knowledge_search` | Runtime-owned `Projected` behavior |
| `knowledge_read` | Runtime-owned `Ephemeral` summary plus `knowledge.evidence` metadata |

`Full` is not the migration default. Any Tool that needs `Full` must document
why the complete model-visible result is safe and necessary in Session.

### 5.6 Citation trust and product display

The model cites only runtime handles returned by `knowledge_read`, such as
`[K:...:1]`. Product `SourceReferences` may convert a validated citation record
into the existing navigable `kb:<kb>:<document>:<chunk>` target.

Display conversion does not create trust. A Knowledge citation is accepted only
when its handle resolves through runtime `CitationValidator` in the same run.
Unknown, forged, stale, or cross-run handles are rejected.

Runtime `bff00c1` adds the final-answer citation validation hook and
`KnowledgeCitationPolicy`; the product consumes
`RequireWhenEvidenceRead`. Citation enforcement remains inside the runtime and
the product does not build a parallel run-state or receipt validator.

## 6. Migration Phases

### Phase 0: Runtime A1/A2 baseline

- [x] Pin all runtime crates to the reviewed A1-A5 revision, or a newer reviewed
  merge commit containing the same contracts.
- [x] Align `llm_adapter` and regenerate `Cargo.lock`.
- [x] Migrate every `Tool::execute` from `ToolError` to `ToolFailure`.
- [x] Migrate `ToolResult.content` to `model_content`, typed extensions, and an
  explicit Session projection.
- [x] Add a projection audit and checked static inventory so a new Tool cannot
  silently inherit unsafe persistence:
  `scripts/check-tool-projections.ps1` and
  `docs/runtime-tool-projections.json`.
- [x] Replace removed runtime workflow APIs:
  - `human_approval` wrapper with the current `BeforeToolCallHook` boundary;
  - `SYNC_SPAWN_TOOL_NAME` / `sync_spawn_agent` with current spawn behavior;
  - `submit_step_result` prompts/mocks with structured LLM step output;
  - old workflow result fields and test `ToolContext` construction.
- [x] Prove `RunRequest` extensions reach ordinary Chat Tool calls.
- [x] Prove how extensions and Knowledge plugins reach each Research/Quiz
  workflow LLM step. Record a runtime issue before adding a product workaround
  if the workflow boundary cannot carry them.
- [x] Resolve the runtime-owned final-answer citation validation boundary.

Resolved upstream gates:

- Runtime `7aebdca` adds `WorkflowRunRequest` and propagates typed extensions
  to every workflow LLM step attempt.
- Runtime `bff00c1` lets `KnowledgePlugin` install a final-answer citation
  validator with strict evidence-read policy.

Both resolutions and product integration evidence are recorded in
`docs/framework-feedback.md`.

Checkpoint:

```powershell
cargo check -p tutor-tools
cargo check -p tutor-agent
cargo check -p tutor-web
cargo test -p tutor-agent --test mock_integration
```

No Knowledge behavior changes ship in this phase.

### Phase 1: LanceDB Knowledge source

- [x] Add `llm-harness-runtime-knowledge` to the workspace and `tutor-rag`.
- [x] Introduce `LanceDbKnowledgeSource` bound to one trusted KB.
- [x] Define the versioned LanceDB Knowledge schema.
- [x] Generate stable opaque item IDs and exact revisions.
- [x] Implement lightweight vector search as `KnowledgeSource::search`.
- [x] Implement exact revision reads as `KnowledgeSource::read`.
- [x] Honor max bytes, cancellation, and supported selectors.
- [x] Keep backend diagnostics behind sanitized `KnowledgeError` output.
- [x] Add a one-time index rebuild path from stored document text.
- [x] Update delete and reindex paths to maintain the new schema atomically.

Implementation notes:

- `knowledge_chunks_v1` intentionally replaces, rather than dual-reads, the
  legacy random-ID table.
- Stable item IDs include the chunk schema version, KB ID, document ID, and
  chunk ordinal. Revisions additionally include canonical chunk content.
- Search snippets are capped at 600 UTF-8 bytes; reads enforce `max_bytes`.
- A document reindex is a merge keyed by `item_id`, including conditional
  removal of old chunks from the same trusted KB/document source.
- Public errors remain sanitized; backend detail is available only through the
  controlled `KnowledgeError::diagnostic` boundary.

Contract tests:

- [x] search returns refs/snippets without full bodies;
- [x] exact read succeeds;
- [x] stale revision returns `StaleReference`;
- [x] cross-KB item read fails;
- [x] malformed refs and selectors fail safely;
- [x] cancellation stops search/read;
- [x] LanceDB paths, raw filters, and embedding errors are not exposed through
  public error display.

### Phase 2: Access and runtime assembly

- [x] Add the product `KnowledgeAuthorizer`.
- [x] Build `KnowledgeRegistry`, process-held `EvidenceAuthority`, provider ID,
  and `KnowledgePlugin` through one assembly helper.
- [x] Generate the evidence secret in trusted process state; never expose it to
  the model, Session, logs, or frontend.
- [x] Use a typed `RunRequest::with_extension(access)` through
  `run_request_with_session_cancel` for Knowledge-enabled ordinary runs.
- [x] Derive access only from the session record, selected KB, bound Tutor, and
  current resource permissions.
- [x] Ensure background/rejoined runs keep their immutable original run access;
  a new user turn receives a newly evaluated context.

Implementation note: the assembled plugin is held by `CapabilityRouter`.
Phase 3 installs it only for Chat and removes `rag_search` from the Chat tool
set atomically, so a Chat model never sees two supported KB protocols.

Security tests:

- [x] absent context fails closed;
- [x] no selected KB assembles no course runtime;
- [x] forged source ID fails;
- [x] forged item ID/revision fails;
- [x] changing model Tool arguments cannot change the selected KB;
- [x] Tutor permission removal prevents the next run;
- [x] sessions bound to different KBs cannot cross-read.

### Phase 3: Chat migration

- [x] Register `KnowledgePlugin` in the Chat harness when a KB is selected.
- [x] Remove `rag_search` from Chat tools and system prompts.
- [x] Teach the Agent to search first, read selected refs, and cite only returned
  handles.
- [x] Bridge runtime-issued Knowledge citation records to product
  `SourceReferences`.
- [x] Preserve normal streaming and final assistant message behavior.
- [x] Verify Session replay contains safe search/evidence projections but no
  `knowledge_read` body.

Implementation note: the product path displays runtime-issued read evidence
without duplicating receipt validation or persisting read bodies. The
`KnowledgePlugin` now uses `RequireWhenEvidenceRead`, so runtime rejects a final
answer that omits a citation after reading trusted evidence and rejects handles
that were not issued in the current run.

Acceptance:

- a grounded Chat answer uses `knowledge_search` then `knowledge_read`;
- a non-RAG answer does not invent Knowledge citations;
- switching sessions during generation does not lose the final answer or source
  metadata.

### Phase 4: Research migration

- [x] Register the same Knowledge assembly in the outer Research Chat agent.
- [x] Pass trusted access and Knowledge tools into detailed Research workflow
  LLM steps that use course material.
- [x] Remove `rag_search` from Research workflow Tool scopes and prompts.
- [x] Keep web search/web fetch evidence distinct from course Knowledge
  evidence.
- [x] Validate course Knowledge handles before publishing the report.
- [x] Keep saved reports and source lists compatible with Notebook and
  `SourceReferences`.

Implementation note: detailed Research calls
`WorkflowEngine::run_with_request`, mounts Knowledge as step plugins, and
declares only `knowledge_search` or `knowledge_read` on the steps that need
them. Because citation handles are scoped to one step run, `write_report`
re-reads selected Knowledge refs and cites the fresh handles issued in that
final step. Web sources continue to use numbered report references.

Acceptance:

- ordinary Research conversation remains normal streaming Chat;
- detailed Research reads course evidence only after the workflow is explicitly
  started;
- report citations distinguish KB and web sources;
- workflow Session entries do not contain full Knowledge bodies.

### Phase 5: Quiz migration

- [x] Replace Quiz's direct `KnowledgeRetriever` source path with the runtime
  Knowledge source/registry boundary.
- [x] Keep deterministic product source collection where it is part of the Quiz
  workflow, but issue and validate evidence through runtime contracts.
- [x] Pass only verified, bounded source bodies into generation and verifier
  steps.
- [x] Map validated Knowledge refs into `QuizCitation` metadata.
- [x] Preserve conversation, Notebook, and no-KB Quiz source paths.
- [x] Remove stale `submit_step_result` mocks/prompts as part of the Phase 0
  runtime workflow migration.

Acceptance:

- a KB-backed Quiz cannot cite an unread or cross-KB chunk;
- stored question citations navigate to the correct KB document/chunk;
- Quiz restore and answer review behavior remain unchanged.

### Phase 6: Remove the legacy Agent RAG boundary

- [x] Delete `tutor-tools::RagSearchTool`.
- [x] Delete `tutor-rag::KnowledgeRetriever`.
- [x] Remove `CapabilityRouter.retriever` and `associated_kb`.
- [x] Remove legacy `rag_search` prompt text, Tool event mapping, mocks, and
  citation extraction.
- [x] Keep direct product search APIs only where they support Knowledge Base
  management UI; they must call product storage/search services, not emulate an
  Agent Tool.
- [x] Confirm no active source contains `rag_search` or model-provided `kb`
  authorization.

### Phase 7: Quality, security, and release gate

- [ ] Capture representative retrieval quality samples before and after.
- [ ] Measure P50/P95 search and read latency.
- [ ] Measure answer token use and durable Session size.
- [ ] Verify no full read bodies appear in JSONL Session files.
- [ ] Verify citation handle forgery and cross-run reuse fail.
- [ ] Verify controlled diagnostics contain enough backend detail while public
  failures remain sanitized.
- [x] Update README, manual, runtime audit, framework feedback, product
  requirements, and desktop QA checklist.
- [ ] Update issue #1 checklist and link the landed commits.

Release commands:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
npm --prefix web-ui test
npm --prefix web-ui run build
```

## 7. Commit Sequence

Keep each checkpoint independently reviewable:

1. `chore(runtime): align with knowledge milestone baseline`
2. `refactor(tools): adopt explicit session projections`
3. `refactor(workflow): migrate current runtime step contracts`
4. `feat(rag): implement LanceDB knowledge source`
5. `feat(agent): inject trusted knowledge access`
6. `feat(chat): use runtime knowledge tools`
7. `feat(research): use runtime knowledge evidence`
8. `feat(quiz): use runtime knowledge evidence`
9. `refactor(rag): remove legacy agent retrieval protocol`
10. `test(knowledge): cover access evidence and session safety`
11. `docs: complete runtime knowledge A6 migration`

Do not combine the dependency/API baseline, storage schema change, and legacy
deletion into one commit.

## 8. Definition of Done

A6 is complete only when all of the following are true:

- Chat, Research, and Quiz have tests using runtime Knowledge contracts.
- All runtime crates use one reviewed revision.
- Every product Tool has an explicit reviewed Session projection.
- The model cannot select or broaden KB authorization.
- Search returns lightweight refs and read returns exact revisioned content.
- Full read content is absent from durable Session replay.
- Every trusted KB citation belongs to verified evidence read in the same run.
- Existing Knowledge Base management and citation navigation still work.
- `KnowledgeRetriever` and `RagSearchTool` no longer exist in active code.
- Full workspace tests, Clippy, and frontend checks pass.

## 9. Explicit Non-Goals

- Runtime Memory Milestone B.
- Automatic memory or Knowledge capture.
- Multi-source Knowledge routing and automatic recall.
- Replacing LanceDB or embedding providers.
- Redesigning Knowledge Base management UI.
- Preserving old random chunk IDs as a permanent compatibility protocol.
- Persisting full retrieved bodies to improve replay.
