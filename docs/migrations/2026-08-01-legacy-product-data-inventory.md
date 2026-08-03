# Folumi Legacy Product Data Inventory

> Status: Superseded by the 2026-08-03 removal decision
>
> Original inventory date: 2026-08-01
>
> Decision date: 2026-08-03

This document remains as a historical inventory. It no longer defines a
migration or export feature. Folumi does not expose UI or API operations that
preview, import, transform, or archive legacy Tutor/Quiz data.

## Current Decision

- Keep the existing `.llm-tutor` directory, Tauri identifier, environment
  variables, and storage keys so supported Folumi data remains discoverable.
- Continue to support current sessions, Sources, Notes, settings, and data
  already stored in the unified runtime Memory backend.
- Do not read Tutor-private continuity, Quiz records, Student Profile data, or
  retired workflow state during normal product operation.
- Do not import legacy data into User Memory or Assistant Continuity.
- Do not provide a legacy ZIP or other application-managed archive.
- Do not delete legacy files automatically. Users who need them must back up
  the application data directory directly before cleaning it manually.
- Do not introduce fallback reads, dual writes, or hidden compatibility routes.

## Historical Inventory and Final Disposition

| Data | Final Folumi disposition |
| --- | --- |
| Settings and provider configuration | Preserve as supported product data. |
| Conversation sessions and messages | Preserve through runtime sessions. |
| Knowledge-base definitions and source documents | Preserve as RAG Sources. |
| Vector indexes and chunks | Treat as rebuildable derivatives. |
| Notebook and external Vault configuration | Preserve as user-owned Notes. |
| Unified runtime Memory data | Preserve and expose through the Memory workspace. |
| Tutor profiles, Soul, and private continuity | Leave files untouched; do not read, import, or export them. |
| Quizzes, answers, and Quiz-derived memory | Leave files untouched; do not read, import, or export them. |
| Retired workflow, Space, Student Profile, and teaching data | Leave files untouched and outside active product behavior. |

## Removal Acceptance Contract

The removal is complete only when:

1. no primary page or settings surface presents legacy migration or archive
   controls;
2. no `/api/migration/legacy*` route is registered;
3. the web backend has no legacy ZIP export path (Notebook ZIP import/export
   remains a separate supported capability);
4. product requirements, user documentation, and desktop QA do not require a
   legacy import/export workflow; and
5. supported sessions, Sources, Notes, settings, and unified Memory continue to
   work without consulting legacy teaching stores.

Restoring any legacy migration or archive capability requires a new explicit
product decision and matching PRD, user-documentation, and regression-test
updates.
