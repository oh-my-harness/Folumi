# Specs Index

Current product and runtime-facing specs live here.

- `2026-06-26-product-requirements-spec.md` is the consolidated product requirement source.
- `2026-08-03-primary-workspaces-decision.md` is the accepted information-
  architecture decision that keeps the internal Knowledge Base domain RAG-only
  and preserves Notebook and Memory as standalone primary workspaces. Its
  2026-08-07 amendment defines the user-facing labels Sources/资料,
  Notebook/笔记, and Personalization/个性化 without renaming internal APIs.
- `2026-08-03-user-memory-redesign.md` is the accepted Memory design. Its global
  Saved Memory is implemented. The opt-in runtime History Recall baseline is
  integrated as visible, agent-decided tool search without pre-run automatic
  injection. Temporary conversations and persistent local SQLite/FTS5 are
  implemented. The Phase 2 lexical baseline and the subsequently discovered
  semantic-paraphrase gap are recorded in `../qa/history-recall-acceptance.md`.
  The external LoCoMo long-conversation retrieval adapter, licensing boundary,
  layered metrics, and reproduction command are documented in
  `../qa/locomo-benchmark.md`.
- `2026-08-05-remove-product-billing-decision.md` records the accepted removal
  of product-side USD cost display and budget controls while retaining token
  and context usage for diagnostics and benchmarks.
- `2026-08-06-notebook-file-naming-decision.md` 以 Markdown 文件名作为
  Notebook 唯一命名来源，并统一文件树重命名交互。
- `2026-08-06-notebook-live-preview-decision.md` 规定 Notebook 使用 Vditor
  即时渲染编辑、自动保存和 Markdown 文件持久化，
  并明确替代早期的块级 textarea Live Preview 实现。
- `2026-08-07-plain-language-ui-copy.md` 规定主界面采用简短、结果导向的
  普通用户文案，并明确用户词汇与内部技术词汇的边界。
- `2026-06-26-memory-consolidation-design.md` is the retired L1/L2/L3 memory
  design and remains only as historical context.
- `2026-07-15-persistent-tutor-design.md` defines persistent tutor identity,
  Markdown Soul, private Tutor Memory, permissions, and handoff boundaries.

Historical v0.1 runtime-demo designs were moved to `docs/plans/archive/`.
They describe pre-migration `PhaseManager`, `ReplanHook`, and direct
`BudgetControlAdapter` wiring, which are no longer current. For the active
runtime migration status, see `docs/framework-feedback.md`.
