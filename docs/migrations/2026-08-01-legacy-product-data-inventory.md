# Folumi Legacy Product Data Inventory

> Status: implemented Phase 5 migration boundary | Date: 2026-08-01

This document records the user data that exists before the Folumi product
contraction. It defines what must be preserved, transformed, or exported before
legacy teaching features are removed. It is an inventory and acceptance
contract, not a compatibility-layer design.

## Principles

- Never delete user data merely because its feature is no longer in the target product.
- Keep the current `.llm-tutor` directory, Tauri identifier, environment variables, and storage keys unchanged for this contraction so existing user data remains discoverable.
- Expose an explicit, one-time migration action; do not dual-write old and new models.
- Make continuity import idempotent and previewable. It is reversible because legacy data is never modified or deleted.
- Separate source material, user-authored notes, and assistant memory throughout migration.
- Derived indexes may be rebuilt, but their source documents and configuration must survive.

## Inventory and Disposition

| Current data | Current location | Folumi disposition | Required handling |
| --- | --- | --- | --- |
| Settings and provider configuration | `settings.json` | Preserve | Migrate schema keys only when needed; do not expose secrets in export logs. |
| Conversation sessions and messages | `sessions/` | Preserve | Retain history, attachments, citations, and runtime session mappings. |
| Knowledge-base definitions and source documents | `knowledge-bases.json` and RAG source storage | Preserve as Sources | Keep original files and metadata; verify citations after rebuilding indexes. |
| Vector indexes and chunks | `rag/` | Rebuildable derivative | Back up, then rebuild from preserved Sources when schema or embedding configuration changes. |
| Notebook and external Vault configuration | `notebook/` and settings | Preserve as Notes | Keep Markdown, frontmatter, wiki links, backlinks, filenames, and external paths. |
| Learner memory | `memory/` | Preserve as User Memory | The unified runtime memory backend exposes this data through user-visible review, edit, approval, and forget controls. |
| Tutor profiles, Soul, and private continuity | `tutors/` and tutor memory | Export definitions; optionally import selected continuity | Let the user export every Tutor. Only explicitly selected active continuity items are copied into Assistant Continuity with provenance. |
| Quizzes, answers, and Quiz-derived memory | `quizzes.json`, `memory/L1/quiz_events.jsonl`, and `memory/L2/quiz.md` | Export, then retire | Preserve the raw JSON/JSONL/Markdown in the legacy ZIP; Folumi does not create, maintain, or recall Quiz memory. |
| Quiz workflow state | `workflow-sessions/` quiz records | Export or discard after confirmation | Preserve completed outputs; incomplete execution state need not remain executable. |
| Research reports and source lists | sessions, workflow records, Notebook | Preserve useful reports as Notes | Retain report content and citations; retire the standalone mode and resumable workflow state. |
| Space and Student Profile data | product stores and memory evidence | Export and review | Export authored/derived material; do not silently turn inferred profiles into Memory. |
| Other workflow traces | `workflow-sessions/` | Audit-only export or prune | Keep only when needed to explain a retained result; obtain confirmation before pruning. |

The implementation must verify actual serialized schemas and paths before the
migration tool is written. This table defines user-visible intent, not permission
to infer unverified on-disk layouts.

## One-Time Migration Shape

1. `GET /api/migration/legacy` scans legacy Tutor continuity and reports selectable active items without writing data.
2. The user chooses the items that should become Assistant Continuity.
3. `POST /api/migration/legacy/continuity` validates the selected identifiers and writes them through the unified memory backend; repeated imports are ignored by stable migration provenance.
4. `GET /api/migration/legacy/export.zip` produces a read-only ZIP archive of legacy Tutor definitions and Quiz data.
5. The legacy source files remain untouched. Folumi has no fallback read path and no dual-write path after the explicit action.

The migration endpoints form a bounded retirement operation, not a runtime
compatibility layer. A failed import leaves the legacy directory untouched and
can be retried safely.

## Old Repository Archive Gate

Archive `oh-my-harness/llm-tutor` only after all of the following are true:

- Folumi `main` passes the Rust workspace, frontend, and desktop build gates.
- A current legacy data directory opens or migrates without losing sessions, Sources, Notes, or settings.
- Knowledge search, original-source reading, citations, and source navigation pass smoke QA.
- Notes can be created, edited, linked, imported, and exported.
- Memory can be disabled, reviewed, corrected, approved, and forgotten.
- Frozen Quiz, Space, Student Profile, Tutor, and Research data has an explicit export path.
- At least one Folumi desktop release artifact passes install-and-launch smoke QA.
- The old repository README points to Folumi and the repository is made read-only.
