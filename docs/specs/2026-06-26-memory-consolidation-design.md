# Memory Consolidation Design

> Superseded on 2026-08-03. The described L1/L2/L3 workflow has been removed
> from the active product and must not be used as an implementation target.
> See `2026-08-03-user-memory-redesign.md` for the replacement proposal.

> Status: core workflow implemented; artifact resolution and budgets remain | Created: 2026-06-26 | Updated: 2026-07-14 |
> Scope: define the memory consolidation workflow, evidence access boundary,
> structured change contract, and review experience for `llm-tutor`.

## 1. Goal

The memory system should turn product activity into durable, inspectable learner
memory without letting the model freely rewrite hidden profile state.

Target flow:

```text
Product event / workspace record
  -> append-only L1 event plus durable source pointer
  -> user starts a Memory maintenance run
  -> agent receives target schema and compact evidence catalog
  -> agent explores target-scoped L1/L2 through runtime Knowledge search/read
  -> agent returns an evidence-bound MemoryChangeSet
  -> product validates refs, anchors, and base revision
  -> user reviews and selects changes in the central diff
  -> product applies accepted operations atomically
  -> Markdown memory documents are serialized with history and undo
  -> agents read L3 memory through runtime Knowledge when useful
```

The key lesson from DeepTutor is that consolidation prompts should not ask the
model to write final Markdown. They should ask the model to extract or edit
small facts against a normalized input contract. The application owns Markdown
formatting, entry ids, reference validation, deduplication, and file writes.

## 2. Design Principles

- Keep raw product data and final memory documents separate.
- Give the LLM a uniform, compact, evidence-rich input shape.
- Require JSON-only output from the LLM.
- Validate every source reference before writing memory.
- Use Markdown as the durable user-editable memory surface.
- Treat memory as personalization evidence, not factual source evidence.
- Allow agents to write only explicit user preferences by default.
- Route profile, scope, recent, and teaching-strategy updates through a
  user-visible memory workbench.
- Make all L1 evidence addressable to the Memory agent without injecting the
  complete ledger into every prompt.
- Keep L1 access read-only and make every evidence read visible in the run flow.
- Return structured changes, not a complete Markdown draft.
- Require user review before any agent-produced change is written.
- Capture the global UI language when a run starts and use it for newly
  generated user-facing Memory content without translating existing entries.

## 3. Layers

### L1: Product Event Ledger

L1 is not the primary UI. It is the raw, append-only or derived event layer used
as consolidation evidence.

Recommended sources:

- chat messages and assistant answers,
- quiz generation events,
- quiz answers and scores,
- notebook entries,
- knowledge-base interactions,
- research reports,
- future task/workflow outputs.

Recommended event shape:

```ts
MemoryEvent {
  id: string
  ts: string
  surface: 'chat' | 'quiz' | 'notebook' | 'knowledge'
  kind: string
  title?: string
  content: string
  metadata?: Record<string, unknown>
  sessionId?: string
  turnId?: string
}
```

The important part is that each event has a stable `surface:id` reference that
can be cited from L2 memory.

### L2: Surface Memory

L2 summarizes one product surface at a time. These files describe durable facts
from a specific activity stream.

Recommended files:

```text
memory/L2/chat.md
memory/L2/quiz.md
memory/L2/notebook.md
memory/L2/knowledge.md
```

Research does not own an L1 category or an L2 document. Ordinary Research-mode
clarification and planning conversation is recorded as Chat L1 with
`capability = research`. After `create_research_report` starts, search, fetch,
source-selection, progress, the structured report attachment, and the unsaved
report body remain in Research runtime/session state and are not
learner-memory evidence. Saving a report to Notebook is the explicit
persistence boundary; the resulting Notebook event may inform `notebook.md`.
Report bodies and external facts stay in Notebook reports rather than being
duplicated into Memory.

### L3: Cross-Surface Learner Memory

L3 synthesizes across L2 files. It is the primary agent-readable memory layer.

Recommended files:

```text
memory/L3/recent.md
memory/L3/profile.md
memory/L3/scope.md
memory/L3/preferences.md
memory/L3/teaching_strategy.md
```

Only `preferences.md` should be directly writable from chat, and only when the
user explicitly states a long-term preference or approves a fact. Other L3 files
should be updated through the memory workbench.

### 3.1 L3 Generation Contract

The normal durable generation path is strictly layered:

```text
L1 product events -> L2 surface memory -> L3 learner memory
```

L3 must not behave like another all-surface L2 run. Its initial evidence
catalog contains L2 documents and stable L2 entries, not the complete L1 event
catalog. The agent lists, searches, and reads concrete L2 entries on demand.
When an L2 claim needs verification, the agent may follow that entry's source
chain to L1. L1 is a provenance drill-down for L3, not its default discovery
surface.

L3 source matrix:

| Target | Primary sources | Additional rule |
| --- | --- | --- |
| `profile.md` | Chat, Quiz, Notebook, and Knowledge L2 | Synthesize durable identity, learning style, strengths, and weaknesses. |
| `scope.md` | Quiz, Chat, Notebook, and Knowledge L2 | Distinguish familiar, practicing, and unsure concepts. |
| `preferences.md` | Chat and Notebook L2 | Explicit preference L1 may be opened only to verify wording or intent. |
| `recent.md` | Bounded recent L1 plus current L2 context | This is the documented exception because L2 does not preserve an exact cross-surface timeline. |
| `teaching_strategy.md` | Accepted `profile.md`, `scope.md`, and `preferences.md`, plus relevant L2 entries | It is generated after those L3 documents and must not become their input. |

Every L2 bullet keeps its stable `<!--m_xxx-->` marker. L3 citations use a
canonical entry-level reference such as:

```text
memory:L2/chat.md#m_01ABC
```

Bare surface refs such as `chat` or `quiz` are not durable provenance. Direct
L1 refs are not accepted as the stored citation for ordinary L3 changes; a
verified L1 event remains reachable through the cited L2 entry. The
`recent.md` exception may store direct event refs when exact chronology is the
claim being represented.

L3 dependencies form a directed graph:

```text
L2 surfaces -> profile / scope / preferences / recent
profile + scope + preferences + relevant L2 -> teaching_strategy
```

L3 never writes L2, and peer L3 documents do not recursively feed each other.
Before an L3 run, the product checks the relevant L2 consolidation cursor or
last-updated timestamp. A stale L2 dependency is shown to the user; a future
explicit "Update all" orchestration may run and review L2 prerequisites before
starting L3, but an ordinary L3 run must not silently rewrite multiple layers.

## 4. Normalized Consolidation Input

DeepTutor's strongest idea is that consolidation consumes normalized evidence
instead of arbitrary app-specific payloads. In the target design, the complete
evidence set is not assembled before the run. The initial context contains the
job, target, base revision, and a compact evidence catalog. L1 tools then return
bounded evidence using the same normalized shape.

Each normalized evidence packet should contain:

- job metadata,
- existing target memory,
- allowed sections,
- focus instructions,
- a chunk-local reference pool,
- source chunks in a stable format.

Target input envelope:

```ts
ConsolidationInput {
  job: {
    mode: 'update' | 'audit' | 'dedup'
    layer: 'L2' | 'L3'
    key: string
    language: 'zh' | 'en'
    today: string
  }
  target: {
    title: string
    baseRevision: string
    existingMarkdown: string
    allowedSections: string[]
    focus: string
  }
  chunk: {
    index: number
    total: number
    start?: number
    end?: number
    citeableRefs: string[]
    text: string
  }
}
```

An LLM-facing evidence chunk returned after a list/search/read request should
be rendered like this:

```md
# Chunk-local citeable refs
- chat:session_123
- chat:session_456

@entity chat:session_123
title: 2 的 pi 次方计算方法
ts: 2026-06-26T10:30:00Z
content:
User asked how to calculate 2^pi. Assistant answered with log/exponential
calculation and numeric result.

@entity chat:session_456
title: 光刻模型与光刻胶模型
ts: 2026-06-26T11:12:00Z
content:
User confused lithography model with photoresist model during a Q&A turn.
```

This format gives the model enough information while making reference checking
straightforward.

## 5. Section Catalog

Each surface and slot should have a fixed focus and section list. The model may
only emit sections from the list.

Recommended L2 catalog:

```yaml
surfaces:
  chat:
    focus: Stable misconceptions, demonstrated mastery, and recurring topics.
    sections: [Misconceptions, Mastery, Topics]
  quiz:
    focus: Error patterns, strong topics, weak topics, and question types.
    sections: [Error patterns, Strong topics, Weak topics]
  notebook:
    focus: Durable behavior across notes and saved research reports, including organization habits, recurring themes, preferred note/report formats, and unresolved questions.
    sections: [Themes, Organization, Formats, Report preferences, Open questions]
  knowledge:
    focus: Document interests, frequent queries, and knowledge gaps.
    sections: [Interests, Frequent queries, Gaps]
```

Recommended L3 catalog:

```yaml
slots:
  recent:
    focus: Rolling timeline of recent learning activity.
    sections: [This week, Earlier]
  profile:
    focus: Durable learner identity, learning style, strengths, and weaknesses.
    sections: [Identity, Learning style, Strengths, Weaknesses]
  scope:
    focus: Concepts the learner has engaged with and confidence labels.
    sections: [Familiar, Practicing, Unsure]
  teaching_strategy:
    focus: How the tutor should adapt examples, difficulty, hints, and reviews.
    sections: [Explanation style, Practice strategy, Review strategy]
  preferences:
    focus: Explicit user-stated long-term preferences.
    sections: [Preferences]
```

Chinese UI can localize section labels, but internal tests are easier if the
stored section keys are stable. A display-name map can translate them.

### 5.1 Output Language Contract

Each run captures `zh-CN` or `en-US` from the global interface settings at
creation time. That value remains fixed until the run completes. The workflow
uses it for `summary`, finding messages, change reasons, and inserted or
replacement memory text in both L2 and L3. Existing Markdown is not rewritten
only to change its language. Stable schema values and section keys remain
unchanged, while code, API names, model names, paper titles, and other proper
nouns may remain in their original language when translation would reduce
precision.

## 6. Update Prompt Contract

Update mode discovers evidence through the tools appropriate to the target
layer and proposes small, evidence-bound changes. L2 uses L1 evidence tools;
L3 uses L2 entry tools and optional provenance drill-down. The model does not
receive an automatically assembled full ledger and does not write a complete
document.

### L2 Update Prompt

The prompt establishes the target surface, allowed sections, current document,
base revision, and tool-use rules. Its output is a `MemoryChangeSet` fragment:

```json
{
  "findings": [],
  "changes": [
    {
      "id": "change_01",
      "op": "insert",
      "text": "<one concise fact, <= 240 chars>",
      "section": "<one of: {sections}>",
      "refs": ["<surface>:<event_id>", "..."],
      "reason": "<why this is durable and useful>"
    }
  ]
}
```

Rules:

- Start with the target surface and use list/search before reading detailed
  evidence.
- Expand to another surface only when it can materially validate the change.
- Prefer durable misconceptions, demonstrated mastery, recurring topics, and
  stable learning needs over one-off chatter.
- Every insert or replacement must cite event-level evidence read in this run.
- Do not duplicate facts already represented by a stable memory entry.
- Use only allowed sections and cautious language.
- Return no change when the evidence is weak or transient.

### L3 Update Prompt

L3 update normally begins with stable L2 entries and follows their provenance
to L1 when a claim needs verification. L3 changes cite stable L2 entry ids whose
source chain resolves to L1; direct L1 event refs may also be included when the
agent reads them. Bare surface names are not sufficient provenance in the
target design.

Learner-level claims must remain hedged, avoid generalizing from a single weak
signal, and preserve the distinction between observed behavior and inferred
teaching strategy.

## 7. Audit Prompt Contract

Audit mode checks current entries against their source chains. It returns
findings and optional anchored changes, never a rewritten document:

```json
{
  "findings": [
    {
      "entryId": "m_abc",
      "severity": "warning",
      "kind": "unsupported | stale | contradictory | overgeneralized",
      "message": "<compact explanation>",
      "refs": ["chat:event_123"]
    }
  ],
  "changes": [
    {
      "id": "change_01",
      "op": "replace",
      "entryId": "m_abc",
      "text": "<corrected fact, <= 240 chars>",
      "refs": ["chat:event_123"],
      "reason": "<short reason>"
    }
  ]
}
```

Audit must resolve existing refs before judging a claim. It may search for newer
contradicting evidence, but any expansion is visible in flow. Changes target
stable entry ids; line-number edits are accepted only as a compatibility input
for documents that have not yet received stable markers.

## 8. Dedup Prompt Contract

Dedup mode proposes only replacements and deletions against stable entry ids:

```json
{
  "findings": [],
  "changes": [
    {
      "id": "change_01",
      "op": "replace",
      "entryId": "m_abc",
      "text": "<merged fact, <= 240 chars>",
      "refs": ["<existing-or-unioned-ref>", "..."],
      "reason": "<short reason>"
    },
    {
      "id": "change_02",
      "op": "delete",
      "entryId": "m_def",
      "refs": [],
      "reason": "duplicate of m_abc"
    }
  ]
}
```

Dedup must not add unrelated facts. It preserves or unions valid refs when
merging, keeps the stronger entry, and returns no changes when entries carry
distinct useful meaning.

## 9. Markdown Output Contract

The LLM never writes this directly. Product code serializes accepted facts and
edits into Markdown.

Target format:

```md
# chat memory

## Misconceptions

- User often confuses lithography model and photoresist model. [^1] <!--m_01ABC-->

## Mastery

- User correctly explains OPC and SMO as different computational lithography techniques. [^2] <!--m_01DEF-->

---

[^1]: chat:session_123
[^2]: quiz:session_456:q_2
```

Required invariants:

- Every bullet has a stable entry id marker: `<!--m_xxx-->`.
- Every cited source appears in the footnote block.
- Footnote labels are generated by the serializer, not the LLM.
- Repeated refs should share one footnote label.
- Parser and serializer should round-trip idempotently.
- Deleting one entry should remove unused footnotes on the next serialize.

## 10. Source Reference Rendering

Markdown footnotes are the durable storage format, not the final interaction
model. The UI should parse memory footnotes and render them as a product-level
source reference system.

Display behavior:

- Inline footnote references such as `[^1]` should render as compact clickable
  source chips, for example `[1]`.
- Clicking an inline source chip should scroll to the matching item in the
  source reference list at the bottom of the rendered content.
- The bottom reference list should show human-readable labels such as
  `Chat`, `Notebook`, `Quiz`, `Research report`, or `Knowledge Base`.
- Clicking a bottom reference item should navigate to the corresponding product
  surface and, when possible, focus the exact source record.
- Stable entry markers such as `<!--m_xxx-->` are internal metadata and must
  never be displayed in rendered Markdown.
- The renderer should not enable arbitrary raw HTML just to hide markers. It
  should either skip HTML safely or remove only the internal marker pattern
  before rendering.

Reference routing rules:

```text
chat:<session_id>[:message_id]              -> Chat session, optional message focus
notebook:<entry_id>                         -> Space / Notebook entry
quiz:<quiz_id>[:question_id]                -> Space / Quiz Bank item, optional question focus
kb:<knowledge_base_id>:<doc_id>[:chunk_id]  -> Knowledge Base document/chunk view
```

Saved Research reports use the ordinary `notebook:<entry_id>` route. The
retired `research:` and `book:` reference forms are removed rather than
retained as compatibility routes.

The same source reference component should be reused by Memory, Student Profile,
Research reports, Quiz review, and RAG answer citations where practical.

## 11. Validation Rules

Before writing memory:

- Parse LLM output as JSON.
- Reject non-JSON prose.
- Validate the `MemoryChangeSet` target path and base revision.
- Reject unsupported operations or unresolved entry/section anchors.
- Reject insert/replace changes with empty or overlong `text`.
- Reject sections not in the allowed section list.
- Reject refs that were not returned by an evidence tool in the current run.
- Reject L2 insert/replace changes with no event-level refs.
- Require L3 refs to resolve through stable L2 entries or directly read L1
  events; reject bare surface names as final provenance.
- Re-resolve source refs before apply to detect stale or removed evidence.
- Apply only user-accepted operations through one deterministic
  parser/serializer path.
- Reject the entire apply when validation or base-revision checks fail; do not
  leave a partially modified document.

## 12. Workbench Behavior

The Memory workbench exposes three actions for L2 and L3:

- Update memory: discover evidence and propose evidence-bound additions or
  corrections.
- Check memory: audit current facts against their sources and return findings
  with optional changes.
- Remove duplicates: propose merges, replacements, or deletions without adding
  unrelated facts.

### 12.1 Workbench Output

The compact workbench on the right is a run controller and flow monitor. It
must not render a complete proposed document or use a long model report as the
primary result.

The normal run view shows:

- selected action and model,
- current flow step and overall status,
- evidence categories searched,
- evidence items read,
- candidate findings and change counts,
- validation status,
- errors, cancellation, and retry state,
- runtime cost or token usage when available,
- a final compact summary such as `4 changes ready for review`.

The flow state vocabulary is:

```text
queued
  -> discovering_sources
  -> reading_evidence
  -> analyzing_memory
  -> proposing_changes
  -> validating_changes
  -> awaiting_review
  -> applying
  -> completed | failed | cancelled
```

Chunk progress and tool reads should map into these product-level states. Raw
prompt/input/output diagnostics may remain available in an explicit developer
trace, but are not part of the normal workbench result.

### 12.2 Structured Change Set

The Memory agent must not return `proposed_markdown`. It returns a structured
change set against an explicit document revision:

```ts
MemoryChangeSet {
  runId: string
  targetPath: string
  baseRevision: string
  summary: string
  findings: MemoryFinding[]
  changes: MemoryChange[]
}

MemoryFinding {
  id: string
  entryId?: string
  severity: 'info' | 'warning' | 'error'
  kind: string
  message: string
  refs: string[]
}

MemoryChange {
  id: string
  op: 'insert' | 'replace' | 'delete'
  section?: string
  entryId?: string
  afterEntryId?: string
  text?: string
  refs: string[]
  reason: string
}
```

Stable entry ids and section anchors are preferred over raw line numbers. The
product validates target revision, operation type, anchors, allowed sections,
text limits, and source refs, then deterministically renders the proposed
result and diff.

Operation requirements are explicit: insert requires `section` and `text`;
replace requires `entryId` and `text`; delete requires `entryId` and omits
`text`. `reason` is always required. Evidence refs are required unless product
validation can prove the operation is a purely structural removal of an empty
or malformed entry.

### 12.3 Central Diff Review

The center document area remains the primary surface and provides three modes:

- Read: rendered Markdown.
- Edit: direct user editing.
- Review: an inline diff for the pending change set, with an optional split
  view when space permits.

Review mode must:

- distinguish inserted, removed, and replaced content,
- group each operation as an independently reviewable change,
- show the agent reason and source chips beside each change,
- resolve a source chip back to its L1 event and original product artifact,
- allow accept/reject per change and accept/reject all,
- preview the deterministic document resulting from accepted changes,
- detect a stale `baseRevision` before apply,
- apply accepted changes atomically,
- create history and support undo after apply.

No persistent memory document changes before explicit user confirmation.

### 12.4 Layout Responsibility

- The left rail selects L2/L3 memory files and stays compact.
- The center area owns reading, editing, findings, and diff review.
- The right workbench owns controls and run progress only.
- L1 remains out of the primary file rail; it is reached through evidence
  exploration and source navigation.

### 12.5 Current Review Boundary

The active UI uses flow progress plus central structured diff review. The
workflow returns a `MemoryChangeSet`; it never returns final Markdown for
direct persistence.

## 13. Agent Tool Boundaries

### 13.1 Runtime Knowledge Evidence

The Memory workbench agent has read-only, on-demand access to all L1 evidence.
This means all evidence is addressable; it does not mean all evidence is
inserted into the initial prompt.

The workflow LLM step mounts the generic runtime tools:

```ts
knowledge_search({ query, source_id, filters, cursor, limit })
knowledge_read({ reference, selector })
```

The initial workflow context contains the target document, schema, base
revision, task rules, and a compact L1 catalog. The agent starts with the target
surface selected by the server. A target-scoped `WorkflowRunRequest` fixes the
allowed layers, surfaces, kinds, principal, and authorization version; model
filters can only narrow that scope.

The runtime Knowledge boundary must:

- paginate and enforce bounded result sizes,
- return stable event-level ids rather than session-only ids,
- preserve source surface, timestamp, session/turn identity, and artifact refs,
- return exact revisioned refs and bounded read bodies,
- emit trace/progress events for every search and read,
- remain read-only and expose no arbitrary filesystem access,
- support cancellation and workflow budgets.

Every proposed fact or edit must cite evidence actually read during that run.
The product rejects unknown, unread, or stale refs. L1 itself remains append-only
from the Memory workflow's perspective.

### 13.2 Target-specific L2/L1 Access

Each Memory workflow run mounts the same single runtime Knowledge plugin while
its trusted request changes the accessible source slice. L2 targets receive
only their L1 surface; ordinary L3 targets receive only their allowed L2
surfaces; `recent.md` receives L2 plus the bounded L1 chronology exception.

Listing and searching return candidates but do not make them citeable. Reading
one exact item adds the `canonical_reference` metadata value to the run's read
set. The product rejects output refs that were not observed through a
successful runtime read in that workflow run.

The L3 workflow stages are:

```text
checking_l2_freshness
  -> discovering_l2_sources
  -> reading_l2_evidence
  -> verifying_l1_evidence (optional)
  -> analyzing
  -> proposing_changes
  -> validating
  -> awaiting_review
  -> applying
  -> completed
```

L3 validation requires at least one read L2 entry for every ordinary insert or
replacement, rejects catalog-only and bare-surface refs, resolves entry ids and
revisions again before apply, and rejects circular dependencies. `recent.md`
uses a separately declared chronology rule for bounded direct L1 refs.

Memory entry length is layer-aware. L2 entries remain compact atomic facts;
L3 entries may contain a longer coherent synthesis, but must not become an
essay or contain multiple unrelated claims. An oversized model change enters
the same bounded repair pass as unread evidence and must be shortened or split
into multiple evidence-bound changes. The second invalid result fails with the
change id, actual character count, target layer limit, and remediation hint.

L1- and L2-backed runs perform a post-generation read-evidence audit before a
change set is accepted. If a draft cites an item that was only listed, searched,
or inferred but not read, the workflow gets one bounded repair pass containing
the unread references. The agent must read and re-evaluate each retained source
or remove the unsupported claim. A second invalid result fails validation; list
results never become citeable merely because they appeared in model context.

### 13.3 Runtime Agent Memory Protocol

`knowledge_search` / `knowledge_read`:

- Available to Chat, Research, and Quiz planning.
- Should be called when personalized teaching, review, quiz targeting, or
  long-running context matters.
- Should not be called for pure factual questions that do not need learner
  personalization.
- Search returns bounded candidates; only an exact read makes a reference
  eligible for a maintenance change set.

`memory_write` / `memory_forget`:

- Write only explicit preferences and require a live product approval.
- The product policy maps the runtime kind to `L3/preferences.md`; the model
  cannot choose a path.
- Forget requires an exact current runtime reference and the same live
  approval boundary.
- Should not update `profile.md`, `scope.md`, `recent.md`, or
  `teaching_strategy.md` during ordinary chat.

Memory content should guide explanation style and planning. It should not be
presented as factual proof about external domains.

## 14. Implementation Notes for `llm-tutor`

Current `llm-tutor` has Markdown memory files, event-level L1 references,
target-scoped runtime Knowledge evidence, a structured `MemoryChangeSet`, central diff
review, atomic selected-change apply, history, undo, and in-process run rejoin
after workspace navigation. As of 2026-07-14, L3 runs also use canonical L2
entry references, runtime Knowledge search/read, target-specific L2 source
matrices, read tracking, and the explicit `recent.md` L1 exception. Ordinary
L3 runs receive no L1 access and their changes must cite an L2 entry read in
the current workflow step. The remaining hardening work should:

- Preserve durable pointers to complete source artifacts instead of relying on
  truncated event summaries alone.
- Add explicit runtime budgets to Memory runs.
- Add retry/rebase controls for failed or stale runs.
- Persist run envelopes so an in-progress Memory run can rejoin after a full
  application restart.
- Add L2 freshness metadata and source-revision validation at apply time.
- Complete the Teaching Strategy dependency DAG through accepted Profile,
  Scope, and Preferences entries.
- Keep L3 updates hedged and source-attributed.
- Extend end-to-end tests to cover original-artifact resolution, cancellation,
  restart rejoin, and cross-surface expansion in a live model run.
- The retired draft-oriented consolidation/assist APIs and their unused
  prompt-injected L3 chunk path were removed on 2026-07-14. The structured
  Memory Run change-set workflow is now the sole Memory generation contract.

## 15. Why This Shape Is Better

### Natural Agent Interaction

Learner memory is part of the Agent's continuous product context, not a
database operation that should normally be narrated to the user. Runtime
Knowledge calls remain visible in trace but are silent in assistant answer text.
When memory is supported, the Agent should apply it directly or refer to it
naturally as something remembered from prior learning interactions. Weak,
stale, ambiguous, or conflicting memory must be hedged and confirmed. An empty
tool result must never be presented as a genuine recollection. If the user asks
how the Agent knows, it should explain the relevant prior interaction or memory
category honestly without hiding provenance.

This design gives us:

- inspectable learner memory,
- controllable model output,
- fewer hallucinated profile claims,
- source-linked memory entries,
- user-editable Markdown,
- room to add automatic consolidation later,
- a clean boundary between product data and runtime agent behavior.

The core rule is simple: the LLM proposes small evidence-bound operations; the
application owns the memory document.
