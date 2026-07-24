# llm-tutor Product Requirements Spec

> Status: active | Date: 2026-06-26 | Last updated: 2026-07-17 | Scope: consolidate current and planned product requirements into one itemized spec.

## 1. Product Goal

- REQ-001: The product shall be a local-first AI learning workspace.
- REQ-002: The product shall support learning from user-provided documents, chat history, web sources, and generated reports.
- REQ-003: The product shall prioritize grounded learning workflows over general-purpose chat.
- REQ-004: The product shall keep agent runtime responsibilities in `llm-harness-runtime` / `llm-harness-agent` wherever possible.
- REQ-005: The product shall keep `llm-tutor` focused on product data, UI,
  knowledge bases, Notebook, quizzes, reports, settings, and runtime-session
  mappings.

## 2. Architecture

- REQ-010: The system shall use a Rust workspace backend.
- REQ-011: The system shall expose a React web UI.
- REQ-012: The backend shall be split into `tutor-agent`, `tutor-tools`, `tutor-rag`, and `tutor-web`.
- REQ-013: `tutor-agent` shall own capability routing and agent prompts.
- REQ-014: `tutor-tools` shall own runtime tool implementations.
- REQ-015: `tutor-rag` shall own document indexing, embeddings, vector storage, and retrieval.
- REQ-016: `tutor-web` shall own HTTP APIs, WebSocket streaming, product stores, and session mapping.
- REQ-017: The web UI shall own product interaction, layout, settings, and rendering.
- REQ-018: The backend shall use runtime sessions for durable conversation history.
- REQ-019: The backend shall avoid building a parallel session/context/orchestration system when runtime APIs are available.

## 3. Session and Conversation

- REQ-030: Users shall be able to create a new chat session.
- REQ-031: Users shall be able to resume an existing chat session.
- REQ-032: Users shall be able to rename a session.
- REQ-033: Users shall be able to delete a session.
- REQ-034: Sessions shall persist user and assistant messages.
- REQ-035: Sessions shall persist selected capability.
- REQ-036: Sessions shall persist selected knowledge base.
- REQ-037: Sessions shall persist selected LLM config.
- REQ-038: Sessions shall persist selected search config.
- REQ-039: Sessions shall expose context usage information when available.
- REQ-040: Sessions shall support runtime compaction summaries.
- REQ-041: The UI shall show recent sessions in the sidebar.
- REQ-042: The chat view shall support streaming assistant output.
- REQ-043: The chat input shall remain usable while an agent is running when product behavior allows it.
- REQ-044: The system shall provide a way to stop or cancel a running agent turn. Status: planned.

## 4. Capability Modes

- REQ-050: The composer shall allow selecting a capability mode.
- REQ-051: The system shall support `chat` mode.
- REQ-052: The former `deep_solve` mode is retired. Complex problem solving
  shall remain a normal Chat/Tutor behavior with on-demand RAG, web, and code
  tools. Historical Deep Solve traces may remain read-only.
- REQ-053: The system shall support `code_exec` mode.
- REQ-054: The system shall support `quiz` mode.
- REQ-055: The system shall support `research` mode.
- REQ-056: Capability changes shall not destroy the current runtime session.
- REQ-057: Capability-specific prompts shall be owned by `tutor-agent`.
- REQ-058: Capability-specific product UI shall be owned by `web-ui`.

## 4A. Persistent Tutors

- REQ-060: The Tutor surface shall manage persistent tutor entities rather
  than act as another capability mode. Status: implemented.
- REQ-061: A tutor shall own a user-editable Markdown Soul, default model,
  allowed capabilities, resource permissions, conversation collection, and
  private Tutor Memory. Soul defines stable identity and teaching behavior;
  changing learning goals belong to Tutor Memory. Status: implemented except
  for the tutor conversation collection and stronger autonomous-write policy.
- REQ-062: The new-conversation empty state shall let the user optionally choose
  a tutor before the first message. The chooser shall remain a fixed-height
  control and open a bounded overlay list; increasing the tutor count shall not
  resize the composer or empty-state layout. Status: implemented.
- REQ-063: When no tutor is selected, the product shall use Temporary Assistant
  for a one-off conversation without persistent identity or private Tutor
  Memory. Temporary Assistant need not appear as a duplicate tutor-list item.
  Status: implemented.
- REQ-064: Tutor identity and capability mode shall remain separate concepts:
  the tutor is who accompanies the learner, while Chat, Research, and Quiz
  describe what it is doing. Complex solving belongs to Chat rather than a
  separate capability. Status: implemented.
- REQ-065: Tutor-bound sessions shall persist an immutable `tutor_id` beside
  the runtime session mapping. Changing tutors shall create a new session or a
  bounded handoff rather than replacing identity in place. Status: implemented
  for immutable binding and new-session creation; handoff remains planned.
- REQ-066: Authorized tutors may read shared Learner Memory, but each tutor's
  commitments, open loops, lesson plans, reflections, and strategy shall remain
  private by default. Status: implemented.
- REQ-067: Tutor Memory shall be visible, editable, removable, resettable,
  source-linked, and lifecycle-aware. It shall not duplicate the complete
  learner profile or store sensitive data and external factual claims.
  Status: implemented for storage, lifecycle, source session, and management;
  hard content-policy validation remains in progress.
- REQ-068: Tutor context shall combine Soul and permissions, relevant Learner
  Memory, relevant private Tutor Memory, runtime session history, and current
  resources through thin mappings to runtime APIs. Status: implemented for
  Soul, runtime history, model defaults, permission-filtered current resources,
  and bounded active Tutor Memory.
- REQ-068A: Soul Markdown shall be injected only as bounded product-owned
  runtime instruction. It shall not be parsed to grant capabilities, tools, or
  resource access and cannot override safety policy. Status: implemented.
- REQ-069: User-created tutors shall expose a visible delete action. Deletion
  is an archive operation: the tutor immediately disappears from active
  management and new-conversation selection, while existing bound sessions and
  their history remain intact. The built-in tutor cannot be deleted. Deleting
  or resetting a tutor shall not implicitly delete global Learner Memory,
  Notebook, Knowledge, Quiz, or Space assets. Status: implemented.
- REQ-069A: The detailed product contract and MVP sequence shall follow
  `docs/specs/2026-07-15-persistent-tutor-design.md`. Status: active.

## 5. Chat Mode

- REQ-070: Chat mode shall answer open-ended user questions.
- REQ-071: Chat mode shall use runtime `knowledge_search` and `knowledge_read`
  for selected Knowledge Base material. Status: implemented.
- REQ-072: Chat mode shall be able to use `web_search`.
- REQ-073: Chat mode shall be able to use `web_fetch`.
- REQ-074: Chat mode shall be able to use `code_exec`.
- REQ-075: Chat mode shall require web search for current, external, or fact-collection requests.
- REQ-076: Chat mode shall avoid inventing facts when web search or fetch fails.
- REQ-077: Chat mode shall display citations only when the agent actually used a citation-producing tool.
- REQ-078: Chat mode shall reject missing, forged, or cross-run Knowledge
  citation handles after trusted evidence is read. Status: implemented through
  the runtime final-answer validator.
- REQ-079: Chat mode shall support user-selected `@` references to Space artifacts. Status: implemented.
- REQ-080: `@` references shall be stored as structured artifact references, not only as plain text mentions. Status: implemented.
- REQ-081: The first supported `@` reference targets shall include Notebook entries, Quiz sessions, and Quiz questions. Status: implemented.
- REQ-082: The chat composer shall display selected `@` references as removable chips. Status: implemented.
- REQ-083: The `@` reference picker shall search Space artifacts by title, type, and useful metadata. Status: implemented.
- REQ-084: Mentioned Space artifact IDs shall be persisted with the user message and restored with the session. Status: implemented.
- REQ-085: Agents shall access mentioned Space artifacts through product tools such as `read_space_item` where possible, instead of blindly injecting long artifact content into every prompt. Status: implemented for Chat and Research.
- REQ-086: When an agent uses mentioned Space artifact content in an answer, the response shall identify the relevant artifact or cite it through the product reference system. Status: planned.
- REQ-087: Ordinary Chat messages shall expose role-appropriate actions through
  a compact message action toolbar that appears on pointer hover or keyboard
  focus without covering message content or shifting conversation layout.
  Assistant actions shall include Copy, Quote, Save to Notebook, and Regenerate
  where supported; user actions shall include Copy, Quote, and Edit. Quoting a
  message shall add a structured or clearly delimited reference to the composer.
  Status: implemented for Copy, Quote, Research-mode Save to Notebook,
  Regenerate, Sources, and user Edit actions.
- REQ-088: Ordinary assistant text shall use a transparent, document-like
  presentation that blends into the Chat background and may use the full Chat
  content lane. Ordinary user messages shall remain visually distinct as
  right-aligned light-gray bubbles with a bounded maximum width. Status: implemented.
- REQ-089: Structured product results such as Research reports, Quiz cards,
  approval requests, Notebook edit proposals, and workflow status surfaces
  shall retain purpose-built card or status presentation instead of inheriting
  ordinary message bubble styling. Citation sources shall remain a separate,
  expandable surface reached through a compact `Sources N` message action when
  appropriate. Status: implemented; structured results retain their dedicated
  components and ordinary citation lists remain separate from the toolbar.

## 6. Attachments

- REQ-090: The chat composer shall allow selecting file attachments.
- REQ-091: The chat composer shall display selected attachments as removable chips.
- REQ-092: Text attachments shall be included as current-turn context.
- REQ-093: PDF attachments shall be parsed server-side and included as current-turn context.
- REQ-094: Attachment parsing failures shall show the failure reason.
- REQ-095: Attachment text shall be truncated to protect context capacity.
- REQ-096: Unsupported attachment types shall fail clearly instead of being silently ignored.
- REQ-097: Attachment content sent to the model shall not visually flood the user message bubble.

## 7. Knowledge Base and RAG

- REQ-110: Users shall be able to create a knowledge base.
- REQ-111: Users shall be able to name a knowledge base.
- REQ-112: Users shall choose embedding configuration when creating a knowledge base.
- REQ-113: Users shall be able to upload documents into a knowledge base.
- REQ-114: The system shall parse UTF-8 text files.
- REQ-115: The system shall parse Markdown files.
- REQ-116: The system shall parse PDF text.
- REQ-117: The system shall show upload and ingestion progress.
- REQ-118: Ingestion progress shall eventually include parse, chunk, embed, and vector-write stages.
- REQ-119: The system shall chunk documents before indexing.
- REQ-120: Chunk IDs shall be stable enough to support citation and document management.
- REQ-121: The system shall store vectors in LanceDB.
- REQ-122: The system shall store document metadata.
- REQ-123: The system shall support searching a knowledge base.
- REQ-124: Runtime `knowledge_search` shall return lightweight refs/snippets and
  `knowledge_read` shall return bounded source bodies plus verified evidence.
  Status: implemented.
- REQ-125: Answers using Knowledge evidence shall show runtime-validated
  citation sources. Status: implemented.
- REQ-126: Users shall be able to view document chunks.
- REQ-127: Users shall be able to delete documents.
- REQ-128: Users shall be able to reindex documents.
- REQ-129: Users shall be able to preview uploaded documents.
- REQ-130: PDF preview shall render correctly. Status: needs hardening.
- REQ-131: Chunking shall be upgraded from basic splitting to paragraph/token-aware splitting. Status: planned.
- REQ-132: Retrieval quality shall support reranking or improved scoring. Status: planned.
- REQ-133: Citations shall support better location metadata such as page, paragraph, or chunk highlight. Status: planned.
- REQ-134: Web pages and fetched sources shall be ingestible into a knowledge base. Status: implemented for Research report sources through the existing Knowledge Base document ingestion path.

## 8. Embedding Configuration

- REQ-150: Users shall be able to add embedding provider configurations.
- REQ-151: Embedding configs shall include endpoint URL.
- REQ-152: Embedding configs shall include API key.
- REQ-153: Embedding configs shall include model ID.
- REQ-154: Embedding configs shall include vector dimensions.
- REQ-155: Embedding configs shall support sending or omitting the `dimensions` parameter.
- REQ-156: Knowledge bases shall store the embedding config they were created with.
- REQ-157: The UI shall warn when a knowledge base embedding config is missing or invalid. Status: planned.
- REQ-158: The system shall provide embedding provider health checks. Status: planned.

## 9. LLM Configuration

- REQ-170: Users shall be able to add LLM configurations.
- REQ-171: LLM configs shall support OpenAI-compatible APIs.
- REQ-172: LLM configs shall support Anthropic Messages APIs.
- REQ-173: LLM configs shall include base URL.
- REQ-174: LLM configs shall include API key.
- REQ-175: LLM configs shall include model ID.
- REQ-176: LLM configs shall include optional chat path.
- REQ-177: LLM configs shall include context window tokens.
- REQ-178: The composer shall allow selecting among configured LLM models.
- REQ-179: LLM model selection shall not require binding the model permanently to a session unless product behavior explicitly chooses to.
- REQ-180: The UI shall provide LLM provider health checks. Status: planned.
- REQ-181: The UI shall describe model capabilities such as tools, streaming, JSON schema, vision, and context window. Status: planned.
- REQ-182: API keys shall eventually be stored more securely than plain local config. Status: planned.

## 10. Web Search and Web Fetch

- REQ-200: The system shall provide a `web_search` tool.
- REQ-201: The system shall provide a `web_fetch` tool.
- REQ-202: Search provider configuration shall be editable in Settings.
- REQ-203: Search providers shall support URL and API key fields.
- REQ-204: Search providers shall support free and paid providers where possible.
- REQ-205: Search providers shall include Bing support.
- REQ-206: Search providers shall include Brave support.
- REQ-207: Search providers shall include Tavily support.
- REQ-208: Search providers shall include Serper support.
- REQ-209: Search providers shall include SerpAPI support.
- REQ-210: Search providers shall include Exa support.
- REQ-211: DuckDuckGo support shall avoid relying on low-quality Instant Answer API behavior.
- REQ-212: `web_fetch` shall extract readable page text.
- REQ-213: Search and fetch failures shall surface actionable reasons.
- REQ-214: Search results shall be deduplicated. Status: planned.
- REQ-215: Search results shall support source quality scoring. Status: planned.
- REQ-216: The system shall support academic search or scholar search. Status: planned.
- REQ-217: The system shall support news/current-events search. Status: planned.
- REQ-218: The system shall support site-restricted search. Status: planned.

## 11. Complex Problem Solving

- REQ-230: Complex questions shall be handled through ordinary Chat/Tutor
  conversation rather than a mandatory fixed workflow.
- REQ-231: The Agent shall clarify ambiguous questions before starting costly
  retrieval or computation when clarification materially affects the answer.
- REQ-232: The Agent may use RAG, web search, web fetch, and code execution on
  demand without changing the conversation into a separate mode.
- REQ-233: Answers shall expose citations and tool evidence when those tools
  materially support the result.
- REQ-234: Complex answers shall use the same streaming, persistence, session
  switching, and follow-up behavior as ordinary Chat.
- REQ-235: The retired Deep Solve workflow, route protocol, and new-run UI
  entry shall not remain active.
- REQ-236: Historical Deep Solve messages and traces may remain readable, but
  reopening them shall continue the session in Chat mode.

## 12. Code Execution

- REQ-250: The system shall provide a `code_exec` tool.
- REQ-251: Code execution shall support Python.
- REQ-252: Code execution shall support shell where allowed by runtime environment.
- REQ-253: Code execution shall be used for non-trivial numeric verification.
- REQ-254: Code execution errors shall be returned clearly.
- REQ-255: Code execution shall respect approval requirements when configured.
- REQ-256: Code execution shall rely on runtime/environment abstractions rather than product-specific execution loops.
- REQ-257: Stronger sandboxing shall be added before untrusted multi-user execution. Status: planned.

## 13. Quiz

- REQ-270: Users shall be able to generate a Quiz from a selected knowledge base.
- REQ-271: Users shall be able to generate a Quiz from conversation material.
- REQ-272: Users shall generate Quiz content from Chat using conversation context, attachments, selected knowledge sources, or user-referenced saved material; Notebook and Research report pages shall not provide independent Quiz generation entry points. Status: implemented for conversation, attachments, knowledge bases, and `@` Space references.
- REQ-273: Quiz generation shall retrieve source chunks when a knowledge base is selected.
- REQ-274: Quiz generation shall produce strict structured JSON.
- REQ-275: Quiz questions shall include answer options.
- REQ-276: Quiz questions shall include the correct answer.
- REQ-277: Quiz questions shall include explanation.
- REQ-278: Quiz questions shall include citations where source material exists.
- REQ-279: Users shall be able to answer Quiz questions.
- REQ-280: Users shall get immediate correctness feedback.
- REQ-281: Users shall see final score summary.
- REQ-282: Users shall see missed-question review.
- REQ-283: Quiz answer and explanation consistency shall be validated. Status: partially implemented with deterministic schema/citation checks; needs LLM verifier hardening.
- REQ-284: Quiz shall support multi-select questions. Status: planned.
- REQ-285: Quiz shall support short-answer judging. Status: planned.
- REQ-286: Quiz shall support wrong-answer review records. Status: planned.
- REQ-287: Quiz shall support adaptive difficulty. Status: planned.
- REQ-288: Quiz shall be exposed to Chat as an enabled agent capability/tool, not as an automatic "send equals generate" mode. Status: implemented with `propose_quiz_plan` and `create_quiz` product tools.
- REQ-289: When Quiz capability is enabled, normal chat turns shall allow discussion of scope, source material, difficulty, question type, learner level, citation strictness, and review goals before any Quiz is generated. Status: implemented through normal Chat plus optional `propose_quiz_plan`.
- REQ-290: The agent shall call a dedicated `create_quiz` product tool only after the user explicitly asks to generate, confirms a plan, or provides an unambiguous generation request. Status: implemented in tool design/prompt contract; needs behavioral QA across providers.
- REQ-291: Quiz generation shall separate user instruction from source material. The latest user message may guide topic and constraints, but it shall not be treated as factual source material unless no other source is available or the user explicitly says it is source material. Status: partially implemented through `kb_id`, `notebook_entry_id`, `source_text`, and `source_label`; needs stronger UI/agent guardrails.
- REQ-292: A successful `create_quiz` tool call shall save the Quiz into Quiz Bank and render an interactive Quiz card in Chat. Status: implemented.
- REQ-293: Quiz planning may use learner memory for personalization, but generated factual answers and citations shall remain grounded in selected source material. Status: implemented for first slice; L3 memory is passed only for personalization when review/practice intent is detected.
- REQ-294: Quiz generation shall include a verifier stage inside the controlled product flow, not as a separate free-form chat agent. Status: implemented with deterministic validation plus a structured LLM verifier running inside the runtime `WorkflowEngine`.
- REQ-295: The verifier stage shall receive only the source chunks, candidate question JSON, answer, explanation, citations, and supporting quote needed for review. Status: implemented.
- REQ-296: The verifier stage shall return structured review output such as `verdict`, `issues`, `supported_answer`, `explanation_consistent`, `citation_supports_answer`, and optional repair guidance. Status: partially implemented with `verdict` and `issues`; richer booleans and repair guidance are planned.
- REQ-297: The verifier shall judge only against supplied source material and shall not introduce external knowledge or new factual claims. Status: implemented in prompt contract; needs provider-behavior QA.
- REQ-298: Questions that fail deterministic validation or verifier review shall not be saved as final Quiz questions unless they are repaired and re-verified. Status: implemented for first slice through runtime workflow repair and final publish validation.
- REQ-299: The first verifier implementation may retry or repair failed questions once, then discard unresolved questions rather than publishing weakly grounded content. Status: implemented for first slice with a bounded runtime workflow repair loop.
- REQ-299A: Interactive Quiz cards rendered inside Chat shall be durable message attachments linked to the saved Quiz session, not only transient WebSocket/UI state. Leaving and returning to the session shall restore the card and keep it answerable. Status: implemented.
- REQ-299B: If a Quiz is generated while the user switches to another session, the completed `create_quiz` tool result shall remain attached to the originating assistant message and be restored when the user returns. Status: implemented.

Current Quiz verification flow:

1. `create_quiz` builds source material from one of these sources: selected knowledge base chunks, a Notebook entry, explicit `source_text`, or selected Chat/Space material.
2. If a usable LLM config exists, the quiz runtime workflow definition is validated before generation: `collect_sources -> generate_questions -> verify_questions -> publish_questions`, with a bounded repair edge from verifier back to generation.
3. `tutor_agent::quiz::generate_quiz_questions_with_workflow` runs the controlled quiz flow through runtime `WorkflowEngine`.
4. The `generate_questions` runtime LLM step returns single-choice question JSON as its structured final assistant output.
5. The `verify_questions` runtime LLM step reviews generated questions against the supplied source chunks and returns a structured pass/fail final assistant output.
6. Failed verifier results route back to `generate_questions` once for repair; unresolved failures do not reach final publish.
7. The `publish_questions` executor deterministically validates JSON shape, option consistency, citations, and supporting quotes before storage.
8. If no usable LLM config exists, the backend uses a deterministic fallback question generator from source chunks.
9. The backend converts verified questions into stored `QuizQuestion` records and maps cited source indices to `QuizCitation` metadata.
10. `validate_quiz_questions_for_storage` rejects empty question sets, empty stems, too-few options, missing correct option IDs, empty explanations, missing citations, and empty citation text.
11. The stored quiz receives a `QuizVerificationReport` with method `llm_verifier_and_citation_check` or `deterministic_fallback_citation_check`.

Remaining hardening: richer verifier booleans and repair guidance can be added later, but the runtime `WorkflowEngine` execution and bounded repair/retry loop are now implemented.

## 14. Research

- REQ-300: Users shall be able to select Research mode in chat.
- REQ-301: Research mode shall create a brief research plan.
- REQ-302: Research mode shall call `web_search` for external facts.
- REQ-303: Research mode shall call `web_fetch` for important sources.
- REQ-304: Research mode shall optionally use runtime `knowledge_search` /
  `knowledge_read` when a Knowledge Base is selected, and the final report step
  shall cite fresh run-local handles. Status: implemented.
- REQ-305: Research mode shall produce a Markdown report.
- REQ-306: Research reports shall include a structured title. The explicit workflow title is authoritative; the first Markdown heading and then the research request are fallback sources. Status: implemented.
- REQ-307: Research reports shall include a summary.
- REQ-308: Research reports shall include key findings.
- REQ-309: Research reports shall include analysis.
- REQ-310: Research reports shall include limitations.
- REQ-311: Research reports shall include follow-up questions.
- REQ-312: Research reports shall include sources.
- REQ-313: Research progress shall appear as compact status, not normal assistant bubbles.
- REQ-314: Research trace events shall include planning, search, read, and report completion.
- REQ-315: Research mode shall clearly state when search or fetch failed.
- REQ-316: Research mode shall not cite sources that were not actually searched or fetched.
- REQ-317: Research reports shall be saved as `NotebookEntry` records with `type = research_report`. Status: implemented.
- REQ-318: Research reports shall restore with structured metadata from Notebook entries after save and from durable runtime trace attachments before save. Status: implemented for report title, Markdown, source metadata, and unavailable-artifact fallback.
- REQ-319: The UI shall provide a dedicated ResearchReport component. Status: implemented.
- REQ-320: The UI shall show a dedicated source list under each Research report. Status: implemented.
- REQ-321: Research reports shall support regeneration/versioning. Status: implemented for the first slice with a report Regenerate action and Notebook report metadata carrying version and generation details.
- REQ-322: Research shall support longer-running multi-step/parallel deep research. Status: implemented for the first slice by allowing the runtime Research workflow search step to spawn independent subtopic agents through `spawn_agent`.
- REQ-323: Research mode shall preserve normal conversational interaction for clarifying research goals, scope, source preferences, output format, depth, time range, and optional Notebook or Knowledge Base context before starting detailed research. Ordinary Research Chat turns shall stream like normal Chat until a report-generation tool call starts. Status: implemented with Research TextDelta routed through the normal final-answer stream before the `create_research_report` tool boundary.
- REQ-324: Research mode shall provide a detailed research workflow that can be explicitly started after the user's need is clear. The workflow shall cover search, source reading, source selection, synthesis, citation checking, and report generation; it shall not be forced for every Research message. The target architecture shall align with Quiz generation: the outer Research Chat agent calls a dedicated `create_research_report` product tool, and that tool runs the runtime `WorkflowEngine` and returns structured report metadata. Status: implemented for the first tool-boundary slice with the previous keyword/confirmation pre-router removed from the Research capability entrypoint.
- REQ-325: Long-running Research runs shall have a durable run identity and status so users can switch sessions, return later, and see the current stage or final report without starting a duplicate workflow. Status: implemented for in-process rejoin; after process restart an active run restores as interrupted pending runtime resume support.
- REQ-326: Research progress and final report attachments shall be restorable from runtime session state and product records after navigation, refresh, or desktop restart. Status: implemented for current stage, terminal state, report metadata, and report attachment; full progress replay and execution resume remain pending on runtime support.

## 14A. Background Runs and Session Rejoin

- REQ-330: Long-running agent turns shall continue when the user leaves the
  current session unless the user explicitly cancels them or the backend fails.
  Status: implemented for in-process runs; process loss restores an explicit
  interrupted state because the runtime cannot yet resume execution.
- REQ-331: The UI shall be able to rejoin an active run by stable run/session
  identifiers and show queued, running, waiting, failed, cancelled, or completed
  state. Status: implemented for in-process rejoin and persisted terminal or
  interrupted state after restart.
- REQ-332: Chat messages shall persist tool result attachments and stable
  references to product artifacts such as Quiz sessions and Notebook research
  reports. Status: implemented for Quiz session references and Research runtime
  trace references, with Notebook references after explicit save.
- REQ-333: Reconnecting to a session shall not start a duplicate workflow for an
  already-active run. Status: implemented for in-process runs.
- REQ-334: The implementation shall prefer runtime session/run persistence from
  `llm-harness-runtime` / `llm-harness-agent`; missing framework primitives
  shall be recorded in `docs/framework-feedback.md`. Status: implemented; the
  durable resume/replay gap is recorded there.
- REQ-335: Leaving and returning to an in-process running session shall restore
  the assistant text generated so far and continue streaming from the same run.
  Snapshot capture and live subscription shall not lose deltas between them.
  Status: implemented.
- REQ-336: WebSocket events and asynchronous session hydration shall be scoped
  to their originating session. Rapid session switching shall not apply stale
  content, completion, close, or HTTP responses to the newly selected session.
  Status: implemented.
- REQ-337: While any session is marked active in the sidebar, the UI shall
  periodically reconcile run state with the backend so a completion event missed
  during navigation cannot leave a permanent running indicator. Status:
  implemented.

## 15. Retired Books Capability

Product decision recorded on 2026-07-14: Books are no longer a target product
capability. Notebook is the single durable destination for research reports and
other generated learning records.

- REQ-340 through REQ-357: The former Book viewing, creation, chapter,
  conversion, editing, export, and retrieval requirements are retired and
  shall not drive future implementation.
- REQ-358: Product navigation and report/Notebook actions shall not expose Book
  creation, chapter creation, or save-to-Book workflows. Book UI, routes,
  stores, source targets, and compatibility code shall be removed. Status:
  implemented.
- REQ-359: Books retirement does not require migration or compatibility for
  previously stored Book data. Obsolete Book data may be deleted with the
  retired storage implementation. Status: implemented.

## 16. Space Workspace

- REQ-600: Space shall be the project-level learning workspace.
- REQ-601: Space shall not be equivalent to Notebook.
- REQ-602: Space shall initially contain Notebook, Quiz Bank, and Student Profile modules.
- REQ-603: The product shall support one default Space before requiring multi-space management.
- REQ-604: Space shall eventually support multiple named learning/research spaces. Status: planned.
- REQ-605: Space shall organize durable learning artifacts and learning state.
- REQ-606: Chat shall remain the primary generation surface.
- REQ-607: Space shall remain the primary review and organization surface.
- REQ-608: Knowledge Base shall remain the primary original-source and retrieval surface.

## 17. Notebook

- REQ-620: Notebook shall be a module inside Space.
- REQ-621: Notebook shall store flexible learning records.
- REQ-622: Notebook entries shall support ordinary notes.
- REQ-623: Notebook entries shall support research reports.
- REQ-624: Notebook entries shall support chat answer excerpts.
- REQ-625: Notebook entries shall support source snippets.
- REQ-626: Notebook entries shall support quiz summaries.
- REQ-627: Notebook shall preserve historical Deep Solve result entries as
  read-only legacy artifacts; new complex answers use normal Chat save flows.
- REQ-628: Research reports shall be represented as Notebook entries with `type = research_report`.
- REQ-629: Notebook entries shall store Markdown content.
- REQ-630: Notebook entries shall store metadata for source session, source message, query, sources, or generated-by fields where available.
- REQ-631: Users shall be able to list Notebook entries. Status: planned.
- REQ-632: Users shall be able to open Notebook entries. Status: planned.
- REQ-633: Users shall be able to create Notebook entries manually. Status: planned.
- REQ-634: Users shall be able to edit Notebook entries. Status: planned.
- REQ-635: Users shall be able to delete Notebook entries. Status: planned.
- REQ-636: Research reports shall be saved to Notebook; no secondary Book
  destination shall be offered. Status: implemented.
- REQ-637: The former send-to-Books requirement is retired.
- REQ-638: Notebook entries shall remain a Markdown/plain-text workspace and shall not be indexed into RAG or vector stores. Status: planned.
- REQ-639: Users shall be able to `@` a Notebook entry in Chat and ask the agent to revise, expand, summarize, or reorganize it. Status: implemented.
- REQ-640: Agent-produced Notebook edits shall be previewed as a proposed Markdown replacement or diff before they are applied. Status: implemented as complete Markdown replacement preview.
- REQ-641: Applying an agent-produced Notebook edit shall require explicit user confirmation. Status: implemented.
- REQ-642: Applied agent-produced Notebook edits shall update entry metadata and create a Notebook memory event. Status: implemented.
- REQ-643: Direct Notebook write tools shall be gated by explicit user instruction and product approval flow, not by implicit model choice alone. Status: implemented by exposing only a preview tool to the agent; writes happen through user-confirmed product UI.
- REQ-780: Notebook shall evolve toward a Markdown-first, Obsidian-like knowledge workspace with connected notes. Status: planned.
- REQ-781: Notebook shall support wiki-style note links such as `[[Note Title]]` and `[[note-id|alias]]`. Status: planned.
- REQ-782: Notebook shall parse and display backlinks for notes that reference the current note. Status: planned.
- REQ-783: Notebook shall parse and filter tags from note Markdown and note metadata. Status: planned.
- REQ-784: Notebook shall support importing Markdown notes from files, folders, or zip bundles. Status: planned.
- REQ-785: Notebook import shall preserve Markdown content, frontmatter, unknown metadata, and wiki links where possible. Status: planned.
- REQ-786: Notebook shall support exporting one note, selected notes, or the full Notebook as Markdown files or a zip bundle. Status: planned.
- REQ-787: Notebook export shall include stable frontmatter for title, id, type, tags, source metadata, created time, and updated time. Status: planned.
- REQ-788: Notebook import/export shall be local and portable; exported notes shall remain readable outside this app. Status: planned.
- REQ-789: Notebook shall support agent-assisted organization proposals such as suggested links, tags, and duplicate-note merges, with explicit user confirmation before writes. Status: planned.
- REQ-790: Agent-assisted Notebook organization shall be primarily triggered from Chat, not from independent generation buttons inside Notebook. Status: planned.
- REQ-791: When users ask Chat to organize a mentioned Notebook entry, the agent shall read the exact entry first and then produce a reviewable proposal. Status: planned.
- REQ-792: Notebook organization proposals shall include enough structure to distinguish complete Markdown edits, tag suggestions, wiki-link suggestions, and duplicate-note merge suggestions. Status: planned.
- REQ-793: Users shall be able to apply or reject Notebook organization proposals from Chat. Status: planned.
- REQ-794: If users ask about their notes without explicitly using `@`, the agent may search Notebook for candidate entries before answering. Status: planned.
- REQ-795: Notebook search for unmentioned questions shall return candidate entry ids, titles, types, tags, snippets, and confidence or ranking metadata. Status: planned.
- REQ-796: If Notebook search finds high-confidence candidates, the agent shall read the relevant entries before answering and cite the used Notebook entries. Status: planned.
- REQ-797: If Notebook search is ambiguous, the agent shall ask the user to choose candidate notes or present the candidate list before making strong claims. Status: planned.
- REQ-798: If users ask to edit, tag, link, or merge Notebook content without `@`, the agent shall search first and ask for target confirmation before creating a write proposal. Status: planned.
- REQ-799: If Notebook search finds no relevant entries, the agent shall say no relevant Notebook content was found before falling back to general knowledge or other tools. Status: planned.
- REQ-800: Chat composer shall expose Notebook association through the same source selector as Knowledge Base association. Status: planned.
- REQ-801: The shared source selector shall support at least No source, Notebook, and one concrete Knowledge Base. Status: planned.
- REQ-802: Notebook association shall use only plain-text search/read over Notebook Markdown; it shall not use embeddings, LanceDB, or vector indexing. Status: planned.
- REQ-803: Notebook association shall be modeled separately from Knowledge Base ids; `kb` shall only identify real Knowledge Bases. Status: planned.
- REQ-804: The UI shall prevent selecting Notebook and Knowledge Base simultaneously from the shared source selector until a multi-source design exists. Status: planned.
- REQ-805: Notebook search results used by the agent shall include navigable Notebook source ids. Status: planned.
- REQ-806: Chat shall add an Organize mode for Notebook and Space organization workflows. Status: planned.
- REQ-807: Organize mode shall enable Notebook search/read and proposal-first organization tools by default. Status: planned.
- REQ-808: Code execution shall be treated as a tool available to suitable modes rather than a standalone user-facing Chat mode. Status: planned.
- REQ-809: Notebook import shall show skipped-file details after the final import, not only during preview. Each skipped item shall include the original file name or zip path and a human-readable reason. Status: implemented.
- REQ-810: Notebook zip or folder import shall create entries in a batch and persist the Notebook store once per import operation rather than rewriting the full store after each entry. Status: implemented.
- REQ-811: The desktop app shall support importing an Obsidian Vault by selecting a local folder through a native directory picker and recursively reading `.md` and `.markdown` files. Status: implemented.
- REQ-812: Notebook import shall explicitly report Obsidian attachments and embedded assets that are not imported yet, including images and other non-Markdown files referenced by Markdown. Status: implemented.
- REQ-813: Notebook persistence shall use a file-backed vault directory where note bodies are stored as individual Markdown files. Status: implemented.
- REQ-814: Notebook shall keep a lightweight index file for stable ids, relative paths, entry types, source mappings, timestamps, and product metadata that should not be stored only in Markdown body text. Status: implemented.
- REQ-815: Notebook import from Markdown or Obsidian Vault folders shall prefer the source file name stem as the entry title, while preserving `frontmatter.title` as metadata, subtitle, or alias. Status: implemented.
- REQ-816: Notebook file-backed storage shall preserve imported relative folder paths where possible. Status: implemented.
- REQ-817: Notebook create, edit, rename, delete, import, export, and agent-applied edit flows shall update Markdown files and the Notebook index consistently. Status: partially implemented; create, edit, delete, import, export, and agent-applied edits are covered, while explicit path rename/link-update workflows remain planned.
- REQ-818: Agent autonomous Notebook/Vault exploration shall be available only when the chat source selector is associated with Notebook. Status: planned.
- REQ-819: When Notebook is not associated and the user has not explicitly `@` mentioned a Notebook entry, the agent shall not silently search or read Notebook content. Status: planned.
- REQ-820: Explicit `@` Notebook references shall still allow reading the referenced entry through `read_space_item`, even when Notebook is not the associated source. Status: planned.
- REQ-821: Notebook/Vault maintenance capabilities such as edit proposals, link suggestions, tag cleanup, move/rename proposals, duplicate review, merge proposals, and new-note proposals shall be enabled only in Organize mode. Status: planned.
- REQ-822: Non-Organize modes may answer from explicitly referenced or associated Notebook content, but shall not initiate Notebook maintenance proposals unless the session is switched to Organize mode. Status: planned.
- REQ-823: Notebook exploration tools shall be vault-aware and return stable ids together with paths, titles, entry types, tags, snippets, and link/backlink metadata where available. Status: planned.
- REQ-824: Notebook maintenance tools shall remain proposal-only; applying changes shall require user confirmation and shall execute through product Notebook APIs, not direct agent file-system writes. Status: planned.
- REQ-825: Notebook file browsing shall use lightweight tree/list metadata and shall not return full Markdown bodies in the file browser payload. Status: implemented.
- REQ-826: Notebook shall load full Markdown content only when a note is opened or explicitly read by an agent/product tool. Status: implemented.
- REQ-827: Notebook shall avoid a blocking full Vault rescan every time the Notebook tab is opened or revisited. Status: implemented.
- REQ-828: Notebook shall provide an explicit Vault refresh/reconcile operation for users to force external file changes to be indexed. Status: implemented.
- REQ-829: Notebook indexing shall track file stats such as relative path, modified time, and size so unchanged files can be skipped during refresh. Status: implemented.
- REQ-830: The desktop app shall use a file watcher for bound Notebook Vault directories where possible, with debounced index updates. Status: implemented.
- REQ-831: Notebook shall keep folder expansion state and selected note state stable across tab switches and non-destructive refreshes. Status: implemented.
- REQ-832: Notebook shall show indexing status such as watching, refreshing, last refreshed time, and changed-file count when available. Status: implemented.
- REQ-833: Notebook file tree rendering shall be designed to support large Vaults, including lazy folder expansion or virtualization when needed. Status: implemented with virtualized visible-row rendering.
- REQ-834: Notebook shall use an editor-style data model where the Vault files, Notebook index, file watcher, frontend explorer, open note buffer, and relation panels are separate responsibilities. Status: planned.
- REQ-835: Opening the Notebook page shall render from the current lightweight index first; full Markdown bodies and selected-note relations shall load only after a note is opened. Status: implemented for note body loading; relation-panel loading remains planned.
- REQ-836: Notebook create-note and create-folder actions shall be scoped to the currently selected folder or an explicit target folder. Status: planned.
- REQ-837: Notebook rename, move, and delete actions shall update the Markdown file path and Notebook index together and shall report affected links when link rewriting is not performed automatically. Status: planned.
- REQ-838: Notebook shall render Obsidian-style `[[wiki links]]` as navigable internal links and shall provide a clear unresolved-link path such as creating the missing target note. Status: partially implemented; link parsing and basic navigation exist, unresolved-link creation remains planned.
- REQ-839: Notebook shall provide a collapsible selected-note information panel for backlinks, outgoing links, tags, source metadata, and local graph. Status: partially implemented; panel and collapse behavior exist, richer relation detail remains planned.
- REQ-840: Notebook shall preserve user Markdown as much as possible, including unknown frontmatter, comments, aliases, and Obsidian-compatible syntax, unless the user explicitly applies a normalization or organization proposal. Status: planned.
- REQ-841: Saving generated Chat or Research content to Notebook shall ask the user to choose a Notebook-relative destination before creating the entry. Status: implemented.
- REQ-842: When Notebook uses app-owned storage, the destination chooser shall be an app-owned Notebook folder tree rather than an operating-system file picker, because Notebook folders are product-owned logical paths. Status: implemented.
- REQ-843: The Notebook folder tree chooser shall support expanding and collapsing folders, selecting the Notebook root or an existing folder, and creating a new folder without leaving the save flow. Status: implemented.
- REQ-844: The save flow shall display the resulting Notebook-relative folder, file name, and full logical path before confirmation, and shall prevent invalid or conflicting paths. Status: implemented.
- REQ-845: The save flow should remember the most recently selected Notebook folder and use it as the next default when that folder still exists; otherwise it shall fall back to the Notebook root. Status: implemented.
- REQ-846: After a successful save, the UI shall confirm the destination and offer a direct action to open the created Notebook entry. Status: implemented.
- REQ-847: When desktop Notebook is bound to a user-visible external Vault, saving generated content shall use the native system Save dialog rooted at that Vault, accept Markdown destinations only, reject destinations outside the bound Vault, and convert the selected path to a Notebook-relative path before writing through Notebook APIs. Status: implemented.
- REQ-848: Saving to Notebook and exporting Markdown shall remain distinct commands: save creates or updates a Notebook-owned entry, while export writes a portable file or archive to an arbitrary user-selected local destination. Status: implemented for command and ownership separation; native desktop export destinations remain planned under REQ-849.
- REQ-849: Desktop Markdown export shall use a native system file dialog, while web/dev export shall use the platform-supported download flow. Status: planned.
- REQ-850: Saving generated Research content shall use the structured report title as the initial Notebook title and native Save-dialog file name; Markdown heading and request-derived titles are fallbacks, not the first prose sentence. Status: implemented.

## 18. Quiz Bank

- REQ-650: Quiz Bank shall be a module inside Space.
- REQ-651: Quiz Bank shall show historical quizzes.
- REQ-652: Quiz Bank shall show quiz scores.
- REQ-653: Quiz Bank shall show missed questions.
- REQ-654: Quiz Bank shall show explanations and citations.
- REQ-655: Quiz Bank shall support review and re-practice. Status: planned.
- REQ-656: Quiz Bank shall not be the primary quiz generation surface.
- REQ-657: Quiz generation shall remain in chat through the composer Quiz mode.
- REQ-658: The standalone Quiz navigation page shall be removed after Quiz Bank exists in Space.
- REQ-659: Quiz history shall move from the current Quiz page into Space / Quiz Bank.
- REQ-660: Quiz Bank shall support filtering by source type later. Status: planned.
- REQ-661: Users shall be able to `@` a Quiz session or Quiz question in Chat for explanation, follow-up practice, review, or related quiz generation. Status: implemented.
- REQ-662: Quiz-related `@` references shall preserve enough metadata for the agent to read the original prompt, question text, answer, explanation, citations, and user answer where available. Status: implemented.

## 19. Student Profile

- REQ-670: Student Profile shall be a module inside Space.
- REQ-671: Student Profile shall summarize what the student has studied.
- REQ-672: Student Profile shall summarize strengths.
- REQ-673: Student Profile shall summarize weaknesses.
- REQ-674: Student Profile shall summarize recent activity.
- REQ-675: Student Profile shall summarize quiz stats.
- REQ-676: Student Profile shall summarize recent notebook topics.
- REQ-677: Student Profile shall recommend next actions.
- REQ-678: Student Profile shall start with deterministic stats before complex LLM memory. Status: planned.
- REQ-679: Student Profile shall remain explainable. Status: implemented through source-linked L3 Memory.
- REQ-680: Student Profile shall eventually be editable or user-correctable. Status: implemented through editable L3 Markdown.
- REQ-681: Student Profile shall use Markdown memory documents as the initial durable source of truth. Status: implemented.
- REQ-682: Student Profile may later use a structured cache or projection derived from Markdown memory. Status: planned.
- REQ-683: Student Profile shall not require a separate hidden student-profile database in the MVP. Status: implemented.

## 20. Memory System

- REQ-690: The product shall provide a Memory module. Status: implemented.
- REQ-691: Memory shall use readable Markdown files as the primary durable representation in the MVP. Status: implemented for L2 and L3; L1 remains normalized product evidence.
- REQ-692: Memory shall support an L1 raw-event layer. Status: implemented.
- REQ-693: L1 shall include chat events. Status: implemented.
- REQ-694: L1 shall include quiz events. Status: implemented.
- REQ-695: L1 shall include notebook events. Status: implemented.
- REQ-696: The former separate Research L1 event category is retired. Ordinary
  Research-mode clarification and planning conversation shall use Chat L1 with
  `capability = research`;
  workflow traces are not learner-memory evidence, and saved reports enter L1
  through Notebook. Status: implemented.
- REQ-697: Memory shall support an L2 per-surface summary layer. Status: implemented.
- REQ-698: L2 shall include `chat.md`. Status: implemented.
- REQ-699: L2 shall include `quiz.md`. Status: implemented.
- REQ-700: L2 shall include `notebook.md`. Status: implemented.
- REQ-700A: L2 shall include `knowledge.md`. Status: implemented.
- REQ-701: The former requirement for a separate `research.md` L2 file is
  retired. Durable behavior from explicitly saved Research reports is
  consolidated into `notebook.md`.
- REQ-702: Memory shall support an L3 cross-surface memory layer. Status: implemented.
- REQ-703: L3 shall include `recent.md`. Status: implemented.
- REQ-704: L3 shall include `profile.md`. Status: implemented.
- REQ-705: L3 shall include `scope.md`. Status: implemented.
- REQ-706: L3 shall include `preferences.md`. Status: implemented.
- REQ-707: L3 shall include `teaching_strategy.md`. Status: implemented.
- REQ-708: Memory entries shall support stable hidden entry ids. Status: implemented.
- REQ-709: Memory entries shall support source references. Status: implemented.
- REQ-710: Memory entries shall be editable by the user. Status: implemented.
- REQ-711: Memory consolidation shall initially be manually triggered from the Memory module. Status: implemented as explicit update/check/dedupe actions.
- REQ-712: Memory consolidation shall show which source layers or surfaces will be used. Status: implemented through target-specific workflow tools and visible flow state.
- REQ-713: Memory consolidation shall write back to Markdown files. Status: implemented after change review.
- REQ-714: The system may later suggest consolidation after N turns, quiz completion, or research-report saves. Status: planned.
- REQ-715: The product shall provide a `read_memory` tool for agents. Status: implemented.
- REQ-716: Agents shall call `read_memory` when personalized teaching, quiz generation, review planning, or long-running learning context requires it. Status: implemented through tool-aware runtime instructions.
- REQ-717: Memory shall not be injected wholesale into every prompt by default. Status: implemented.
- REQ-718: `write_memory` shall be limited to explicit user preferences or user-approved facts. Status: implemented in runtime policy and tool contract.
- REQ-719: Memory content shall guide teaching behavior and personalization, not act as factual source material for external facts. Status: implemented.
- REQ-720: Memory Markdown footnote refs shall render as clickable inline source chips. Status: implemented.
- REQ-721: Clicking an inline memory source chip shall scroll to the corresponding bottom reference item. Status: implemented.
- REQ-722: Clicking a bottom memory reference item shall navigate to the related
  Chat, Notebook (including saved Research reports), Quiz, or Knowledge Base
  surface when possible. Book and Research-specific references are no longer
  supported. Status: implemented for current reference targets.
- REQ-723: Internal memory entry markers such as `<!--m_xxx-->` shall never be displayed in rendered Markdown. Status: implemented by Markdown rendering while remaining available in source/edit mode.
- REQ-724: The Memory LLM workspace shall require an explicit maintenance mode
  (`update`, `check`, or `dedupe`) and model selection before the user starts a
  run; selecting either control shall not start the workflow by itself. Status:
  implemented.
- REQ-725: The selected Memory model shall be resolved from the user's saved LLM
  configurations and passed to the existing `tutor.memory` workflow for that
  run. Status: implemented.
- REQ-726: The Memory document shall occupy the primary workspace area. The
  memory-file rail and LLM workspace shall use compact controls and restrained
  fixed widths so document reading and editing remain the visual priority.
  Status: implemented.
- REQ-727: The Memory workbench agent shall have read-only, on-demand access to
  all L1 event surfaces; all L1 evidence shall be addressable without being
  injected wholesale into the initial prompt. Status: implemented.
- REQ-728: The Memory workflow shall provide bounded tools to list and search
  L1 events, read an event, read surrounding source context, and resolve the
  original product artifact. Status: partially implemented; list, search,
  event, context, and complete event-snapshot reads are available, while
  product-artifact resolution remains follow-up work.
- REQ-729: L1 evidence tools shall use pagination, support cancellation and
  budgets, and shall not expose arbitrary filesystem access. Status: partially
  implemented; pagination and the read-only boundary are implemented, while
  explicit budgets remain.
- REQ-730: L1 events shall have stable event-level or turn-level references;
  multiple events in one session shall not collapse to a shared session-only
  citation identity. Status: implemented.
- REQ-731: L1 events shall preserve either a complete evidence snapshot or a
  durable pointer that resolves to the complete source content. Status:
  partially implemented; Chat events, including Research clarification turns,
  preserve complete exchanges, while remaining product surfaces require a full
  pointer audit.
- REQ-732: A Memory run shall begin with the target surface and may explicitly
  expand evidence discovery to other L1 surfaces. Cross-surface expansion shall
  be visible in the run flow. Status: implemented.
- REQ-733: Every L1 list, search, and read performed by the Memory agent shall
  be represented in trace and product-level flow progress. Status: implemented.
- REQ-734: Every agent-proposed Memory change shall cite evidence that was
  actually read during that run; unread, unknown, or stale refs shall be
  rejected by product validation. Status: implemented.
- REQ-735: The right Memory workbench shall display run controls, flow state,
  evidence and change counts, validation, errors, cancellation, retry, and a
  compact completion summary. It shall not display a full document draft as the
  normal run result. Status: partially implemented; flow, validation errors,
  change counts, and summaries are present, while cancellation and retry remain.
- REQ-736: Memory runs shall expose product-level states for queueing, source
  discovery, evidence reading, analysis, change proposal, validation, review,
  apply, completion, failure, and cancellation. Status: implemented.
- REQ-737: The Memory agent shall return a structured change set rather than a
  complete `proposed_markdown` document. Status: implemented.
- REQ-738: A Memory change set shall include the run id, target path, base
  revision, compact summary, findings, and independently reviewable insert,
  replace, or delete operations. Status: implemented.
- REQ-739: Each proposed operation shall include stable target anchors where
  applicable, changed text where applicable, source refs, and a human-readable
  reason. Stable entry ids and sections shall be preferred over line numbers.
  Status: implemented.
- REQ-740: The central Memory document area shall provide Read, Edit, and Review
  modes. Review mode shall render a deterministic diff generated by product
  code from the validated change set. Status: implemented.
- REQ-741: Memory diff review shall distinguish insertions, deletions, and
  replacements and shall show each change's reason and navigable source refs.
  Status: implemented.
- REQ-742: Users shall be able to accept or reject each Memory change and to
  accept or reject all pending changes. Status: implemented.
- REQ-743: Agent-produced Memory changes shall not modify persistent documents
  before explicit user confirmation. Status: implemented.
- REQ-744: Before apply, the product shall validate the target revision,
  operation and anchors, allowed sections, text limits, and source refs.
  Status: implemented.
- REQ-745: Accepted Memory changes shall be applied atomically, recorded in
  history, and reversible through undo. Status: implemented.
- REQ-746: If the target document has changed since the run's base revision,
  apply shall stop and require a rebase or rerun instead of silently overwriting
  user edits. Status: implemented.
- REQ-747: The left Memory rail shall own compact file selection, the center
  shall own document reading/editing/diff review, and the right workbench shall
  own controls and flow progress. L1 shall not appear as ordinary editable
  memory files in the primary rail. Status: implemented.
- REQ-748: Leaving and returning to the Memory workspace shall rejoin the newest
  active or reviewable Memory run without starting a duplicate run. The UI
  shall restore its target file, flow state, and pending change review. Status:
  implemented.
- REQ-749: The left Memory file rail shall project the state of every active or
  reviewable Memory run onto its target module. Running work shall use an
  animated progress indicator, pending changes shall use a distinct review
  indicator, and both shall be restored after workspace navigation. Status:
  implemented.
- REQ-750: A Memory run shall capture the global interface language when the
  run starts and use it as the output language for newly generated summaries,
  findings, reasons, and inserted or replaced memory text across L2 and L3.
  Existing memory shall not be translated merely because the global language
  changes, and technical terms or proper nouns may remain in their source
  language. The captured language shall remain fixed for the lifetime of the
  run. Status: implemented.
- REQ-751: Active L2 memory shall consist of `chat.md`, `quiz.md`,
  `notebook.md`, and `knowledge.md`; it shall not create or expose a separate
  `research.md` target. Status: implemented.
- REQ-752: `notebook.md` shall summarize durable behavior across ordinary
  notes and saved Research reports, including organization habits, preferred
  formats, report preferences, recurring research themes, and unresolved
  questions. It shall not copy report bodies or external factual findings into
  learner memory. Status: implemented.
- REQ-753: Research workflow searches, fetched sources, intermediate progress,
  traces, and unsaved reports shall remain in Research runtime/session state
  and shall not be treated as L1 learner-memory evidence. Status: implemented.
- REQ-754: Existing `L2/research.md` content shall not be migrated or retained.
  The obsolete file may be removed when the active L2 catalog is updated.
  Status: implemented.
- REQ-755: `MemoryEventCategory::Research`, `L1/research_events.jsonl`, and the
  Research-specific memory source filter shall be removed without legacy data
  migration. Ordinary Research-mode conversation events shall be recorded as
  Chat L1 with their capability metadata preserved. Status: implemented.
- REQ-756: Saving a Research report to Notebook shall be the explicit boundary
  that makes the report eligible for long-term memory consolidation. The saved
  report shall produce or update Notebook L1 evidence and may inform
  `notebook.md`; an unsaved report shall not. Status: implemented.
- REQ-757: Once `create_research_report` starts, its search/fetch trace,
  structured report attachment, and report body shall be excluded from Chat L1
  recording. A saved report shall use the normal Notebook event and
  `notebook:` reference contract; no `research:` memory reference shall be
  created. Status: implemented.
- REQ-758: Ordinary L3 generation shall use stable L2 entries as its primary
  evidence and shall not behave as another all-surface L1 consolidation run.
  Status: planned.
- REQ-759: L3 evidence discovery shall provide bounded list, search, read, and
  L1-provenance drill-down tools for stable L2 entries. Candidate lists shall
  not make an entry citeable. Status: planned.
- REQ-760: L3 changes shall use canonical entry-level L2 references rather than
  bare surface names. Status: planned.
- REQ-761: `profile.md`, `scope.md`, and `preferences.md` shall synthesize from
  their documented L2 source matrix; `teaching_strategy.md` may additionally
  use accepted Profile, Scope, and Preferences without creating a circular
  dependency. Status: planned.
- REQ-762: `recent.md` may use bounded recent L1 events as an explicit
  chronology exception while retaining current L2 context. Status: planned.
- REQ-763: L3 runs shall check relevant L2 freshness before analysis and expose
  stale dependencies instead of silently rewriting multiple layers. Status:
  planned.
- REQ-764: The retired draft-oriented Memory assist and consolidation APIs,
  prompt-injected L3 chunk implementation, and bare-surface citation path shall
  be removed before the layered L3 runtime is introduced. Status: implemented.
- REQ-765: Agents shall treat `read_memory` as silent internal context loading
  and shall not narrate implementation details such as "checking memory" or
  "reading the memory file" in assistant answer text. Status: implemented.
- REQ-766: When learner memory is relevant and sufficiently supported, agents
  shall use it naturally as remembered context. They may answer directly or use
  conversational language such as "I remember" without naming the tool or
  storage layer. Status: implemented.
- REQ-767: Agents shall hedge or ask for confirmation when memory is weak,
  stale, ambiguous, or conflicting, and shall not claim to remember content
  when `read_memory` returned no supporting memory. Status: implemented.
- REQ-768: Silent memory use shall not remove observability. Memory tool calls
  remain available in trace, and an agent shall truthfully explain the relevant
  prior conversation, quiz, Notebook-derived summary, or learner memory when
  the user explicitly asks how it knows. Status: implemented.

## 21. Markdown Rendering

- REQ-370: Assistant messages shall render Markdown.
- REQ-371: Markdown rendering shall support GitHub-flavored Markdown tables.
- REQ-372: Markdown rendering shall support math.
- REQ-373: Markdown rendering shall support KaTeX.
- REQ-374: Markdown rendering shall support soft line breaks.
- REQ-375: Markdown rendering shall support heading anchors.
- REQ-376: Markdown rendering shall support external links safely.
- REQ-377: The system shall not attempt to fix invalid LLM Markdown that is semantically malformed.
- REQ-378: The system prompt should encourage valid Markdown for tables and math.
- REQ-379: Product source references shall use dedicated UI behavior rather than relying only on raw Markdown footnote rendering. Status: planned.

## 22. Trace, Status, and UX

- REQ-390: Tool calls shall emit trace events.
- REQ-391: Tool results shall emit trace events.
- REQ-392: Long-running workflows shall emit status events.
- REQ-393: Some status events shall update in place rather than produce many visible bubbles.
- REQ-394: Progress should feel like an assistant message when product-relevant.
- REQ-395: Debug trace shall be available in a collapsible right panel.
- REQ-396: The right trace panel shall default to collapsed.
- REQ-397: The user-facing chat area shall not be visually separated from trace by a heavy divider.
- REQ-398: Failures shall show the reason, not only a generic failed state.
- REQ-399: The UI shall distinguish final answer text from progress/thinking/status text.

## 23. Settings

- REQ-410: Settings shall use tab-like sections.
- REQ-411: Settings shall include appearance controls.
- REQ-412: Settings shall include status/diagnostics.
- REQ-413: Settings shall include network/search configuration.
- REQ-414: Settings shall include LLM configuration.
- REQ-415: Settings shall include embedding configuration.
- REQ-416: Settings shall include capability/tool configuration.
- REQ-417: Settings shall include memory/session configuration when available.
- REQ-418: Settings shall support adding multiple LLM configs.
- REQ-419: Settings shall support adding multiple embedding configs.
- REQ-420: Settings shall support adding multiple search configs.
- REQ-421: Settings shall support save/apply behavior.
- REQ-422: Settings shall show unsaved changes clearly.
- REQ-423: Settings shall provide provider health checks. Status: planned.
- REQ-424: Settings shall support config import/export. Status: planned.
- REQ-425: Appearance colors shall be expressed through semantic theme tokens
  rather than component-local palette choices. The default `cool-light` theme
  shall use a cool-gray application frame and sidebar, a near-white Chat
  canvas, white assistant content surfaces, medium-gray user bubbles, and blue
  interaction accents. Status: implemented for the application shell,
  sidebar, Chat canvas, and ordinary messages.
- REQ-426: Settings shall allow users to select from multiple color themes,
  persist the selection locally, and apply it without restarting the app.
  The initial choices shall be `cool-light` and `graphite-dark`; changing only
  the theme shall not reset or replace the active conversation session.
  Status: implemented.
- REQ-427: The `graphite-dark` theme shall use neutral graphite framing, a
  near-black Chat canvas, elevated charcoal content surfaces, soft white text,
  and blue interaction accents. Ordinary text, muted text, controls, status
  colors, message surfaces, and scrollbars shall remain distinguishable without
  relying on pure-black backgrounds. Status: implemented for current product
  surfaces; continue migrating new component-local colors to semantic tokens.

## 24. Navigation and Layout

- REQ-440: The app shall have a left sidebar.
- REQ-441: The sidebar shall include Chat.
- REQ-442: The sidebar shall include Tutor/assistant entry.
- REQ-443: The sidebar shall include Writing entry.
- REQ-444: The former Books sidebar requirement is retired.
- REQ-445: The sidebar shall include Knowledge Base.
- REQ-446: The sidebar shall include Quiz.
- REQ-447: The sidebar shall include Space.
- REQ-448: The sidebar shall include Memory.
- REQ-449: The sidebar shall include Settings.
- REQ-450: The sidebar shall show recent sessions.
- REQ-451: The sidebar shall be collapsible.
- REQ-452: Recent sessions shall support rename and delete.
- REQ-452A: Selecting or merely activating a recent session shall not reorder
  the sidebar session list. A session should move to the top only after the user
  sends a new message in that session or the session otherwise receives new
  conversation activity.
- REQ-452B: Recent sessions shall support manual pinning and unpinning. Pinned
  sessions shall remain above unpinned sessions and shall not be displaced by
  ordinary activity sorting.
- REQ-452C: Recent session pin/unpin shall be available from a product-owned
  right-click context menu on the session item, alongside existing session
  actions such as rename and delete.
- REQ-452D: When a recent session is currently open in the Chat workspace, its
  sidebar item shall show a restrained but unambiguous selected state that is
  visually distinct from hover and running indicators. Selection styling shall
  not reorder the session list and shall remain compatible with pinned and
  running states. Status: implemented.
- REQ-452E: The entire visible hover and selected area of a recent-session item
  shall activate that session, except for explicit inline actions such as rename
  and delete. The full-row target shall remain keyboard focusable. Status:
  implemented.
- REQ-453: Clicking Chat shall open a new conversation by default.
- REQ-454: The new conversation view shall center the composer.
- REQ-455: The chat scroll behavior shall match common chat apps.
- REQ-456: The UI shall remain usable on common desktop sizes.
- REQ-457: Mobile responsiveness shall be improved. Status: planned.

## 24A. Product Onboarding and Guidance

- REQ-458: The product shall provide a first-run onboarding flow whose goal is
  to help the user complete the first useful learning task, rather than present
  a tour of every page and control. Status: implemented.
- REQ-459: First-run onboarding shall provide six concise steps: verify or
  configure an available LLM, optionally choose or create a Tutor, configure
  and use a Knowledge Base, configure Notebook storage, understand and maintain
  Memory, understand the real composer controls, and select a Chat, Research,
  Quiz, or Organize conversation mode. Status: implemented.
- REQ-460: The model step shall detect whether a usable model configuration
  already exists. When no usable configuration exists, it shall link directly
  to the relevant settings and offer the existing connection test. It shall
  not ask an already-configured user to enter credentials again. Status:
  implemented; configured-profile release QA remains pending.
- REQ-461: Tutor selection during onboarding shall remain optional. Skipping
  it shall use Temporary Assistant and shall not block the user from entering
  the normal conversation workspace. Status: implemented.
- REQ-461A: Knowledge Base onboarding shall explain that RAG requires an
  embedding configuration, show current embedding and Knowledge Base readiness,
  link to the real configuration and Knowledge Base surfaces, and explain how
  a Knowledge Base is selected from Chat for retrieval and citations. Users who
  do not need RAG may skip the step. Status: implemented.
- REQ-461B: Notebook onboarding shall distinguish the always-available app-local
  Notebook from an optional external Markdown Vault, show the current storage
  mode, and link to both Notebook settings and the Notebook workspace. Status:
  implemented.
- REQ-461C: Memory onboarding shall explain the L1/L2/L3 model and the supported
  update, check, deduplicate, review, and apply workflow. It shall link to the
  real Memory workspace without modifying memory merely by viewing the guide.
  Status: implemented.
- REQ-462: The final onboarding step shall provide a dedicated child interface
  that first mirrors the real composer controls, then introduces Chat,
  Research, Quiz, and Organize. Each mode shall cover suitable scenarios,
  runtime or workflow behavior, usable material, expected output, and an
  editable starter prompt. Selecting its start action shall open a real new
  conversation in that mode without automatically sending the prompt. The
  interface shall respect the selected Tutor's allowed capabilities and
  Notebook permission. Notebook itself shall remain a workspace rather than a
  conversation mode; internal Code Exec and retired Deep Solve shall not
  appear. Status: implemented.
- REQ-463: Empty product surfaces shall provide contextual guidance focused on
  the next meaningful action for that surface. Notebook, Quiz Bank, Memory,
  Tutor, and Research guidance shall disappear automatically once the relevant
  user data or state exists. Status: implemented for Notebook, Quiz Bank,
  Memory, Tutor, and Research/Quiz conversation empty states.
- REQ-464: Contextual guidance shall not permanently occupy primary workspace
  area, repeatedly interrupt experienced users, or depend on long sequential
  spotlight tours. Complex features may show a concise one-time hint when
  first used. Status: implemented for compact contextual guidance; no
  additional one-time interaction hints are currently enabled.
- REQ-465: Settings > Help shall provide a persistent, non-linear guide center
  rather than reopening first-run onboarding as its primary experience. Users
  shall be able to enter any topic directly, and the last viewed topic shall be
  restored locally without resetting onboarding state. Rerunning first-run
  setup may remain a secondary explicit action. Status: implemented.
- REQ-466: Onboarding state shall be local-first and versioned. At minimum it
  shall track an `onboarding_version`, completion state, and dismissed
  contextual hints so future releases can introduce only the relevant new
  guidance instead of replaying the full first-run flow. Status: implemented.
- REQ-467: Dismissing or skipping onboarding shall be supported at every
  nonessential step. A dismissed flow shall leave the normal product fully
  usable, except where an unavailable model genuinely prevents an LLM action.
  Dismissal shall pause rather than complete an unfinished flow. While the flow
  remains incomplete, a compact floating action shall reopen it without taking
  permanent workspace layout. Launching a real starter task or choosing the
  explicit final `Complete` action shall mark the flow complete and remove the
  floating action. Status: implemented.
- REQ-468: Onboarding and contextual guidance shall follow the active UI
  language and desktop theme, remain keyboard accessible, and avoid browser-
  style permission or navigation interactions. Status: partially implemented;
  Chinese/English copy, themes, semantic controls, and minimum-window layout
  are present, while complete keyboard-only installed-desktop QA remains.
- REQ-469: Onboarding analytics shall not require hosted telemetry for MVP.
  Completion and hint state shall stay in local settings unless the user later
  opts into a separately specified diagnostics mechanism. Status: implemented.
- REQ-469A: Product guidance for Chat shall use an interface-faithful composer
  illustration with the same order, icons, and labels as the real input box.
  It shall cover conversation mode, attachment upload, Knowledge Base or
  Notebook source association, Space `@` references, model selection, and the
  send/stop control. Status: implemented.
- REQ-469B: Guidance shall explain the operational difference between attaching
  a temporary file, associating a Knowledge Base or Notebook for retrieval, and
  explicitly referencing one Notebook or Quiz target with `@`. Status:
  implemented.
- REQ-469C: The stable built-in `general-tutor` profile shall serve as the
  Usage Guide Tutor. It shall answer product-use questions in ordinary Chat,
  name exact product paths and controls, and remain non-deletable. Existing
  untouched General Tutor profiles may migrate to this identity, while
  user-customized built-in profiles shall not be silently overwritten. Status:
  implemented.
- REQ-469D: Help shall provide a direct action that starts a real new Chat bound
  to the Usage Guide Tutor. The action may prefill an editable prompt but shall
  not send it automatically. Status: implemented.

## 25. Storage and Data

- REQ-470: Product data shall be stored locally for MVP.
- REQ-471: Knowledge base metadata shall be durable.
- REQ-472: Quiz sessions shall be durable.
- REQ-473: The former Books durability requirement is retired; no Book data
  compatibility or migration is required.
- REQ-474: Runtime sessions shall be durable through runtime session storage.
- REQ-475: Trace events shall be restorable where relevant.
- REQ-476: Compact summaries shall be restorable where runtime supports them.
- REQ-477: The project shall evaluate SQLite when JSON stores become limiting.
- REQ-478: Storage paths shall be documented.
- REQ-479: Migration strategy shall be defined before changing persisted schemas. Status: planned.

## 26. Testing and Quality

- REQ-490: Frontend production build shall pass.
- REQ-491: `tutor-agent` mock integration tests shall pass.
- REQ-492: `tutor-web` library tests shall pass.
- REQ-493: RAG retrieval shall be testable without a real LLM.
- REQ-494: Quiz generation shall have non-real-LLM tests.
- REQ-495: Research trace events shall have mock tests.
- REQ-496: The former Book store/API test requirement is retired; tests shall
  be removed with the retired implementation.
- REQ-497: Attachment parsing shall have tests.
- REQ-498: `tutor-web` startup smoke test shall be added. Status: planned.
- REQ-499: `cargo clippy --workspace --all-targets --all-features -- -D warnings` shall be run and cleaned up or documented. Status: planned.
- REQ-500: CI shall run Rust tests and frontend build. Status: planned.

## 27. Documentation

- REQ-510: README shall describe current capabilities accurately.
- REQ-511: README shall describe startup commands accurately.
- REQ-512: README shall describe current limitations accurately.
- REQ-513: Product roadmap shall match implemented state.
- REQ-514: Research plan shall track implemented and planned work.
- REQ-515: Framework gaps shall be recorded in `docs/framework-feedback.md`.
- REQ-516: Runtime/API issues shall be raised upstream when appropriate.
- REQ-517: Real-provider test requirements shall be documented. Status: planned.

## 28. Security and Privacy

- REQ-530: The app shall be considered single-user local-first in MVP.
- REQ-531: API keys shall not be logged in trace events.
- REQ-532: API keys shall be masked in the UI.
- REQ-533: Uploaded documents shall stay local unless explicitly sent to providers for embedding or LLM context.
- REQ-534: Code execution shall not be treated as safe for hostile multi-user input.
- REQ-535: Multi-user auth and permissions shall be out of MVP scope.
- REQ-536: Stronger sandboxing shall be required before hosted/multi-user deployments. Status: planned.

## 29. Desktop Release

- REQ-560: The product shall be packaged as a Tauri desktop application. Status: implemented; manual desktop QA pending.
- REQ-561: The desktop app shall bundle the React UI production build. Status: implemented and validated by the `v0.3.1` release workflow; clean-install QA remains pending.
- REQ-562: The desktop app shall start the local Rust backend automatically. Status: implemented; manual desktop QA pending.
- REQ-563: The desktop app shall not require users to run `npm run dev` or `cargo run` manually. Status: implemented for release builds; manual desktop QA pending.
- REQ-564: The desktop app shall use an OS-appropriate application data directory by default. Status: implemented.
- REQ-565: The desktop app shall keep API keys user-configured and shall not bundle provider credentials. Status: implemented.
- REQ-566: The first desktop release shall prioritize Windows packaging. Status: implemented; `v0.3.1` publishes NSIS and MSI assets.
- REQ-567: The release process shall support reproducible builds through scripts or CI. Status: implemented and validated with private dependency access in the `v0.3.1` tag workflow.
- REQ-568: The app shall preserve the local-first storage model in the desktop release. Status: implemented.
- REQ-569: Desktop packaging details shall be tracked in `docs/plans/2026-06-28-tauri-desktop-release-plan.md`. Status: implemented.
- REQ-570: Every future public desktop release shall include a macOS artifact, preferably a `.dmg` built on macOS and uploaded to the same GitHub Release as the Windows installers. Status: implemented and validated for both x64 and arm64 in `v0.3.1`.
- REQ-571: The desktop app shall hide or replace browser-default interactions that make the product feel like an embedded webpage, including visible browser context-menu behavior. Status: in progress; app-owned context menus, native clipboard use, external-link routing, and file-drop interception are implemented.
- REQ-572: The desktop app shall use a fixed application shell where top-level window scrolling is avoided and scroll behavior is owned by specific panes or work areas. Status: in progress; top-level shell and Chat/Trace panes are hardened, full surface audit remains.
- REQ-573: The desktop app shall provide product-owned context menu capability
  areas for major active surfaces such as Notebook, Chat, Knowledge, and
  Research, with detailed menu items specified during implementation design.
  Status: in progress; framework and first Notebook/generic action slice are implemented.
- REQ-574: The desktop app shall prefer native desktop affordances for file/folder selection, revealing local files, external link opening, and future app-level shortcuts where appropriate. Status: in progress; shared directory picker helper, Notebook Vault folder binding, native clipboard, data directory reveal, and external-link opening are implemented.
- REQ-575: Desktop polish shall preserve the local-first sidecar architecture and shall not rewrite existing backend routes as Tauri commands unless a native capability requires it. Status: implemented.
- REQ-576: The Windows desktop app shall retain native window chrome and system
  caption controls while hiding the upper-left caption icon and visually
  suppressing the product-name label through a thin Windows-only adapter. Native
  drag, Snap Layouts, double-click maximize/restore, minimize, maximize, close,
  keyboard behavior, resize, taskbar icon, and Alt+Tab icon shall remain intact.
  macOS and Linux shall retain their unmodified native title bars. Status:
  implemented.

## 30. Acceptance Baseline

- REQ-550: A clean clone shall be able to start backend and frontend using README commands.
- REQ-551: A user shall be able to create a knowledge base and upload a document.
- REQ-552: A user shall be able to ask a RAG-grounded question and see citations.
- REQ-553: Retired. Users shall solve complex questions through Chat/Tutor and
  may inspect ordinary tool traces; no new Deep Solve turn can be started.
- REQ-554: A user shall be able to generate and answer a Quiz.
- REQ-555: A user shall be able to run a Research turn and get a sourced report.
- REQ-556: A user shall be able to save a Research report into Notebook.
- REQ-557: A user shall be able to reopen the app and find prior sessions and
  saved Notebook reports.
