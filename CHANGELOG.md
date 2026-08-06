# Changelog

All notable changes to this project will be documented in this file.

This project follows Semantic Versioning for source releases. Desktop bundle
versions must use numeric `MAJOR.MINOR.PATCH` values because Windows MSI does
not accept SemVer pre-release identifiers. Mark alpha/beta builds with Git tags,
release titles, or artifact names such as `v0.1.0-alpha.1`.

## Unreleased

## 0.4.1 - 2026-08-06

### Added

- Added standalone Notebook and Memory workspaces. Notebook now provides a
  folder tree, Markdown editing, Wiki links, tags, backlinks, relationship
  navigation, Vault integration, contextual creation, and recoverable folder
  deletion.
- Added the item-oriented Saved Memory system with explicit provenance,
  revision history, conflict replacement, expiration, user editing and
  forgetting, assistant-write permissions, and JSON export.
- Added runtime-backed History Recall with persistent indexing, federated
  search, exact source reads, temporary memory-isolated conversations, and
  visible navigation back to recalled sessions.
- Added a one-click Chinese LoCoMo benchmark console with retrieval and
  end-to-end answer evaluation, versioned JSON results, and comparison charts.
- Added LanceDB-backed runtime Knowledge integration, verified source reads,
  duplicate detection, retryable imports, and full-index rebuilding.

### Changed

- Reframed Folumi as a local-first personal knowledge assistant with five
  primary workspaces: Assistant, Knowledge Base, Notebook, Memory, and
  Settings. Knowledge Base now contains RAG sources only.
- Consolidated identity into one Assistant Profile managed from Memory, with a
  visible default Folumi identity and natural, non-narrated memory tool use.
- Replaced the legacy layered Memory pipeline with runtime knowledge and memory
  primitives, while keeping product adapters thin and explicit.
- Simplified ordinary Chat to cover research-oriented questions through its
  existing knowledge, web, citation, and code tools.
- Reworked onboarding, empty states, source controls, and product documentation
  around the streamlined Folumi information architecture.

### Removed

- Removed Quiz, Space, Student Profile, multi-Tutor, standalone Research,
  billing controls, and the legacy data migration UI and APIs from the active
  product path.
- Removed the obsolete L1/L2/L3 Memory files, consolidation workflows, and
  parallel product-owned recall pipeline.

### Fixed

- Hardened Saved Memory and History Recall routing, source selection, retries,
  exact reads, session indexing, and durable recall behavior.
- Restored the IDE-like Notebook workspace and polished file-tree selection,
  creation dialogs, root context menus, deletion confirmation, and undo.
- Ensured the desktop backend sidecar exits with its parent process and kept
  heavy Windows Rust build steps serialized.

## 0.3.4 - 2026-07-17

### Fixed

- Derived the sidebar version label from the frontend package version instead
  of a separate hard-coded value.
- Added release-time version consistency checks for Cargo, Tauri, npm,
  documentation, and Git tags.

## 0.3.3 - 2026-07-16

### Changed

- Complex problem solving now stays in ordinary Chat or Tutor conversations,
  with retrieval, web, and code tools available when needed.

### Removed

- Retired the standalone Deep Solve capability, fixed solve workflow, and new
  run controls. Historical Deep Solve messages, traces, and Notebook artifacts
  remain readable.

## 0.3.2 - 2026-07-16

### Added

- Per-session Chat reading-position persistence that restores the previous
  viewport after conversation switching or app restart while preserving
  intentional bottom-follow behavior.

### Fixed

- Prevented in-flight or newly completed assistant messages from disappearing
  when session history loading races with WebSocket reconnection.
- Reconciled background run completion so session indicators do not remain
  stuck in the running state after rapid conversation switching.
- Rendered LLM-style `\(...\)` and `\[...\]` Markdown formulas through KaTeX
  without rewriting literal examples inside inline or fenced code.

## 0.3.1 - 2026-07-15

### Added

- Persistent Tutor profiles with Markdown Soul, default model and capability
  policy, resource permissions, and immutable runtime-session binding.
- A compact Tutor chooser for new conversations while retaining Temporary
  Assistant as the no-selection path.
- Per-Tutor private continuity memory for commitments, open loops, lesson
  plans, reflections, and teaching strategies, including inspect, edit,
  resolve, reopen, delete, and reset controls.

### Changed

- Learner Memory and Tutor Memory now use a tool-aware runtime routing policy:
  learner facts stay in shared Learner Memory, Tutor-owned continuity stays
  private, unavailable tools are omitted from prompts, and duplicate writes
  are forbidden.
- Tutor Soul now shapes Chat, Research, Quiz, and Deep Solve through the shared
  runtime path without introducing a separate agent implementation.
- Tutor model defaults and Knowledge, Notebook, Space, Quiz-source, and Learner
  Memory permissions are enforced at session and mounted-tool boundaries.
- Memory-backed answers use reliable prior context naturally instead of
  narrating internal memory lookups.
- The Memory workspace can switch directly between L2 and L3 files and keeps
  no-change workflow results reviewable.
- Settings configuration deletion now uses a compact icon-only action, and the
  new-conversation Tutor picker has a clearer Temporary Assistant default.

### Fixed

- Prevented stale Tutor editor state from overwriting newer backend changes.
- Restored shared Learner Memory access for authorized chat agents.
- Allowed no-change Memory reviews to close cleanly without forcing a write.

### Known Gaps

- Tutor-to-Tutor handoff and the consolidated Tutor conversation workspace are
  not complete.
- Autonomous Tutor Memory currently has prompt/tool-contract safeguards;
  server-side sensitive-content validation remains pending.
- Running workflows still do not resume from their exact interruption point
  after a full application restart.
- Linux packaging, code signing, automatic updates, and system-keychain secret
  storage are not yet available.

## 0.2.1 - 2026-07-14

### Added

- Persistent cool-light and graphite-dark application themes.
- A redesigned Memory maintenance workspace with selectable model and action,
  background run indicators, progress recovery, evidence-backed diffs, and
  per-change review.
- Layered Memory evidence: L2 reads L1 product events, while ordinary L3 runs
  read stable L2 entries and retain source provenance.
- A root user manual covering current product workflows and the planned Tutor
  capability.

### Changed

- Redesigned chat presentation with full-width Agent answers, higher-contrast
  user messages, hover actions, and theme-matched scrollbars.
- Made complete session rows clickable while preserving selection, pinning,
  ordering, and background activity behavior.
- Memory generation now follows target-specific L2/L3 tool boundaries, rejects
  unread evidence citations, repairs oversized proposed changes, and honors the
  selected interface language for newly generated memory.
- Research no longer owns a separate Memory category. Research conversation is
  summarized through Chat, and saved reports enter Memory through Notebook.
- Removed the Books surface and the legacy Memory consolidation path.
- Desktop release assets now include platform and architecture in their names.
- Hardened desktop development startup against stale Vite and `tutor-web`
  processes and serialized backend build/launch.

### Fixed

- Restored Memory run state and per-module indicators after workspace
  navigation.
- Fixed repeated L2 workflow failures caused by declaring tools that were not
  mounted for the active memory layer.
- Fixed invalid and aliased Memory evidence references across Notebook, Quiz,
  Chat, and Knowledge events.
- Fixed Memory overview alignment, composer model icon sizing, and dropdown
  icon surfaces.

### Known Gaps

- API keys are stored in local JSON for now; system keychain integration is a
  later hardening task.
- Running workflows do not yet resume from their exact interruption point after
  a full application restart.
- Memory L2 freshness warnings, apply-time source revision checks, and the full
  Teaching Strategy dependency order remain pending.
- Linux packaging and automatic updates are not yet available.
