# Folumi Product Requirements Specification

> Status: active target baseline | Last updated: 2026-08-07

This specification is authoritative for the current product. Historical Tutor,
Quiz, Space, Student Profile, and standalone Research requirements remain in
archived plans only and are not Folumi requirements.

The accepted workspace boundaries and change-control rule are recorded in
[`2026-08-03-primary-workspaces-decision.md`](./2026-08-03-primary-workspaces-decision.md).
The removal of the dedicated Research product path is recorded in
[`2026-08-05-remove-research-mode-decision.md`](./2026-08-05-remove-research-mode-decision.md).
The retained Notebook workspace capabilities are recorded in
[`2026-08-05-notebook-workspace-capabilities-decision.md`](./2026-08-05-notebook-workspace-capabilities-decision.md).

## 1. Product Definition

- **REQ-001** Folumi shall be a local-first personal knowledge assistant.
- **REQ-002** The primary navigation shall contain Assistant, Sources,
  Notebook, Personalization, and Settings. Chinese labels shall be 助手、资料、
  笔记、个性化、设置. Knowledge Base and Memory remain internal domain names.
- **REQ-003** The product shall expose one Assistant identity rather than a
  user-managed collection of Tutors.
- **REQ-004** Sources, Notes, Memory, and Sessions shall remain distinct data
  domains with explicit ownership and mutation rules.
- **REQ-005** Quiz, Space, Student Profile, multi-Tutor management, and any
  dedicated Research mode, action, workflow, or report-recovery path shall not
  exist in active UI, APIs, capability routing, persistence, or recovery paths.

## 2. Assistant and Sessions

- **REQ-100** Users shall conduct ordinary multi-turn conversations through a
  runtime-backed session.
- **REQ-101** Sessions shall persist messages, verified citations, tool traces,
  durable task results, and the selected knowledge scope.
- **REQ-102** Switching sessions or reloading the application shall not lose a
  running task or duplicate completed output.
- **REQ-103** The composer shall allow explicit selection of a Source
  collection and exact Note references.
- **REQ-104** Referenced Notes shall be persisted by stable identity and read
  on demand with `read_notebook_item`; their full text shall not be blindly
  injected into every prompt.
- **REQ-105** Ordinary Chat shall handle research-oriented requests through
  the same conversation flow, using web, Knowledge Base, Notebook, and citation
  tools when appropriate; it shall not require a separate Research capability
  or report workflow.
- **REQ-106** Code execution, web search, web reading, knowledge tools, and Note
  tools shall be enabled by server-side policy, not merely by prompt wording.
- **REQ-107** Ordinary conversation completion shall clear transient working
  indicators without appending a generic `Done` status or exposing internal
  context-message counts. Completion notices remain appropriate for meaningful
  artifacts, user-requested stops, failures, and other actionable outcomes.

## 3. Sources

- **REQ-199** Sources shall be a dedicated RAG source workspace and shall not
  embed editable Notes or Personalization management in the same interface.
  Individual Knowledge Base containers shall be labeled Source collections.
- **REQ-200** Sources shall preserve original documents and source metadata as
  authoritative data.
- **REQ-201** The product shall expose import progress, failure reasons, retry,
  duplicate detection, and index rebuild.
- **REQ-202** Vector indexes and chunks shall be rebuildable derivatives of
  preserved Sources.
- **REQ-203** Users shall be able to search, preview, and open the original
  location of a Source.
- **REQ-204** Knowledge claims and citations shall derive from runtime Evidence
  produced by authorized source reads.
- **REQ-205** Source documents shall be read-only to Assistant mutation tools.

## 4. Notes

- **REQ-299** Notebook shall be a standalone primary workspace for recording,
  organizing, reading, and editing Notes.
- **REQ-300** Notes shall be user-owned Markdown stored in the application
  Vault or a user-selected external Vault.
- **REQ-301** Notes shall support folders, tags, Wiki links, backlinks, search,
  and direct editing inside user-selected local Markdown folders.
- **REQ-302** Note mutation tools shall enforce Vault root confinement.
- **REQ-303** Updates shall use revision checks, and destructive actions shall
  require confirmation with a recovery path.
- **REQ-304** Source and Note results may appear in one search response but
  shall preserve their data type, provenance, and authority boundary.
- **REQ-305** The standalone Notebook UI shall retain an editor-style workspace
  with a hierarchical and virtualized file tree, folder-local creation,
  desktop context actions, Markdown editing and preview, persisted expansion
  state, and visible Vault refresh/watch status. It shall not regress to a flat
  CRUD list as a consequence of removing Space or other retired products.
- **REQ-306** The Notebook relation surface shall expose tags, outgoing Wiki
  links, unresolved-link creation, backlinks, and a collapsible local graph;
  users shall be able to navigate resolved links without leaving Notebook.

## 5. Memory and Continuity

> Transition status (2026-08-03): the former L1/L2/L3 activity-capture and
> consolidation system is retired. REQ-400 through REQ-416 describe the
> replacement product target. Global Saved Memory, its lifecycle rules,
> UI controls, and runtime mutation gate are active. The default-off History
> Recall baseline now uses the runtime cross-Session contract. Runtime Sessions
> own conversation-local continuity and are not Saved Memory scopes.

- **REQ-399** Personalization shall be a standalone primary workspace for
  configuring the single Assistant and for enabling, reviewing, editing, and
  forgetting durable personal context. These responsibilities shall be
  separated into Assistant setup and About me tabs, with About me selected by
  default. Memory remains the internal domain name.
- **REQ-400** Personal information shall be optional and controlled by a master
  switch labeled Use personal information.
- **REQ-401** About me shall distinguish explicit Personal information
  (internal Saved Memory) from opt-in Reference past conversations (internal
  History Recall); past-conversation reference shall be independently
  controllable and disabled by default. The first release shall expose History Recall as visible,
  agent-decided Knowledge tool calls when prior conversation context is clearly
  requested or referenced, and shall not inject hidden recall before every run.
- **REQ-402** Users shall be able to inspect, add, edit, pin, complete,
  reconfirm, and forget individual Saved Memory items with visible provenance
  and revision.
- **REQ-403** Saved Memory shall use the user-facing kinds fact, preference,
  goal, and continuity without creating hidden storage layers.
- **REQ-404** Saved Memory may be created when the user explicitly asks to
  remember something, adds an item in the Personalization UI, or directly
  states clearly durable and personally useful information such as a preferred name, stable
  preference, goal, or continuity item and the Assistant proposes a bounded
  write. The Assistant shall not infer unstated facts or proactively save
  transient, third-party, secret, or sensitive details. Notebook and Knowledge
  Base activity shall not create Saved Memory merely because it occurred.
- **REQ-405** Saved Memory shall be global. Runtime Sessions shall own
  conversation-local continuity and may be referenced as provenance, but shall
  not be duplicated as a Saved Memory scope. Knowledge Base IDs, Notebook
  paths, and navigation pages shall not define Memory partitions.
- **REQ-406** Items about the same topic shall follow explicit
  conflict and lifecycle rules: equivalent content refreshes confirmation,
  contradictions atomically supersede the old item after confirmation, and
  resolved, superseded, expired, or forgotten items do not participate in
  normal recall.
- **REQ-407** Assistant name and behavior instructions shall be managed from
  Personalization rather than duplicated in Settings; they shall not override runtime
  safety, tool approval, or data-access policy.
- **REQ-408** Notebook and Knowledge Base operations shall never be captured as
  long-term Memory merely because they occurred.
- **REQ-409** The retired layered files and consolidation APIs shall not be
  read, migrated, or regenerated by the replacement system.
- **REQ-410** History Recall shall treat runtime Sessions as the sole authority,
  use only a rebuildable derived index, return exact Session/turn sources, and
  never automatically promote a retrieved conversation into Saved Memory.
- **REQ-411** A temporary conversation shall not read or write Saved Memory,
  shall not search prior Sessions, and shall not enter the future History Recall
  index.
- **REQ-412** Deleting a Session shall invalidate its History Recall index;
  forgetting a Saved Memory item shall remove its body, sources, full-text
  index content, and recoverable bodies from item history.
- **REQ-413** Recall and mutation shall pass runtime access control, context
  construction, and mutation gates; prompt instructions shall not grant
  authority or implement a parallel history path.
- **REQ-414** Memory and conversation history are personalization and
  continuity inputs, not factual Knowledge evidence, and shall not weaken
  source citation requirements.
- **REQ-415** A product embedding Folumi components may disable built-in Memory
  or provide its own knowledge sources and policies without adopting Folumi's
  organization.
- **REQ-416** Assistant-initiated Saved Memory writes shall require per-item
  approval by default. The Personalization UI shall expose a separate, default-off
  permission allowing only assistant `memory_write` operations to skip that
  approval. Forgetting, conflict resolution, and destructive changes shall not
  inherit the permission. Disabling Memory shall revoke the active permission.
- **REQ-417** The Assistant shall perform routine tool and Memory operations
  without narrating implementation steps such as “I will check Memory” or “I
  will search History,” and shall answer in a natural voice consistent with the
  Assistant Profile. It may explain process when work is materially long,
  consent is required, an operation fails or leaves important uncertainty, or
  the user asks for the process or source. Product tool traces remain visible.
- **REQ-418** Long-term conversation quality shall be evaluated in separate
  retrieval and end-to-end answer layers. External LoCoMo results shall be
  reported by category with dataset revision and model/runtime provenance, and
  shall supplement rather than replace Folumi lifecycle, permission, privacy,
  Chinese-language, Saved Memory, and interaction-style regression tests.
- **REQ-419** Each LLM configuration shall expose a runtime-backed thinking
  level. Provider-returned displayable reasoning shall stream separately from
  the final answer, render as secondary collapsible text on that answer, and
  remain recoverable from the runtime Session without entering recall or RAG.

## 6. Settings, Privacy, and Portability

- **REQ-600** Users shall configure LLM, embedding, search, appearance, Note
  Vault through Settings; Assistant setup, Memory policy,
  and Personal information shall be managed from the standalone
  Personalization workspace.
- **REQ-601** Product data shall remain local unless a configured provider or
  explicit external tool call requires transmission.
- **REQ-602** The UI shall use intent-oriented labels—Sources for external
  evidence, Notebook for user-authored Markdown, and Personalization for how
  the Assistant knows the user and continues conversations—rather than expose
  Knowledge Base or Memory as primary navigation labels.
- **REQ-603** Sources, Notes, settings, sessions, and Memory shall have explicit
  preservation or export behavior.
- **REQ-604** Settings shall not expose a standalone Permissions & Data page.
  Runtime safety checks remain enforced internally, while the desktop app owns
  its local data directory without requiring a dedicated settings shortcut.

## 7. Runtime Boundary

- **REQ-700** Folumi shall use `llm-harness-runtime` for sessions, context,
  tools, hooks, trace, compaction, Knowledge, and Memory behavior rather than
  implementing a parallel runtime.
- **REQ-701** Runtime contract gaps shall be recorded in the runtime repository
  with a durable issue and shall not be filled by product compatibility glue.
- **REQ-702** Ephemeral ContextAttachment support shall be adopted only after a
  first-class runtime contract exists; ephemeral body content shall not be
  restored as durable session knowledge.

## 8. Acceptance Gates

- **REQ-800** Rust workspace tests shall pass on the supported stable toolchain.
- **REQ-801** Frontend tests and production build shall pass.
- **REQ-802** The desktop package shall build and pass install-and-launch smoke
  QA before a release is published.
- **REQ-803** Active code search shall find no Quiz workflow, multi-Tutor store,
  Tutor private-memory runtime, Space route, retired capability branch, or
  legacy migration/import/export route.
- **REQ-805** File-watcher tests shall wait for both indexed content and exposed
  watcher status instead of relying on fixed timing assumptions.
