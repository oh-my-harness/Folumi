# Folumi Product Requirements Specification

> Status: active target baseline | Last updated: 2026-08-03

This specification is authoritative for the current product. Historical Tutor,
Quiz, Space, Student Profile, and standalone Research requirements remain in
archived plans only and are not Folumi requirements.

The accepted workspace boundaries and change-control rule are recorded in
[`2026-08-03-primary-workspaces-decision.md`](./2026-08-03-primary-workspaces-decision.md).

## 1. Product Definition

- **REQ-001** Folumi shall be a local-first personal knowledge assistant.
- **REQ-002** The primary navigation shall contain Assistant, Knowledge Base,
  Notebook, Memory, and Settings.
- **REQ-003** The product shall expose one Assistant identity rather than a
  user-managed collection of Tutors.
- **REQ-004** Sources, Notes, Memory, and Sessions shall remain distinct data
  domains with explicit ownership and mutation rules.
- **REQ-005** Quiz, Space, Student Profile, multi-Tutor management, and
  standalone Research modes shall not exist in active UI, APIs, capability
  routing, persistence, or recovery paths.

## 2. Assistant and Sessions

- **REQ-100** Users shall conduct ordinary multi-turn conversations through a
  runtime-backed session.
- **REQ-101** Sessions shall persist messages, verified citations, tool traces,
  durable task results, and the selected knowledge scope.
- **REQ-102** Switching sessions or reloading the application shall not lose a
  running task or duplicate completed output.
- **REQ-103** The composer shall allow explicit selection of a knowledge base
  and exact Note references.
- **REQ-104** Referenced Notes shall be persisted by stable identity and read
  on demand with `read_notebook_item`; their full text shall not be blindly
  injected into every prompt.
- **REQ-105** Research shall be an explicit task action inside Assistant and
  may run a controlled workflow that returns a cited report to the originating
  session.
- **REQ-106** Code execution, web search, web reading, knowledge tools, and Note
  tools shall be enabled by server-side policy, not merely by prompt wording.

## 3. Sources

- **REQ-199** Knowledge Base shall be a dedicated RAG source workspace and
  shall not embed editable Notes or Memory management in the same interface.
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
- **REQ-301** Notes shall support folders, tags, Wiki links, backlinks, import,
  export, search, and direct editing.
- **REQ-302** Note mutation tools shall enforce Vault root confinement.
- **REQ-303** Updates shall use revision checks, and destructive actions shall
  require confirmation with a recovery path.
- **REQ-304** Source and Note results may appear in one search response but
  shall preserve their data type, provenance, and authority boundary.

## 5. Memory and Continuity

- **REQ-399** Memory shall be a standalone primary workspace for configuring
  the single Assistant profile and for enabling, reviewing, editing, and
  forgetting long-term memory.
- **REQ-400** Long-term Memory shall be optional and controlled by a master
  switch.
- **REQ-401** Users shall be able to inspect, edit, approve, reject, and forget
  individual memory items with visible provenance.
- **REQ-402** User Memory shall hold stable preferences, explicit user facts,
  and long-term goals.
- **REQ-403** Assistant Continuity shall hold commitments, open loops, and
  reusable working strategies in the unified memory backend.
- **REQ-404** Session text shall not become long-term memory solely because it
  appeared in conversation.
- **REQ-405** Recall and mutation shall pass runtime access control and the
  mutation gate; prompt instructions shall not grant authority.
- **REQ-406** A product embedding Folumi components may disable built-in Memory
  or provide its own knowledge sources and policies without adopting Folumi's
  organization.
- **REQ-407** Assistant name and behavior instructions shall be managed from
  Memory rather than duplicated in Settings; they shall not override runtime
  safety, tool approval, or data-access policy.

## 6. Settings, Privacy, and Portability

- **REQ-600** Users shall configure LLM, embedding, search, appearance, Note
  Vault, and tool governance through Settings; Assistant profile, Memory policy,
  and Memory items shall be managed from the standalone Memory workspace.
- **REQ-601** Product data shall remain local unless a configured provider or
  explicit external tool call requires transmission.
- **REQ-602** The UI shall make current knowledge scope and long-term Memory
  behavior understandable to the user.
- **REQ-603** Sources, Notes, settings, sessions, and Memory shall have explicit
  preservation or export behavior.

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
