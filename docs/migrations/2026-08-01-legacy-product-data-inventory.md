# Folumi Legacy Product Data Inventory

> Status: Phase 1 migration contract | Date: 2026-08-01

This document records the user data that exists before the Folumi product
contraction. It defines what must be preserved, transformed, or exported before
legacy teaching features are removed. It is an inventory and acceptance
contract, not a compatibility-layer design.

## Principles

- Never delete user data merely because its feature is no longer in the target product.
- Keep the current `.llm-tutor` directory, Tauri identifier, environment variables, and storage keys unchanged during Phase 1.
- Perform one versioned migration after the target information architecture is stable; do not dual-write old and new models.
- Make migration idempotent, previewable, backed up, and reversible until the user accepts the result.
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
| Learner memory | `memory/` | Review and transform | Present candidate User Memory items with provenance; import only accepted items. |
| Tutor profiles, Soul, and private continuity | `tutors/` and tutor memory | Export; optionally transform one selected profile | Let the user export every Tutor. A selected profile may seed the single Assistant configuration and accepted continuity memory. |
| Quizzes and answers | `quizzes.json` | Export, then retire | Export human-readable Markdown/JSON with questions, answers, results, and timestamps. |
| Quiz workflow state | `workflow-sessions/` quiz records | Export or discard after confirmation | Preserve completed outputs; incomplete execution state need not remain executable. |
| Research reports and source lists | sessions, workflow records, Notebook | Preserve useful reports as Notes | Retain report content and citations; retire the standalone mode and resumable workflow state. |
| Space and Student Profile data | product stores and memory evidence | Export and review | Export authored/derived material; do not silently turn inferred profiles into Memory. |
| Other workflow traces | `workflow-sessions/` | Audit-only export or prune | Keep only when needed to explain a retained result; obtain confirmation before pruning. |

The implementation must verify actual serialized schemas and paths before the
migration tool is written. This table defines user-visible intent, not permission
to infer unverified on-disk layouts.

## One-Time Migration Shape

1. Detect the legacy schema version and create a timestamped backup manifest.
2. Scan data and show a dry-run summary grouped into Preserve, Review, Export, and Rebuild.
3. Ask the user to choose any Tutor profile or memory items that should become Assistant configuration or User Memory.
4. Write the new model into a staging directory and validate referential integrity.
5. Atomically switch to the new data directory only after validation succeeds.
6. Keep the backup and migration report until the user explicitly removes them.

The migration must not maintain a permanent adapter, fallback read path, or
dual-write mechanism. A failed migration leaves the legacy directory untouched.

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
