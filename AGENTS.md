# Development Principles

- Use `llm-harness-runtime` / `llm-harness-agent` first. Do not reimplement
  session, context building, tool orchestration, hooks, trace, compaction, or
  provider behavior in this repo.
- Keep `llm-tutor` focused on product data and UI: knowledge bases, documents,
  spaces, notebooks, quizzes, settings, and mappings to runtime session IDs.
- For durable conversation history, prefer runtime sessions such as
  `AgentHarness::with_session` and runtime session repos.
- If the framework API is awkward or missing a needed capability, record it in
  `docs/framework-feedback.md` instead of silently building a parallel system.
- When diagnosing problems, prefer a root-cause design fix over accumulating
  patches that make the project heavier or harder to reason about.
- Keep adapters between product code and runtime code thin, explicit, and
  covered by boundary tests.
- When using PowerShell to inspect or transform text files, explicitly use
  UTF-8 for files that may contain Chinese or other non-ASCII text, for example
  `Get-Content -Encoding UTF8`, to avoid introducing mojibake into UI copy or
  docs.
- On Windows, treat Rust commands that link workspace binaries as heavyweight:
  run only one Cargo build/test/Clippy command at a time with
  `CARGO_BUILD_JOBS=1`, validate targeted crates before one full-workspace
  gate, and never run tests and Clippy in parallel. If a command times out,
  inspect `cargo`/`rustc`/`link` processes and wait for or terminate that exact
  process tree before retrying; do not start a duplicate build while orphaned
  compiler or linker processes are still running.
- After completing a meaningful task, commit the changes promptly.
