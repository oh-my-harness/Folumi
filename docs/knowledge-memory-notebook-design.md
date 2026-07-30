# Knowledge, Memory, and Notebook Architecture

Status: Accepted

This document defines the ownership boundary between runtime capabilities and
`llm-tutor` product capabilities. It is the target architecture for the next
migration stages.

## Goals

- Use `llm-harness-runtime` for reusable Agent infrastructure and protocols.
- Keep human-readable learner memory documents as the durable source of truth.
- Give tutors durable private continuity memory without maintaining a parallel
  memory tool stack.
- Keep course RAG behind the runtime Knowledge contract.
- Let the Agent directly operate the user's Notebook through bounded,
  product-owned tools.
- Keep every cross-domain transfer explicit. Searching or reading one store
  must never silently copy data into another store.

## Ownership Decision

| Capability | Runtime contract | Product responsibility | Source of truth |
| --- | --- | --- | --- |
| Tutor Memory | Memory plus Knowledge | Tutor-specific schema, adapter, storage, and status transitions | Tutor Memory records |
| Learner Memory | Memory plus Knowledge | Markdown parsing, durable storage, indexing, and product UI | L1 event JSONL and L2/L3 Markdown |
| Course RAG | Knowledge | Ingestion, chunking, embeddings, LanceDB, and knowledge-base selection | Imported course documents |
| Notebook | Runtime Tool orchestration only | Notebook domain model, file operations, indexing, validation, and UI | Notebook files |

Runtime continues to own sessions, context construction, tool orchestration,
hooks, trace, cancellation, compaction, access control, Knowledge protocols,
and Memory protocols. `llm-tutor` owns product data and thin adapters to those
protocols.

## Tutor Memory

Tutor Memory contains continuity state owned by one tutor:

- commitments;
- open loops;
- lesson plans;
- teaching reflections;
- future teaching strategies.

It must not contain learner profile facts, credentials, copied Notebook
content, course facts, research results, or unsupported judgments.

### Required runtime integration

- Reading is exposed through a tutor-scoped runtime `KnowledgeSource`.
- Creation and deletion use runtime `MemoryService` and `MemoryPlugin` when
  their semantics match.
- The product store implements a thin runtime `MemoryStore` adapter.
- Runtime access context must bind every request to exactly one tutor.
- Search results are candidates only. The Agent must perform an exact,
  revisioned read before using memory content.
- Tutor-specific operations that runtime Memory does not model, such as
  resolving an open loop while retaining its history, remain thin product
  domain tools until runtime provides an appropriate update/status contract.
- While runtime exposes only one fixed-name `MemoryPlugin` per Agent, a
  tutor-scoped product tool may call a separate runtime `MemoryService`
  directly. It must not bypass runtime policy, mutation gate, provenance, or
  receipt validation.
- Any missing runtime capability is recorded in `docs/framework-feedback.md`.

### Retrieval

Tutor Memory retrieval must be hybrid:

1. structured constraints for tutor, status, kind, and due state;
2. semantic vector retrieval over text and next action;
3. lexical retrieval for exact terms and identifiers;
4. recency and status ranking;
5. exact revisioned `knowledge_read`.

The product record store remains authoritative. Search indexes are derived and
must be rebuildable.

## Learner Memory

Learner Memory is shared personalization context scoped to the user. L1 event
JSONL and L2/L3 Markdown remain the durable, human-readable source of truth.
Runtime does not replace these documents.

### Required runtime integration

- Read and discovery use the runtime Knowledge registry and tools.
- Explicit durable writes and forget operations use runtime Memory policy,
  mutation gate, service, plugin, and receipts.
- Maintenance workflows use runtime Workflow and Knowledge contracts.
- A product batch/CAS transaction may remain at the application boundary until
  runtime provides equivalent atomic batch semantics.
- Memory content is personalization context, not factual course evidence, and
  does not require learner-visible citations.

### Retrieval

Learner Memory retrieval must combine:

- deterministic structured recall for identity, requested name, scope, and
  explicit preference kinds;
- semantic vector recall for goals, strengths, weaknesses, preferences,
  teaching state, and summaries;
- lexical/BM25 recall for exact terms and identifiers;
- recency, confidence, and importance signals;
- exact revisioned reads before content is used.

Pure vector similarity is not sufficient for identity facts. Pure lexical
matching is not sufficient for free-text memory. Markdown remains
authoritative; vector and lexical indexes are derived.

Semantic Learner Memory retrieval uses the embedding configuration explicitly
selected in product settings. A Knowledge Base/session-specific embedding
configuration takes precedence; otherwise the active embedding configuration
is used. If no embedding configuration is selected or the provider is
unavailable, structured and lexical recall remain available. When a remote
embedding provider is selected, eligible memory text is sent to that configured
provider for embedding.

## Course RAG

Course RAG remains a runtime Knowledge source.

- `llm-tutor` owns document ingestion, chunking, embedding configuration, and
  LanceDB retrieval.
- Runtime owns source registration, access checks, search/read tools, exact
  references, evidence authority, and citation policy.
- Course claims require verified evidence according to the configured runtime
  citation policy.
- RAG data must never be silently promoted into Tutor Memory or Learner Memory.

## Notebook

The Notebook is a user-authored product workspace. It is not Memory and is not
required to use the runtime Knowledge abstraction.

The Agent directly operates it through bounded, product-owned tools executed by
the runtime tool loop. Direct control means domain-level access, not arbitrary
filesystem or shell access.

### Required capabilities

- list the Notebook tree;
- search paths, titles, tags, metadata, links, and Markdown content;
- read one Notebook item;
- create an item;
- update an item;
- rename an item;
- move an item;
- delete an item;
- inspect the result of a change;
- undo a completed change where practical.

Notebook search may use structured filters, BM25, and embeddings behind the
product `search_notebook` tool. Notebook files remain authoritative and every
index must be rebuildable.

### Mutation rules

- Listing, searching, and reading are immediately executable when Notebook
  access is enabled.
- An explicit user request authorizes the requested create, update, rename, or
  move operation within the associated Notebook.
- Self-initiated edits without an explicit user request remain proposals.
- Delete, destructive overwrite, and bulk mutation require a separate trusted
  confirmation.
- Updates use an expected revision or equivalent CAS check.
- Paths are normalized and confined to the associated Notebook root.
- Mutations are traced and return the final artifact reference and revision.
- The system should preserve an undo or recovery path for material changes.

### Domain separation

- Notebook writes never use `memory_write`.
- Reading Notebook content does not automatically create memory.
- The Agent may remember a Notebook-related user preference only through a
  separate, explicit Memory action.
- Notebook content used in an answer is identified as user-authored workspace
  content, not external course evidence.

## Agent Exploration

Runtime integration must preserve the useful behavior of document exploration
without granting arbitrary filesystem access.

An Agent may:

1. browse categories or the Notebook tree;
2. perform a broad or structured search;
3. read exact revisioned results;
4. follow related references or source links;
5. reformulate the query and continue searching.

Knowledge and Memory sources should expose enough metadata for discovery while
withholding authoritative content until exact read. A missing generic runtime
browse or source-filter capability should be handled by a thin product adapter
and reported upstream, not by bypassing runtime access controls.

## Acceptance Criteria

### Tutor Memory migration

- No Tutor Memory tool writes directly to the product store. A temporary
  scoped tool may be a thin `MemoryService` caller until runtime supports
  routing multiple memory stores without fixed-name tool conflicts.
- Tutor Memory reads use runtime Knowledge search and exact revisioned read.
- Tutor isolation, stale-reference rejection, and mutation policy have boundary
  tests.
- Resolve/status remains an explicit documented domain operation if runtime has
  no matching contract.

### Learner Memory retrieval

- Identity and requested-name queries deterministically recall profile entries.
- Semantically equivalent free-text queries recall preferences and learning
  state without requiring shared keywords.
- Exact terms and identifiers remain searchable.
- Index loss can be recovered by rebuilding from L1/L2/L3 files.
- Existing access, revision, and mutation-gate tests continue to pass.

### Course RAG

- Semantic retrieval remains behind runtime Knowledge search/read.
- Exact references and required citation verification continue to pass.
- No parallel course-search tool is introduced.

### Notebook direct control

- The Agent can list, search, read, create, update, rename, move, and delete
  through bounded Notebook tools.
- Explicit user-requested non-destructive writes can complete without a second
  model workflow.
- Stale revisions, root escape, destructive operations without confirmation,
  and cross-Notebook access fail closed.
- Every successful mutation returns a final reference/revision and has an
  audit/recovery path.

## Delivery Order

1. Record and accept this ownership boundary.
2. Migrate Tutor Memory reads and generic mutations to runtime adapters.
3. Add derived hybrid semantic retrieval to Learner Memory.
4. Expand Notebook proposal-only editing into bounded direct management.
5. Run end-to-end desktop acceptance covering all three stores and verify that
   no implicit cross-domain copying occurs.
