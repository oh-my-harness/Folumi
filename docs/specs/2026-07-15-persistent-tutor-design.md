# Persistent Tutor Design

> Status: core identity, Soul, permissions, and private memory released in v0.3.1; workspace aggregation and handoff in progress | Date: 2026-07-15 | Last updated: 2026-07-16 | Product surface: 辅导机器人

Implementation plan:
`../plans/2026-07-15-persistent-tutor-implementation-plan.md`.

## 1. Product Decision

The Tutor surface shall represent persistent tutor entities, not another Chat
mode. A tutor owns a Markdown Soul, capability policy, resource permissions,
conversation collection, and private continuity memory. New conversations
may choose who the user wants to learn with; without a selection they use the
Temporary Assistant. A persistent tutor may then use
Chat, Research, Quiz, Notebook, Knowledge, Space, and Memory as
parts of one learning relationship.

The product model is:

```text
Tutor = who accompanies the learner
Capability = what the tutor is doing now
Model = which LLM executes the work
Resources = which user-owned material may be used
```

## 2. Memory Ownership

Persistent tutors do not each copy the complete learner profile. Memory is
split by ownership:

| Memory | Owner | Purpose | Sharing |
| --- | --- | --- | --- |
| L1 evidence | Product workspace | What happened in Chat, Quiz, Notebook, and Knowledge | Shared product evidence |
| Learner Memory | Learner | Profile, scope, preferences, strengths, weaknesses, and recent learning state | Readable by authorized tutors |
| Tutor configuration | User | Tutor identity, Markdown Soul, defaults, and permissions | Belongs to one tutor |
| Tutor Memory | Tutor relationship | Commitments, open loops, lesson plans, reflections, and next actions | Private to one tutor by default |

Learner Memory answers “what is known about the learner.” Tutor Memory answers
“what this tutor promised, where this learning relationship stopped, and what
the tutor should do next.”

Example:

```text
Learner Memory: The learner understands formulas more easily after an example.
Tutor Memory: Begin the next positional-encoding lesson with a two-dimensional example.
```

Tutor Memory must not become a hidden duplicate user profile. New observations
about the learner continue to enter L1 and the normal Learner Memory
consolidation path.

### 2.1 Runtime memory routing

Memory instructions are assembled from the tools actually mounted for the
current turn. A prompt must not name or recommend a memory tool that the
current tutor is not allowed to use.

- Learner facts, preferences, strengths, weaknesses, scope, and recent state
  belong to shared Learner Memory. Direct runtime `memory_write` remains limited to an
  explicit user request or clear approval; ordinary conversation and inferred
  traits remain L1 evidence for the normal consolidation workflow.
- Tutor promises, open loops, lesson plans, teaching reflections, strategies,
  and next actions belong to private Tutor Memory. Autonomous
  `remember_for_later` and `resolve_tutor_memory` are described only when those
  tools are mounted for that tutor.
- One item is written to one owner. The Agent must not duplicate the same item
  across Learner Memory and Tutor Memory.
- Research findings, external claims, report prose, Notebook content, quiz
  questions, and quiz answers remain product artifacts rather than memory
  records.
- Both memory types are read as silent internal context. The Agent applies
  supported memory naturally, asks for confirmation when it is uncertain, and
  does not claim unsupported recall.

## 3. Tutor Memory

Each tutor maintains one product-owned structured store:

```text
tutors/<tutor-id>/memory.json
```

Soul remains user-editable Markdown because it is a stable authored identity.
Tutor Memory uses typed records because provenance, lifecycle state, isolation,
expiry, and atomic updates must be enforced without parsing prose documents.

Supported entry kinds:

- `commitment`: something the tutor promised to do.
- `open_loop`: a question, exercise, or follow-up that remains unresolved.
- `lesson_plan`: the agreed or inferred next teaching sequence.
- `reflection`: evidence about whether a teaching approach worked.
- `strategy`: concrete behavior the tutor should use in future sessions.

Entries require stable IDs, timestamps, source references, lifecycle state, and
optional expiry. Completed commitments and resolved open loops are closed
rather than kept forever as active context.

Tutors may autonomously write low-risk operational memory when it directly
improves continuity. They may not store credentials, sensitive personal data,
external factual claims, or unsupported personality judgments. Tutor Memory is
visible, editable, removable, and resettable by the user.

## 4. Tutor Entity and Soul

A tutor profile contains at least:

```json
{
  "id": "transformer-tutor",
  "name": "Transformer 导师",
  "soul_markdown": "# 核心身份\n\n你是一位帮助学习者系统掌握 Transformer 架构的导师。\n\n# 教学风格\n\n- 先建立直觉，再介绍公式。",
  "default_model_config_id": "...",
  "default_capability": "chat",
  "allowed_capabilities": ["chat", "research", "quiz"],
  "learner_memory_access": true,
  "resource_permissions": {
    "knowledge_base_ids": [],
    "notebook": true,
    "space": true
  },
  "autonomous_memory": true
}
```

`soul_markdown` is the stable, user-owned definition of who the tutor is and
how it teaches. It may describe identity, teaching style, specialties,
principles, and boundaries. It must not become a task list, learner profile, or
copy of current conversation state.

The current learning goal, next lesson, commitments, and unresolved work belong
to Tutor Memory because they change over time. Model selection, capabilities,
resource access, and safety policy remain structured configuration and are
never inferred by parsing Soul Markdown. Soul cannot override enforced product
permissions or runtime safety instructions.

## 5. New Conversation Entry

The empty Chat state keeps the normal greeting and exposes a compact tutor
chooser beneath the composer. Temporary Assistant is the default when no tutor
is selected.

> 这次想和哪位导师交流？

The implemented chooser shows all available tutors, the selected tutor's Soul
summary, and a management action. Temporary Assistant is represented by no
selection rather than by a duplicate list item. Clicking the selected tutor
again clears the selection.

Selecting a tutor updates the pending conversation configuration. The runtime
session is created only when the user sends the first message, matching normal
deferred Chat session creation. Selection alone does not create, open, or
reorder a conversation.

Later continuity enhancements may add:

- each tutor's current goal, last progress, and open-loop count;
- recent-tutor ordering and one-click continuation of an existing conversation;
- open-loop and background-run indicators.

Temporary Assistant preserves today's lightweight Chat behavior. It may read
authorized Learner Memory but has no persistent Soul, private Tutor Memory, or
long-term tutor plan.

## 6. Session Binding and Handoff

Every persistent conversation stores a stable `tutor_id` beside its runtime
session mapping. The binding does not change in place because changing tutor
identity would replace Soul instructions and private memory inside an existing
runtime history.

To change tutors, the product creates a new conversation through a handoff:

1. choose the destination tutor;
2. prepare a bounded conversation summary;
3. let the user choose which artifacts and open loops to share;
4. create a new runtime session owned by the destination tutor.

Deleting or resetting a tutor must not delete global Learner Memory, Notebook,
Knowledge, Quiz, or Space assets. Session retention is an explicit user choice.

## 7. Context Assembly

For a tutor-bound turn, product code supplies thin, explicit context mappings
to the runtime:

```text
tutor Soul and permissions
  + relevant Learner Memory
  + relevant private Tutor Memory
  + runtime session history
  + current capability and selected resources
  -> AgentHarness / WorkflowEngine
```

The runtime continues to own sessions, context construction, tools, traces,
compaction, and workflow execution. `llm-tutor` owns tutor records, permission
mappings, memory files, UI, and runtime-session IDs.

Memory should be read on demand. Only small high-priority commitments and open
loops may be included at turn start; complete tutor files must not be injected
into every prompt.

## 8. Product Integration

- Chat is the tutor's primary interaction surface.
- Chat also owns complex explanations, derivations, and tool-assisted solving.
- Research is a detailed workflow the tutor can start after confirming scope.
- Quiz checks learning progress and feeds shared Learner Memory evidence.
- Notebook stores durable reports and learning material.
- Knowledge supplies grounded source documents.
- Space exposes quizzes, notes, profile, and other learning assets.
- Learner Memory is shared user context.
- Tutor Memory preserves relationship-specific plans and commitments.

The user may choose a tutor before the first message. Without a selection, the
conversation uses Temporary Assistant. The tutor chooses or proposes the
appropriate capability as the task develops.

## 9. UI Shape

The current Tutor page provides a compact tutor rail and a profile editor for
name, Markdown Soul, default model, default and allowed capabilities, resource
permissions, and memory policy. Soul supports edit and rendered-preview modes.
The server applies the resolved model at session creation and revalidates
resource permissions for session changes and every runtime turn.

User-created tutors expose a direct delete action on their row in the tutor
rail. The product presents this as deletion, while storage uses archive
semantics so existing tutor-bound sessions keep their identity and history.
Archived tutors are removed from active management and future selection. The
profile workspace does not duplicate the action, and the built-in Usage Guide
Tutor does not expose it because it cannot be deleted.

The stable built-in profile with ID `general-tutor` is the product Usage Guide
Tutor. It remains an ordinary tutor-bound Chat rather than a special tutorial
runtime mode. Its Soul explains the current product structure, names exact UI
labels and navigation paths, distinguishes attachments, associated sources,
and explicit `@` references, and asks what the user is trying to accomplish
before giving steps. It does not claim to click controls or inspect local data
that was not supplied to the conversation.

New stores seed the Usage Guide identity directly. An existing built-in profile
is migrated only when its name and Soul still match the former untouched
General Tutor defaults; user-customized built-in Souls are preserved.

The Tutor page also provides a continuity view for private typed memory. Users
can add, edit, resolve, reopen, delete, and reset entries without changing
shared Learner Memory. Each runtime tool is constructed with one immutable
Tutor ID; active entries are injected as a bounded turn-start summary and full
entries remain available through `read_tutor_memory`.

The target Tutor workspace extends that surface with:

- left rail: tutor list, selection, and run state;
- main area: the selected tutor's conversations;
- compact side area: current goal, next plan, commitments, and open loops;
- settings: Markdown Soul, model defaults, capabilities, resource permissions, and
  autonomous-memory policy;
- memory management: inspect, edit, close, delete, or reset private entries.

Conversation rows will display their tutor identity. Tutor selection remains
an optional, fixed-height control in the new-conversation empty state. Its
options render in a viewport-level, height-bounded overlay that scrolls
internally; the number of tutors must never increase the empty-state or
composer height. The overlay remains compact rather than inheriting the full
composer width, presents tutor identity and Soul summary with a clear selected
state, and adds search when the tutor collection becomes long.

## 10. MVP Scope

### Phase 1: Persistent Identity

Status: implemented in `v0.3.1`.

- Add tutor store and CRUD API.
- Add a built-in Usage Guide Tutor with stable ID `general-tutor`.
- Add tutor selection to the new-conversation screen.
- Persist immutable `tutor_id` on sessions.
- Apply tutor Soul, default model, and capability permissions.

### Phase 2: Private Continuity Memory

Status: implemented in `v0.3.1`; hard content-policy validation for autonomous
writes remains pending.

- Add the typed Tutor Memory store and scoped read/write tools.
- Support commitments, open loops, plans, reflections, and strategy.
- Show tutor memory in the Tutor workspace.
- Allow reset without changing Learner Memory.

### Phase 3: Resources and Handoff

Status: resource permissions are implemented; bounded handoff, recent-tutor
summaries, and Tutor-workspace run aggregation remain pending.

- Add per-tutor Notebook, Knowledge, Space, and Learner Memory permissions.
- Add bounded handoff into a new tutor-bound session.
- Add recent tutor, open-loop, and background-run indicators.

## 11. Acceptance Criteria

- A user can create or select a tutor before starting a conversation.
- A tutor-bound session restores the same Soul and private memory after restart.
- Two tutors can share Learner Memory while keeping commitments and plans
  private from one another.
- A tutor can continue an unresolved learning thread across sessions.
- Chat, Research, and Quiz remain capabilities rather than becoming
  duplicate tutor types.
- Temporary Assistant remains available for one-off conversations.
- Resetting a tutor does not destroy global learning assets or Learner Memory.
