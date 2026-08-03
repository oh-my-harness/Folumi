# Specs Index

Current product and runtime-facing specs live here.

- `2026-06-26-product-requirements-spec.md` is the consolidated product requirement source.
- `2026-08-03-primary-workspaces-decision.md` is the accepted information-
  architecture decision that keeps Knowledge Base RAG-only and preserves
  Notebook and Memory as standalone primary workspaces.
- `2026-08-03-user-memory-redesign.md` is the accepted Memory design. Its Saved
  Memory baseline is implemented; opt-in History Recall and workspace scope
  remain gated by their documented prerequisites.
- `2026-06-26-memory-consolidation-design.md` is the retired L1/L2/L3 memory
  design and remains only as historical context.
- `2026-07-15-persistent-tutor-design.md` defines persistent tutor identity,
  Markdown Soul, private Tutor Memory, permissions, and handoff boundaries.

Historical v0.1 runtime-demo designs were moved to `docs/plans/archive/`.
They describe pre-migration `PhaseManager`, `ReplanHook`, and direct
`BudgetControlAdapter` wiring, which are no longer current. For the active
runtime migration status, see `docs/framework-feedback.md`.
