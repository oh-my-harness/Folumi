# Folumi User Memory Redesign

> Status: Proposed — awaiting product review before implementation
>
> Decision date: 2026-08-03
>
> Supersedes: the active L1/L2/L3 memory model and Memory consolidation
> workflow described by earlier design and implementation-plan documents

## Context

Folumi now presents Memory as a standalone, user-controlled workspace. The
product shows individual long-term memories and lets the user inspect, edit, or
forget them. The previous backend instead modeled Memory as hidden activity
logs (L1), generated summaries (L2), and profile files (L3), plus maintenance
runs that consolidated one layer into another.

That model no longer matches the product. It records activity the user did not
ask to remember, makes file placement part of the domain model, and leaves a
large invisible maintenance system behind a simple item-oriented interface.

## Proposal

If accepted, User Memory will be a flat collection of explicit, durable memory items. A memory
item is a product entity, not a Markdown bullet, file location, event, summary,
or consolidation result.

The old layered system is retired completely:

- no L1 activity events are recorded from Assistant, Notebook, or Knowledge
  Base operations;
- no L2 summaries or L3 profile files are read or written;
- no consolidation preview, run, apply, undo, or file-editing API remains;
- old layered files are not migrated or imported into the new system; and
- old files may remain inert on disk, but the application does not discover,
  read, mutate, or delete them automatically.

## Product model

Each memory item has:

| Field | Purpose |
| --- | --- |
| `id` | Stable opaque item identity |
| `kind` | User-facing semantic category |
| `content` | The concise fact or continuity statement |
| `source_refs` | Optional links back to the conversation or product object that justified the write |
| `provenance` | Bounded machine-readable origin metadata for audit and explanation |
| `idempotency_key` | Prevents duplicate writes when a runtime call is retried |
| `created_at` / `updated_at` | Lifecycle timestamps |
| `expires_at` | Optional expiry for time-bounded information |
| computed `revision` | Exact compare-and-swap token for edits and deletion |

The supported kinds are:

- `profile`: stable facts about who the user is or how they wish to be
  addressed;
- `preference`: stable communication, learning, or workflow preferences;
- `goal`: a durable outcome the user is working toward;
- `commitment`: a promise the assistant should carry into a later session;
- `open_loop`: an unfinished item that should be resumed;
- `strategy`: a reusable approach that has been explicitly chosen.

Kinds organize and filter the list; they do not determine physical storage
locations. Assistant Profile remains a separate tab backed by product settings
and must not be confused with a `profile` memory about the user.

## Write and recall policy

Ordinary conversation stays in the runtime session. Merely mentioning a fact,
opening a note, importing a source, or running a search never creates long-term
memory.

The assistant may propose a memory only when the user explicitly asks it to
remember something or explicitly confirms a proposal. Every assistant write
and forget operation continues through `llm-harness-runtime-memory`, including
access control, secure write policy, idempotency, exact revisions, and the live
mutation confirmation gate. UI edits and deletes also require the latest item
revision.

Recall uses the runtime Knowledge boundary. Search may establish relevance but
must not expose the full private content in a result snippet; the assistant
must perform an exact revisioned read. Expired items are hidden from runtime
recall but remain visible to the user until edited or forgotten.

The master switch controls whether new sessions mount Memory read/write
capabilities. Turning it off does not erase existing items.

## Proposed storage boundary

The initial local implementation would store one versioned document at
`memory/items.json`. The repository exposes a `MemoryItemStore` boundary so the
physical backend can later move to SQLite without changing product APIs or
runtime contracts. The expected working set is a small collection of curated
items; full activity history belongs to runtime sessions, not this store.

Storage must provide:

- atomic replacement of the versioned document;
- serialized mutation within the process;
- idempotent upsert;
- exact-revision update and delete;
- stable opaque IDs; and
- fail-closed validation for kinds, content length, provenance, and schema
  version.

## Proposed user interface and API

The Long-term Memory tab presents a single item list. It may group or filter by
kind, but it must not expose L1/L2/L3, file paths, Markdown markers, or
consolidation concepts.

The active API is limited to:

- `GET /api/memory/items` — list items with computed revisions;
- `PATCH /api/memory/items` — edit one item using `id` and `revision`; and
- `DELETE /api/memory/items` — forget one item using `id` and `revision`.

Creating memory through conversation remains a runtime `memory_write`
operation with live confirmation. A future explicit “Add memory” UI may call a
dedicated endpoint, but must apply the same validation and must not reintroduce
automatic extraction or consolidation.

## Non-goals

- behavioral telemetry or a complete audit log;
- automatic profiling from user activity;
- summarizing every conversation, note, or source into Memory;
- exposing physical storage structure as navigation;
- multiple hidden memory tiers; or
- compatibility with the retired layered files and maintenance APIs.

## Acceptance criteria

- Active production code contains no L1/L2/L3 event recording or consolidation
  workflow.
- Notebook and Knowledge Base changes never write Memory.
- The Memory page and API use stable item IDs and revisions only.
- Runtime search/read/write/forget works against the same flat item store.
- Writes and forgets requested by the assistant require live confirmation.
- Turning Memory off prevents mounting it in new sessions without deleting
  items.
- Existing layered files do not affect new-system results and are never
  silently deleted.
- Product requirements, user documentation, and regression tests describe the
  flat explicit-memory model.

## Change control

Reintroducing automatic capture, hidden layers, consolidation, or migration
from the retired store requires a new explicit product decision and matching
PRD, documentation, and regression-test changes.
