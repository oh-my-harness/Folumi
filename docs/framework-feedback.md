# llm-harness-runtime v0.2 Framework Feedback

> Written after implementing tutor_agent v0.1 (2026-06-13).
> This document records friction points, missing APIs, and positive validations
> to inform llm-harness-runtime v0.3 planning.
>
> Historical note (2026-07-16): the standalone Deep Solve capability has been
> retired. References below describe runtime migration work completed while that
> capability still existed; they are retained as framework integration history,
> not as the current product architecture.

## Hooks Used

| Hook | Use Case | Verdict |
|------|---------|---------|
| `BeforeToolCallHook` | Human approval for sensitive tools | ✅ worked as designed |
| `PrepareNextTurnHook` | Historical PhaseManager active-tool filtering | ✅ worked; now replaced by workflow step tool scopes |
| `AfterProviderResponseHook` | BudgetControlAdapter cost accumulation | ✅ worked; now awaiting safer app-level budget policy |
| `ShouldStopHook` | BudgetControlAdapter loop stop policy | ⚠️ unsafe for ordinary one-turn chat with current semantics |

## Friction Points

- **Resolved 2026-07-24: Workflow LLM steps can receive trusted request extensions**
  - Checked against `codex/session-projection` commit `8ab2a377` on 2026-07-23.
    Ordinary product Chat now calls `AgentHarness::run(RunRequest)` and an
    integration test proves that a typed extension reaches
    `ToolContext.run`.
  - Rechecked on 2026-07-24 against the unchanged branch plus runtime `main`
    and run-safety PR #76 (`071f703`); `run_llm_step` still enters through
    `harness.prompt(&prompt_text)`.
  - `WorkflowEngine::run_llm_step` still calls `harness.prompt(&prompt_text)`.
    This creates a fresh request with no product extensions. The shared and
    per-step builder customizers can install plugins and tools, but cannot
    attach immutable trusted access data to the step's `RunContext`.
  - Impact: `KnowledgePlugin` can be mounted on Research/Quiz step harnesses,
    but `KnowledgeRequestContext::from_run` will fail closed because the
    `KnowledgeAccessContext` cannot reach those tool calls.
  - Suggestion: let `WorkflowEngine::run` accept a typed workflow request whose
    extensions are propagated to every LLM step, or add a trusted per-step
    `RunRequest` factory to `WorkflowEngineConfig`. The runtime should preserve
    extension non-serialization and run-local immutability.
  - Resolution: runtime `e9b72ed` adds `WorkflowRunRequest` and
    `WorkflowEngine::run_with_request`. `llm-tutor` now passes the trusted
    `KnowledgeAccessContext` into detailed Research and has an integration test
    proving `knowledge_search` and `knowledge_read` succeed inside workflow
    steps without product-owned run state.

- **Resolved 2026-07-24: Knowledge citation validation has a runtime final-answer boundary**
  - Checked against `codex/session-projection` commit `8ab2a377` on 2026-07-23.
    `KnowledgeReadTool` correctly issues and verifies evidence, registers a
    run-scoped citation handle, and persists only an ephemeral summary plus
    `knowledge.evidence` metadata.
  - Rechecked on 2026-07-24; no final-answer Knowledge validator is present on
    the pinned branch, runtime `main`, or PR #76 (`071f703`).
  - `CitationValidator` validates handles through `KnowledgeRunState` in the
    current `RunContext`, but `KnowledgePlugin` only registers search/read
    tools. It does not validate handles used by the final assistant message.
  - `AgentHarness::run` returns `Result<()>` and does not expose the completed
    `RunContext`. Existing `AfterTurnHook` is observational, so an application
    cannot atomically reject or repair an invalid final answer before it becomes
    the durable result without rebuilding citation state outside the runtime.
  - Suggestion: add a final-answer validation hook with access to
    `RunContext` and the candidate assistant message, or let
    `KnowledgePlugin` install a validator that can reject/repair unknown
    handles before final-answer persistence.
  - Resolution: runtime `526c186` adds the final-answer validator and
    `KnowledgeCitationPolicy`. `llm-tutor` uses
    `RequireWhenEvidenceRead`, including per-step Research plugins, and tests
    both ordinary Chat and a multi-step Research report with fresh run-local
    citation handles.

- **Historical: runtime `workflow` / issue #43 fix branch compiled, with provider tool-call adjacency improved**
  - `llm-tutor` tested `llm-harness-runtime` issue #43 fix branch commit `e200c12` and aligned `llm-api-adapter` to `69a868f`.
  - Runtime issue #43 remains open, but PR #44 (`Fix provider-specific tool message normalization boundary`) contains the relevant fix. The default converter no longer inserts an empty assistant between consecutive tool result messages, preserving OpenAI-compatible `assistant tool_calls -> tool -> tool` adjacency.
  - The branch adds workflow/subagent APIs and re-exports chat provider types from `llm_harness_loop`, but it still depends on the external `llm-api-adapter` repo. Embedding traits/types are not re-exported, so RAG code still needs a direct adapter dependency.
  - `HarnessBuilder` again exposes `system_prompt`, `model_info`, and `final_answer_mode`, and product Chat/Code Exec harness construction now uses builder plus a thin plugin to inject product hooks.
  - `submit_step_result` behaves as a terminal structured step response in workflow tests; no extra follow-up text turn is required for Memory/Quiz workflow mocks.
  - Suggestion: merge PR #44 into the main/workflow line, re-export embedding provider types or provide a runtime embedding boundary, and keep provider-specific message normalization in adapter/provider code rather than the provider-neutral runtime converter.

- **Historical migration log: runtime workflow support became available and was consumed**
  - `llm-tutor` now pins `llm-harness-runtime` to workflow branch commit `cc0b737`, which includes expanded workflow and spawn/subagent modules. Current product flows consume `WorkflowEngine` plus the runtime JSONL session factory; no separate free-form subagent is needed for the migrated paths.
  - The old adapter pin conflict is resolved by aligning `llm-api-adapter` to the runtime-compatible revision.
  - First migration step: Deep Solve now defines its phase graph as an `llm_harness_runtime::workflow::model::Workflow` and validates it through `validate_workflow` before execution.
  - Second migration step: Quiz generation now defines its controlled product flow (`collect_sources -> generate_questions -> verify_questions -> publish_questions`) as a runtime `Workflow` and validates it through `validate_workflow` before generation.
  - Third migration step: ordinary Chat, Code Exec, and the existing Deep Solve phases now construct harnesses through runtime `HarnessBuilder` instead of manually assembling `AgentHarnessOptions`; product code only maps tools, prompts, and hooks into a thin builder config.
  - Fourth migration step: Deep Solve and Quiz workflow edges now use runtime-evaluable `EdgeCondition::Expr` predicates instead of legacy label strings, so the built-in `EdgeConditionJudge` can route them once execution moves to `WorkflowEngine`.
  - Fifth migration step: Quiz generation now performs a controlled verifier repair pass that mirrors the runtime workflow's `verify_questions -> generate_questions` repair edge while the full `WorkflowEngine` execution path is being adopted.
  - Sixth migration step: `llm-tutor` now has a thin `runtime_engine` adapter that builds `WorkflowEngineConfig` from the product `ExecutionEnv`, LLM client, model, and runtime JSONL session factory. A smoke test runs an executor workflow through runtime `WorkflowEngine`.
  - Seventh migration step: Quiz generation now has a product runtime workflow path. `collect_sources`, `generate_questions`, `verify_questions`, and `publish_questions` run as runtime executor steps, and the repair edge is driven by `WorkflowEngine` transitions. The web Quiz route and chat `create_quiz` tool now call this runtime workflow path when an LLM is configured.
  - Eighth migration step: Memory assist/update/check/dedupe now has a runtime `WorkflowEngine` path. The web memory route builds a runtime workflow config with a JSONL session root and runs the LLM-backed memory workflow through a registered runtime executor.
  - Ninth migration step: Deep Solve now runs through runtime `WorkflowEngine`. Product code registers a retrieve executor, product tools/hooks, and a thin event bridge; runtime owns the plan/solve/synthesize LLM step sessions, step history, `submit_step_result` routing, and workflow transitions.
  - Tenth migration step: Quiz generation and verification now run as runtime LLM steps. Product code only collects sources into workflow context, publishes the final validated questions, and enforces a bounded repair loop through a thin workflow judge; the model submits structured quiz and verifier results through runtime `submit_step_result`.
  - Eleventh migration step: Memory assist/update/check/dedupe now runs as a runtime LLM step. Product code prepares the memory prompt in workflow context and validates the submitted structured memory result; the model submits memory facts/edits through runtime `submit_step_result`.
  - Twelfth migration step: agent-side legacy direct structured-output helpers have been removed. Deep Solve, Quiz, and Memory now use runtime workflow/harness paths for LLM orchestration; product code keeps only domain validation, source repair, and runtime executor bridges.
  - Thirteenth migration step: app-side declarative edge evaluation has been removed. Deep Solve and Memory now pass a no-op marker into `WorkflowEngine::new`, allowing runtime's built-in declarative edge judge to own `EdgeCondition::Expr` routing.
  - Fourteenth migration step: legacy Deep Solve `PhaseManager`, `ReplanHook`, `ReplanTool`, and `SolveContext` have been removed. Replanning is now represented only as workflow structured output (`submit_step_result` with `route:"replan"`) and runtime edge transitions.
  - Fifteenth migration step: Quiz and Memory workflow APIs no longer accept duplicate client/model parameters; runtime client/model ownership now flows only through `WorkflowEngineConfig`.
  - Sixteenth migration step: upgraded all runtime crates to `cc0b737` and verified `tutor-agent` / `tutor-web` compile against the workflow runtime branch. The newest runtime still keeps `NoopJudge`, `EdgeConditionJudge`, and fixed-env helpers private, so the tiny product marker judge and env factory remain necessary thin adapters.
  - Seventeenth migration audit: re-tested `HarnessBuilder::budget(limit, None)` on `eea964b` for ordinary Chat/Code Exec harnesses. `cargo test -p tutor-agent --test mock_integration` still timed out, so product code continues to avoid wiring runtime budget hooks into one-turn harnesses until the stop semantics are safe for this usage.
  - Eighteenth migration step: Chat/Code Exec now follow runtime's final-answer contract through `FinalAnswerMode::tool_with_text_fallback()` and `AgentEvent::as_final_answer()` / `as_progress()`, so durable assistant bubbles no longer come from intermediate progress text.
  - Nineteenth migration step: Deep Solve previously consumed runtime `WorkflowEvent::StepProgress`; `cc0b737` removed this event, and `e200c12` restores it. The current product bridge still reports workflow step start/finish/failure only until the UI needs finer step-internal progress.
  - Twentieth migration step: Chat and Code Exec emit product `runtime_usage` traces from `AgentHarness::usage()`, and Deep Solve emits the same trace from runtime `TaskResult.cost`.
  - Twenty-first migration step: Quiz and Memory workflow helpers now return runtime `TaskResult.cost` alongside domain output, so callers no longer need to reconstruct workflow usage from app-layer state.
  - Twenty-second migration step: upgraded runtime crates to PR #44 commit `e200c12` and adapter to `69a868f`. Chat/Code Exec returned to `HarnessBuilder`, with product hooks injected through a minimal plugin, so runtime owns cost hook injection again.
  - Twenty-third migration step: Research detailed runs now use runtime `WorkflowEngine` with explicit search, read, citation-check, and report steps. Product code only prepares the confirmed request, mounts existing tools, bridges workflow events into Research trace events, and persists the final report back to the current runtime session.
  - Twenty-fourth migration step: runtime `e9b72ed` closes both Knowledge A6
    gates. Chat now enforces final-answer citations after evidence reads, and
    detailed Research receives trusted access through `WorkflowRunRequest`,
    uses step-scoped Knowledge plugins, and no longer mounts `rag_search`.
  - Historical completion audit at that checkpoint: the project pin was
    `e200c12`; `cargo tree -p tutor-agent` showed one `llm_adapter` source
    (`69a868f`) and one runtime revision. Active source no longer contained the
    legacy Deep Solve phase loop or app-side declarative edge evaluation paths.
  - Remaining migration target: settings diagnostics still use a direct adapter probe because they are provider connectivity checks, not agent orchestration. Further cleanup depends on runtime/adapter support for provider-native structured LLM step options, public declarative/no-op judge helpers, typed validation/retry helpers, safe budget policies, and normalized model metadata discovery.

- **WorkflowEngine needs an app-friendly cancellation handle**
  - Expected: app code can start `WorkflowEngine::run()` and connect an external stop token without owning a cloneable engine or manually spawning a task around internal step abort state.
  - Actual: `WorkflowEngine::cancel()` exists but the engine is not cloneable, so a route that awaits `run()` directly cannot also call `cancel()` from an external `CancellationToken` without changing ownership structure.
  - Suggestion: expose `run_with_cancel(token)` or a lightweight cloneable `WorkflowHandle` returned by `start()`, so product routes can wire stop buttons consistently across ordinary harness turns and multi-step workflows.

- **Budget control still needs a safer runtime API**
  - Product code no longer constructs `BudgetControlAdapter` directly for ordinary harness setup; it now carries only the session budget limit in `GovernanceConfig`.
  - Attempting to wire `HarnessBuilder::budget(..., None)` into ordinary one-turn Chat/Code Exec harnesses still makes mock integration tests hang on runtime `eea964b`, matching the earlier `ShouldStopHook` semantic issue: `false` means "continue loop", not "allow this call and finish normally". This has not been re-enabled on `cc0b737`.
  - `WorkflowEngineConfig` exposes hooks and step cost aggregation, but does not yet expose a simple builder-style `budget(...)` / shared budget policy API for multi-step workflows.
  - Follow-up: add runtime budget helpers that distinguish per-call budget accounting from agent-loop stop decisions, and expose the same policy for ordinary harnesses and workflows.

- **`BeforeToolCallCtx` requires a live `AssistantMessage` reference, making unit tests noisy**
  - Expected: construct a minimal mock in test code to verify hook logic
  - Actual: `BeforeToolCallCtx` borrows `AssistantMessage`, preventing straightforward construction. Tests need `Box::leak` or an `Arc` with full field population just to test a simple allow/deny decision.
  - Suggestion: add a `BeforeToolCallCtx::new(name, args)` constructor for tests, or make `assistant_message` borrow `Option`al.

- **`AuditEntry` requires hash-chain fields (`hash`, `prev_hash`) that are internally managed by `JsonlAuditSink`**
  - Expected: callers should provide domain fields only, the sink fills in the chain
  - Actual: `AuditEntry` is public with required hash fields that callers must guess at. The sink then overwrites them. Unclear contract.
  - Suggestion: split into `AuditPayload` (caller-provided) and `AuditEntry` (sink-computed with hash chain), or make hash fields `Option` with `take()` semantics.

- **Resolved: repeated harness setup has been consolidated**
  - Earlier `SolveOrchestrator` phases repeated manual `AgentHarnessOptions` and harness setup code.
  - `llm-tutor` now uses runtime `HarnessBuilder` for ordinary capability harness setup and runtime `WorkflowEngine` for Deep Solve, Quiz generation, and Memory assist workflows.
  - Follow-up: reduce product-side bridge code as runtime exposes public declarative judges and typed structured-output helpers.

- **Session option/metadata types are not re-exported from the root facade**
  - Expected: common session types used with `JsonlSessionRepo` can be imported from `llm_harness_agent::{...}` or the prelude.
  - Actual: `JsonlSessionRepo`, `Session`, and `SessionRepo` are root exports, but `CreateSessionOptions`, `ListSessionOptions`, and `SessionMetadata` require `llm_harness_agent::session::{...}`.
  - Suggestion: re-export these common session types at the root/prelude for a smoother app-layer integration.

- **`SessionInfo` entries do not appear to update session metadata name via `Session::append`**
  - Expected: appending `SessionEntryPayload::SessionInfo { name }` updates `SessionMetadata.name`, matching the type comment that the most recent `SessionInfo` wins.
  - Actual: `Session::append` updates model metadata for `ModelChange`, but not name metadata for `SessionInfo`, so apps still need a separate title derivation path.
  - Suggestion: update metadata name in `Session::append` when a `SessionInfo` payload is appended, or expose a public high-level `Session::set_name`.

- **Resolved: runtime pin previously blocked downstream embedding usage**
  - Expected: once `llm-api-adapter` adds `EmbeddingProvider`, downstream apps can update and use it for RAG indexing.
  - Actual: `llm-harness-runtime` still depends on the older adapter revision, so `llm-tutor` cannot independently bump `llm_adapter` without ending up with two incompatible `Provider` traits in the dependency graph.
  - Suggestion: update `llm-harness-runtime` to the adapter revision that includes embedding support, then optionally re-export embedding traits/types from the runtime facade.

- **Resolved: latest runtime HEAD can now be consumed by Cargo**
  - Expected: pinning `llm-harness-runtime` to the latest commit should fetch cleanly as a git dependency.
  - Actual: commit `c6eba08` pulls submodule `examples/coding-agent` with URL `git@github.com:oh-my-harness/coding-agent.git`, which Cargo reports as an invalid relative URL.
  - Suggestion: use a valid absolute SSH URL such as `ssh://git@github.com/oh-my-harness/coding-agent.git`, or avoid requiring example submodules for library consumption.

- **Typed structured-output validation still needs app-level boilerplate**
  - Expected: product flows like quiz generation can ask the framework for typed JSON output with provider-aware schema support, retries, and validation error reporting.
  - Actual: runtime LLM steps declared with `with_structured(Some(true))`
    extract JSON from the final assistant text and can upgrade to a native JSON
    response format when the provider reports support. Product code still
    supplies prompt-level shape instructions and performs domain deserialization
    and validation after the runtime returns `serde_json::Value`.
  - Suggestion: add a runtime or agent helper such as `generate_structured<T>(prompt, schema/options)` that uses provider capabilities, validates typed output, and returns structured errors suitable for UI display.

- **Structured product flows cannot yet combine typed output with normal tool orchestration**
  - Expected: a product flow such as Quiz can ask the model to call tools like `read_memory`, then return validated typed JSON questions in one runtime-managed flow.
  - Actual: Research, Quiz, and Memory workflow steps can mount tools and end
    with runtime-extracted structured JSON. The remaining gap is typed schema
    ownership and domain validation, not a synthetic submit Tool.
  - Suggestion: add a runtime pattern for "tool-using structured generation", for example `AgentHarness::generate_structured_with_tools<T>()`, where tools, trace events, schema output, validation, and retries are all runtime-managed.

- **Default workflow judge helpers are still not public**
  - Expected: product code can construct a declarative `EdgeCondition::Expr` workflow through public runtime APIs without implementing any `StepTransitionJudge`.
  - Actual: runtime now auto-selects its built-in edge judge when the provided judge reports `is_noop()`, so `llm-tutor` no longer duplicates edge evaluation. However, runtime's `NoopJudge` / `EdgeConditionJudge` are still `pub(crate)`, so apps need a tiny marker judge solely to opt into the built-in behavior.
  - Suggestion: expose a `WorkflowEngine::new_declarative(workflow, config)` helper that selects the built-in edge judge automatically, or publish a small no-op marker constructor.

- **Workflow semantic repair loops still need a declarative bounded policy**
  - Expected: product flows such as Quiz can express verifier repair loops with declarative runtime workflow edges and runtime-owned visit limits.
  - Actual: runtime `EdgeConditionJudge` can route based on `StepResult.structured`, and `StepExecutionPolicy.max_attempts` can retry execution failures, but there is no declarative edge condition or step policy for "repair once, then fail" based on semantic verifier output and prior step visits. `llm-tutor` therefore keeps a thin `QuizWorkflowJudge` only to bound the `verify_questions -> generate_questions` repair loop.
  - Suggestion: let edge conditions inspect step visit counts / `step_history`, or add a workflow-level `max_visits_per_step` / `max_semantic_repairs` policy that can be attached to a transition.

- **Durable background agent runs need a runtime rejoin primitive**
  - Expected: long-running agent or workflow runs can survive UI/WebSocket disconnects, expose a stable run id/status, replay missed progress, and recover to a clear terminal state after process restart when possible.
  - Actual: `llm-tutor` can keep active runs alive in-process and rejoin them by session id, but full run envelopes, missed-progress replay, restart recovery, and cancellation state are not first-class runtime session concepts yet.
  - Suggestion: add runtime-managed run records with `run_id`, `session_id`, `assistant_message_id`, status transitions, cancellation, progress cursors, and rejoin/replay APIs so product UIs do not need their own scheduler or event journal.

- **Runtime sessions need an atomic append or shared write coordinator**
  - Expected: harness-owned assistant writes and app-owned custom entries such as traces can safely target the same durable session without creating accidental branches.
  - Actual: `Session::append` reads the active cursor, appends, and advances the cursor as separate operations. When an event sink appends a trace while the harness appends the final assistant message, both entries can receive the same parent and the later cursor update hides one sibling from the active path.
  - Product workaround: stream run events immediately but buffer their session entries until the harness turn has settled, then append traces, artifacts, and stage records sequentially after the final assistant message. Transient cancellation state is not appended during the active turn.
  - Suggestion: expose an atomic append primitive or a session-scoped writer/transaction that the harness and application event sinks can share.

- **Model metadata discovery is still app-specific**
  - Expected: settings diagnostics and runtime context budgeting can ask the provider adapter for normalized model metadata such as context window, native embedding dimension, supported embedding dimensions, and detected source.
  - Actual: `llm-tutor` has to implement a thin `GET /models` probe, provider-specific auth headers, endpoint derivation, and recursive parsing of fields such as `context_window`, `max_context_tokens`, and `max_model_len`.
  - Suggestion: add an `llm-api-adapter` capability such as `list_models()` / `model_metadata(model)` and expose source labels (`metadata`, `known_model`, `default`) so apps do not duplicate provider quirks.

- **Resolved: compact summaries now read directly from runtime session entries**
  - Expected: UI-visible compaction summaries should come from runtime `SessionEntryPayload::Compaction` records.
  - Actual: `llm-tutor` previously wrote an additional custom `compact_summary` entry after each chat turn, which duplicated runtime-owned session/compaction state.
  - Change: removed the product-layer summary mirror. Session detail responses still expose `compact_summary`, but it is now derived only from the latest runtime compaction entry.

- **Resolved: final assistant bubbles now follow the runtime final-answer contract**
  - Expected: UIs should restore durable assistant bubbles only from runtime `AssistantMessageKind::FinalAnswer`; intermediate `Progress` messages may remain in runtime context but should not appear as final chat answers.
  - Actual: Chat/Code Exec previously treated every `MessageEnd` and streamed `TextDelta` as candidate final answer text, so progress before tool calls could be returned or restored as a normal assistant bubble.
  - Change: Chat/Code Exec now match runtime `AgentEvent::FinalAnswer` / `Progress`, return only `FinalAnswer` text, and web session rendering ignores `Progress` assistant messages when mapping runtime messages to chat roles. Harness construction also enables runtime `FinalAnswerMode::tool_with_text_fallback()`, so models may use the built-in `final_answer` tool without losing plain-text compatibility.
  - Remaining gap: streamed `TextDelta` is still emitted immediately for UX. If the runtime later exposes per-delta final/progress classification, the UI can avoid briefly showing progress text in the main stream.

- **OpenAI-compatible adapters still need to normalize reasoning-only assistant history**
  - Expected: a prior assistant message containing only reasoning/thinking blocks should not be serialized as an invalid OpenAI-compatible assistant message.
  - Actual: the `e200c12` runtime converter preserves `ResponseContent::Reasoning`; the `69a868f` OpenAI adapter serializes it as `reasoning_content`. Some OpenAI-compatible providers reject an assistant message that has neither `content` nor `tool_calls`, returning `Invalid assistant message: content or tool_calls must be set`.
  - Product workaround: `llm-tutor` installs a thin `OpenAiSafeContextConverter` that delegates to runtime's `DefaultConvertToLlm` and then drops assistant history messages with no text and no tool invocations. Tool-call assistant messages are preserved, so OpenAI tool-call adjacency remains intact.
  - Suggestion: handle this at the adapter/runtime boundary, either by omitting reasoning-only assistant messages for OpenAI-compatible wire formats or by mapping provider-supported reasoning history into a valid content representation.

- **Resolved: workflow steps use runtime structured final output**
  - Runtime commit `8ab2a377` removed the synthetic
    `submit_step_result` Tool contract.
  - Research, Quiz, and Memory LLM steps now declare
    `with_structured(Some(true))`, finish with a plain JSON assistant response,
    and route on `StepResult.structured`.
  - Product prompts and mocks no longer reference `submit_step_result`.

- **Resolved: workflow step progress events are available again on `e200c12`**
  - Expected: product UI should reuse runtime workflow progress events for step-internal tool/message boundaries instead of inventing a parallel progress model.
  - Actual: `cc0b737` temporarily removed `WorkflowEvent::StepProgress`, but PR #44 commit `e200c12` exposes it again.
  - Current product decision: keep the existing coarse Deep Solve bridge for now; wire `StepProgress` into UI only when a concrete step-internal progress design is needed.

## Positive Validations

- **CompositeBeforeToolCallHook** can layer domain-specific + cross-cutting hooks; current product code only needs human approval hooks after moving replan into workflow routing
- **BudgetControlAdapter** dual-role as `AfterProviderResponseHook` + `ShouldStopHook` is elegant — one instance, two contracts
- **`active_tools` in `NextTurnDirective`** was useful for historical PhaseManager-style filtering; current workflow paths prefer runtime step `allowed_tools`
- **`HarnessHooks::none()`** pattern with struct update syntax (`..HarnessHooks::none()`) makes selective hook wiring readable
- **`AgentHarness::subscribe()` before `prompt()`** pattern allows reliable event collection without race conditions

## API Gaps

| Gap | Description | Severity |
|-----|-------------|----------|
| No test-helper constructors for hook context types | Building `BeforeToolCallCtx` in tests is unnecessarily hard | Medium |
| Session options/metadata missing from root/prelude exports | Apps need mixed import paths for common session operations | Low |
| SessionInfo does not update metadata name | Session titles need app-layer workaround | Medium |
| AuditEntry hash fields leak implementation detail | Callers must provide hash-chain fields that the sink overwrites | Low |
| WorkflowEngine migration bridge still needs thin product adapters | Product flows still need executor state mapping, product trace bridges, and typed domain validation around runtime structured output | Low |
| No typed structured-output helper | Product flows must duplicate JSON extraction, schema hints, validation, and retry policy | Medium |
| No tool-using structured-generation helper | Product flows such as Quiz cannot combine `read_memory` tool orchestration with typed JSON output without a parallel loop | Medium |
| No public declarative/no-op workflow judge helper | Product workflows need a tiny marker judge to opt into runtime's built-in declarative edge router | Low |
| No declarative bounded semantic repair policy | Quiz still needs a thin product judge to cap verifier-driven repair loops | Medium |
| No safe app-level budget policy helper | Ordinary one-turn harnesses and multi-step workflows cannot share budget accounting without app-layer loop-risk or hook boilerplate | Medium |
| No normalized model metadata API | Apps duplicate `/models` probing, auth headers, context-window parsing, and embedding dimension capability discovery | Medium |

## Proposed v0.3 Changes

1. Add `BeforeToolCallCtx::new_test(name, args)` that uses a dummy assistant message internally
2. Make `AuditEntry.hash` and `AuditEntry.prev_hash` optional with internal fill-in, or split into payload vs entry types
3. Continue documenting `HarnessBuilder` examples for app-layer harness factories and plugin-based hook injection
4. Re-export common session repo option and metadata types from the facade/prelude
5. Add `Session::set_name` or metadata updates for `SessionInfo`
6. Add a typed structured-output helper for provider-aware JSON/schema generation
7. Add a tool-using structured-generation helper for flows like memory-aware Quiz
8. Add normalized model metadata discovery in the adapter/runtime boundary
9. Expose a public no-op/declarative workflow judge helper or `WorkflowEngine::new_declarative` constructor
10. Continue hardening `WorkflowEngine` examples for app-level workflows that mix executor steps, LLM steps, and subagent reviewers
11. Add a safe app-level budget helper/policy API that separates cost accounting from loop continuation
12. Add declarative bounded semantic repair policies for verifier-driven workflow loops
