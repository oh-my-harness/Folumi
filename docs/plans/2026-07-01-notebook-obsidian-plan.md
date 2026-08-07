# Notebook Obsidian-Like Plan

> 历史说明：本计划中的笔记导入与导出能力已由
> `docs/specs/2026-08-08-notebook-import-export-removal-decision.md` 明确撤销；
> 当前产品直接使用用户选择的本地 Markdown 文件夹。

> Status: proposed | Date: 2026-07-01 | Scope: evolve Space / Notebook from a saved-record list into a Markdown-first connected knowledge workspace.

## 1. Product Positioning

Notebook should become the user's durable knowledge workspace inside Space.

It should borrow the useful parts of Obsidian:

- Markdown-first notes,
- wiki-style links,
- backlinks,
- tags,
- local import/export,
- graph-style navigation later.

It should not become a full Obsidian clone in the first slice. The goal is to
make learning artifacts connected, portable, and agent-readable while keeping
Chat as the agent interaction surface.

The product split remains:

```text
Chat
  Generate, explain, research, quiz, and propose edits.

Notebook
  Store and connect durable Markdown notes.

Books
  Publish polished outputs from selected notes.

Memory
  Summarize learner state and behavior.
```

Notebook is user-authored knowledge. Memory is agent-maintained learner state.
They can reference each other, but they should not collapse into one store.

## 2. Required Capabilities

### Markdown Notes

- Each Notebook entry is a Markdown note.
- Notes keep title, body, type, tags, source metadata, created time, and updated time.
- Existing `NotebookEntry` storage remains the first implementation base.
- Research reports remain `NotebookEntry(type = research_report)`.

### Wiki Links

Notebook Markdown should support wiki-style links:

```md
[[Photolithography]]
[[note-id|display title]]
[[OPC]]
```

Initial behavior:

- Parse links from Markdown.
- Resolve by stable note id first when available.
- Resolve by normalized title when id is not present.
- Show unresolved links as creatable notes.
- Keep normal Markdown links unchanged.

### Backlinks

Each note detail view should show notes that link to the current note.

Backlinks should include:

- source note title,
- short snippet around the link,
- link target,
- updated time.

### Tags

Notebook Markdown and metadata should support tags:

```md
#lithography
#weak-point
#research
```

Tags should support:

- parsing from note body,
- manual metadata tags,
- filtering note list,
- future use by Quiz and Student Profile.

### Import

Import is required, not optional.

Supported first formats:

- one or more `.md` files,
- a folder of Markdown files,
- a `.zip` containing Markdown files and assets.

Import behavior:

- Preserve filenames as initial titles when frontmatter is absent.
- Parse YAML frontmatter if present.
- Preserve Markdown content without destructive cleanup.
- Convert Obsidian-style `[[links]]` into internal link index entries without rewriting content by default.
- Store unknown frontmatter fields in metadata.
- Detect duplicate titles and either keep both with suffixes or ask the user before merge.
- Record import source metadata.
- Show skipped-file details after the final import, including file path and reason.
- Batch-create imported entries so a large import writes the Notebook store once
  instead of once per note.
- In the desktop app, support native folder import for Obsidian Vaults by
  recursively reading Markdown files from the selected directory.
- Detect Obsidian attachments or embedded asset references that are not imported
  yet and report that limitation clearly in the import result.

Later import formats:

- plain text files,
- exported chat Markdown,
- exported Book chapters,
- selected Knowledge Base snippets.

### Export

Export is required, not optional.

Supported first formats:

- export one note as Markdown,
- export selected notes as Markdown files,
- export the whole Notebook as a folder or zip.

Export behavior:

- Preserve Markdown body.
- Preserve YAML frontmatter for title, id, type, tags, source metadata, created time, and updated time.
- Preserve `[[links]]` where possible.
- Include assets once asset support exists.
- Provide a predictable file naming strategy.

Later export formats:

- PDF,
- HTML,
- Book-ready Markdown bundle,
- Obsidian-compatible vault export.

## 3. Data Model Direction

The Notebook storage target is a file-backed vault directory, not a single JSON
blob. Markdown note files should become the durable source of truth for note
content, while JSON is kept only as a lightweight index and product metadata
store.

Target layout:

```text
notebook/
  index.json
  vault/
    TCC.md
    FindOptics.md
    folder/
      Nested Note.md
```

`vault/` stores user-authored Markdown files. `index.json` stores stable ids,
relative paths, entry types, product source metadata, timestamps, and other
fields that do not naturally belong in Markdown frontmatter.

Keep the existing `NotebookEntry` API shape while using the file-backed
persistence backend:

```ts
NotebookEntry {
  id: string
  spaceId?: string
  type: 'note' | 'research_report' | 'chat_excerpt' | 'quiz_summary' | 'source_snippet' | 'deep_solve_result'
  title: string
  path?: string
  markdown: string
  tags: string[]
  metadata: Record<string, unknown>
  sourceSessionId?: string
  sourceMessageId?: string
  createdAt: string
  updatedAt: string
}
```

Add a derived index:

```ts
NotebookLinkIndex {
  sourceEntryId: string
  targetEntryId?: string
  targetTitle?: string
  raw: string
  alias?: string
  resolved: boolean
}
```

The link index can be rebuilt from note Markdown. It should not become the
primary source of truth.

### Title and Path Semantics

For imported Markdown and Obsidian vault notes, the note title used by Tutor
Agent should prefer the file name stem, not `frontmatter.title`.

Example:

```text
TCC.md
---
title: TCC (Transmission Cross Coefficient)
---
```

The entry title should be `TCC`. The frontmatter title should be preserved in
metadata and may be shown as an optional display subtitle or alias. This keeps
`[[TCC]]`, exported filenames, Obsidian compatibility, and local file paths
stable.

### Storage Direction

Notebook storage should use the file-backed vault layout directly:

- write each note body to an individual `.md` file under `notebook/vault/`,
- keep an `index.json` for ids, relative paths, source mappings, entry types,
  timestamps, and non-Markdown product metadata,
- preserve imported relative folder paths,
- use file-name stems as imported note titles by default,
- preserve frontmatter and unknown metadata without letting frontmatter title
  overwrite the imported file-name identity,
- update the index and Markdown file together for create, edit, rename, delete,
  import, export, and agent-applied edit flows.

The old single-file `notebook_entries.json` format is not a supported storage
format for this product direction.

### Editor-Style File Browser and Indexing

The Notebook UI should behave more like a local Markdown editor than a saved
record list. The file browser must stay fast even when the bound Vault contains
many folders and notes.

Current first-slice behavior is intentionally simple but too expensive for a
real Vault:

- the Notebook list API refreshes from the Vault on each list call,
- refresh recursively scans the whole Vault directory,
- refresh reads every Markdown file body,
- list summaries can trigger more than one full refresh,
- the list payload still carries note Markdown in the flattened entry object,
- detail loading and relation calculation can repeat the same broad work.

This is acceptable for small demos, but it is not the target editor model.

Target behavior:

- file tree listing returns lightweight metadata only:
  `id`, `title`, `path`, `entry_type`, `updated_at`, `size`, `mtime`, and basic
  tag/frontmatter summary where already indexed;
- note Markdown is loaded only when the user opens a note detail;
- link, backlink, tag, and graph data are derived indexes that can be refreshed
  separately from the visible file tree;
- opening the Notebook tab should use the current in-memory index immediately
  and avoid a blocking full Vault scan;
- external Vault changes should be detected through a file watcher or explicit
  refresh, not by rescanning the entire Vault for every UI navigation;
- large folder trees should support lazy expansion and virtualized rendering if
  the note count becomes large.

Backend direction:

- split the list API from the detail API:
  - `GET /api/notebook/tree` returns folders and lightweight note summaries,
  - `GET /api/notebook/entries/{id}` returns full Markdown and relation detail,
  - `POST /api/notebook/refresh` explicitly refreshes or reconciles the Vault;
- keep a `NotebookIndex` in memory with path, id, title, timestamps, file size,
  and known derived metadata;
- store enough file stat data in `index.json` to skip unchanged files during
  refresh;
- read changed Markdown files only when `mtime` or size changes, or when a file
  is opened and no fresh content cache exists;
- use a file watcher such as Rust `notify` for bound Vault directories when the
  desktop backend is running;
- debounce watcher events and batch index writes to avoid rewriting
  `index.json` for every low-level file-system event.

Frontend direction:

- do not refresh the Notebook list every time the tab becomes visible if a
  recent tree is already loaded;
- keep the expanded folder state and selected note stable across tab switches;
- render the file tree from lightweight summaries;
- load note body, backlinks, and local graph data only after note selection;
- provide a visible manual refresh action for users who suspect external file
  changes have not been picked up yet;
- surface indexing state such as `watching`, `refreshing`, `last refreshed`,
  and `N changed files indexed`.

Mature editors such as VS Code follow this split: the explorer is a metadata
view over the file system, while file content, search indexes, symbols, and
graph-like relations are loaded lazily or maintained by background watchers.
Tutor Agent should adopt the same shape for Notebook instead of treating list
navigation as an opportunity to fully rebuild the Vault.

### Mature Editor Model

The Notebook editor should use an editor-style data model instead of a
record-list model.

In this model, the Vault is the source of truth for Markdown files, while the
backend maintains a lightweight index and the frontend maintains UI state for
open documents. Each layer has a separate responsibility:

- file system: stores folders, Markdown files, and future assets;
- Notebook index: stores stable ids, relative paths, file stats, titles, tags,
  aliases, parsed links, backlinks, and product metadata;
- file watcher: observes external changes and schedules debounced index
  updates;
- frontend explorer: renders folder and note metadata without reading note
  bodies;
- editor tabs or current document state: load and keep only opened note bodies;
- relation panels: request backlinks, outgoing links, local graph, and metadata
  for the selected note only.

Expected lifecycle:

1. App startup loads `notebook/index.json` and starts the watcher for the bound
   Vault if available.
2. Opening Notebook asks for the current tree from memory/index and should not
   perform a blocking recursive scan.
3. Expanding folders and filtering notes operate on lightweight tree metadata.
4. Selecting a note loads that note's Markdown body and selected-note relation
   detail.
5. External file changes trigger watcher events, which are debounced, batched,
   and reconciled into the index.
6. Manual refresh remains available as a fallback when watcher events are missed
   or the user changes files while the backend is not running.

This is close to how mature local editors work: VS Code keeps an explorer model
and file watcher, opens document buffers lazily, and runs heavier search or
symbol work separately from ordinary navigation. Tutor Agent should follow this
shape even if the first UI has only one visible editor pane.

### Editor UX Target

The first production-ready Notebook editor should provide:

- a persistent folder tree with stable expansion and selection state;
- a single-note Markdown editor/preview area, with room to evolve into tabs;
- fast title/path rename flows that update the file path and index together;
- create note and create folder actions scoped to the selected folder;
- move/delete actions that update links or at least report affected links;
- Obsidian-style `[[wiki links]]` rendering and navigation;
- clear unresolved-link behavior, such as offering to create the target note;
- a collapsible right panel for backlinks, outgoing links, tags, source
  metadata, and local graph;
- a compact status line for watcher/index state, current path, unsaved state,
  and last saved time.

The editor should prefer preserving user files over imposing product-specific
serialization. Unknown frontmatter, comments, aliases, and Obsidian-compatible
syntax should remain in the Markdown unless the user explicitly applies a
normalization or organization proposal.

## 4. UI Direction

Notebook should feel like a compact knowledge workspace:

- left pane: fast file tree, filters, and tags,
- center pane: Markdown editor/preview,
- right pane: backlinks, outgoing links, source metadata, agent edit proposal.

Near-term views:

- list,
- detail,
- edit,
- backlinks,
- tags,
- manual refresh and indexing status.

Later views:

- local graph for current note,
- full graph for Space,
- unresolved links list,
- duplicate notes review.

## 5. Agent Behavior

Agents should interact with Notebook through explicit product tools and user
confirmation.

Allowed agent behaviors:

- read a mentioned note,
- propose edits to a note,
- propose new links,
- propose tags,
- propose merging duplicate notes,
- summarize a group of notes,
- generate a quiz from explicitly referenced notes.

Write behavior:

- Agent writes are proposals first.
- User applies or rejects proposals.
- Applied writes create Notebook memory events.
- Direct silent writes are out of scope.

Potential future tools:

```text
list_notebook_tree
read_notebook_note
search_notebook
propose_notebook_change
```

`read_space_item` can remain the initial boundary. Add more specific Notebook
tools only when the generic Space tool becomes awkward.

Tool availability rules:

- Notebook exploration tools are available only when the chat source selector is
  set to Notebook, or when the user explicitly references Notebook content with
  `@`.
- Explicit `@` references allow the agent to read the mentioned artifact through
  `read_space_item`; they do not grant permission to explore the whole Vault.
- Notebook maintenance tools are available only in Organize mode.
- Chat, Quiz, and Research may answer from explicitly referenced or
  associated Notebook content, but they should not propose Notebook maintenance
  operations unless the session is in Organize mode.
- Applying Notebook changes remains product-owned and user-confirmed. The agent
  must never write directly to the Vault or bypass Notebook APIs.

Exploration capability means the agent can inspect the Vault without changing
it: list the tree, search paths/titles/tags/frontmatter/body/wiki links, read
selected notes, and inspect links/backlinks. Maintenance capability means the
agent can propose changes: edits, links, tags, moves, renames, merges, splits,
or new notes.

### Chat-Triggered Organization

Notebook organization should primarily be triggered from Chat, not from a
separate "AI maintenance" surface inside Notebook.

Supported user intents:

- "帮 @OPC 这篇笔记补一些 wiki links。"
- "给 @光刻模型 这篇笔记整理标签。"
- "看看 @OPC 和 @光学邻近校正 是不是重复。"
- "把这篇笔记改得更适合后续出题。"

The product flow is:

```text
User asks in Chat
  -> Agent reads mentioned Notebook entries through read_space_item
  -> Agent produces a proposal tool result
  -> Chat renders a review card
  -> User applies or rejects
  -> Product updates Notebook through normal Notebook APIs
  -> Product records a Notebook memory event
```

The agent must not directly write Notebook content. Even if the user asks the
agent to "修改这篇笔记", the agent should produce a proposal first. The product
UI owns the final write after explicit user confirmation.

Proposal types:

- `notebook_edit`: complete Markdown replacement or diff for one entry.
- `notebook_tags`: suggested tags to add/remove.
- `notebook_links`: suggested `[[wiki links]]` to add or normalize.
- `notebook_merge`: suggested canonical note, merged Markdown, and affected
  source entries.

For the first implementation, all organization proposal types can be serialized
as a structured proposal plus a complete Markdown replacement. More specialized
apply APIs can come later if complete Markdown replacement becomes too blunt.

### Notebook Lookup Without Explicit `@`

Users should not always have to `@` a note. If the user asks a question that
appears to involve saved notes, the agent may search Notebook before answering.

Examples:

- "我之前关于 OPC 记了什么？"
- "根据我的笔记解释一下光刻胶模型。"
- "帮我从已有笔记里找一下和 EUV 有关的内容。"
- "这和我之前研究的张仕林有什么关系？"

This requires a separate Notebook lookup capability from `read_space_item`:

```text
search_notebook(query, limit, filters?)
  -> returns candidate entries with id, title, type, tags, snippet, score

read_notebook_note(entry_id)
  -> returns exact Markdown for a selected candidate
```

Behavior rules:

- If the user explicitly references an artifact with `@`, use `read_space_item`
  for that exact artifact.
- If Notebook is not associated in the chat source selector, do not search or
  read unrelated Notebook entries autonomously.
- If the user asks about "my notes", "Notebook", "previously saved", or a topic
  likely stored in Notebook, call `search_notebook` before answering only when
  Notebook is associated.
- If search returns one or a few high-confidence candidates, read the relevant
  note(s) and answer with Notebook citations.
- If search returns ambiguous candidates, ask the user to choose or present the
  candidate list before making strong claims.
- If no candidate is found, say that no relevant Notebook entry was found, then
  optionally answer from general knowledge only if the user asked for that.
- For edit/merge requests without `@`, search first, then ask the user to
  confirm the target notes before creating a proposal.

This keeps Chat natural while avoiding two bad extremes: forcing users to always
pick notes manually, or letting the agent silently invent Notebook context.

### Chat Source Association

Notebook association in the chat composer should share the existing knowledge
source button with Knowledge Base selection. Do not add a second top-level
Notebook toggle in the composer.

The shared selector should represent "source association" rather than only
"knowledge base association":

```text
No source
Notebook
Knowledge Base: <kb name>
```

Product semantics:

- `No source`: the agent uses conversation context, attachments, explicit `@`
  mentions, memory, and mode-specific tools only.
- `Notebook`: the agent may use Notebook as a local plain-text knowledge source.
- `Knowledge Base`: the agent may use the selected LanceDB-backed RAG knowledge
  base.

Notebook association is intentionally plain-text only. It must not use
embeddings, LanceDB, or any future vector index. Notebook remains a Markdown
workspace, not a RAG corpus.

When Notebook is associated:

- Chat mode may search Notebook when answering questions that could benefit from
  saved notes.
- Organize mode should actively use Notebook search and read tools.
- Quiz mode may use Notebook search as source material when no more specific
  `@` item or attachment is provided.
- Research mode may use Notebook as private prior context, but external factual
  claims still require web search/fetch when appropriate.
- The agent may autonomously explore Notebook as a plain-text Vault, but it
  should cite navigable Notebook sources when using discovered notes.

The backend should model this separately from `kb_id`:

```ts
SessionSourceAssociation =
  | { type: 'none' }
  | { type: 'notebook'; scope: 'default_space' }
  | { type: 'knowledge_base'; kbId: string }
```

For compatibility with the current session model, the first implementation may
store this as:

```ts
kb?: string
notebook_enabled: boolean
```

Rules:

- `kb` must only mean a real Knowledge Base id.
- `notebook_enabled` must only mean plain-text Notebook search is available.
- The UI should prevent selecting both Notebook and Knowledge Base from the same
  shared source button until a future multi-source design is introduced.
- Notebook search results should cite navigable Notebook sources such as
  `notebook:<entry_id>`.

### Chat Modes And Tools

Chat modes should represent task intent and default workflow, not a hard-coded
tool set.

Recommended composer modes:

- `chat`: ordinary tutoring, explanation, and lightweight Q&A.
- `deep_solve`: retired legacy session type; old records remain readable.
- `quiz`: quiz generation from conversation, attachments, associated source, or
  explicit `@` material.
- `research`: web/source exploration and report generation.
- `organize`: Notebook/Space organization, including search, tags, links,
  deduplication, and edit proposals.

`code_exec` should be demoted from a user-facing mode to a tool available to
other modes. The model should use it when computation, verification,
simulation, parsing, or code execution is needed. This prevents a confusing
"code mode" that is really just one tool, while keeping code execution useful in
Chat, Research, Quiz, and Organize workflows.

Organize mode defaults:

- Notebook association should be enabled by default when entering Organize mode.
- The agent should search Notebook before making claims about saved notes.
- The agent may propose edits, tags, links, moves, renames, merges, splits, or
  new notes.
- All writes must remain proposal-first and user-confirmed.

Maintenance mode boundary:

- Notebook maintenance proposal tools are only registered in Organize mode.
- If a user asks for maintenance from another mode, the assistant should explain
  that Notebook maintenance requires Organize mode and offer to continue there.
- Read-only Notebook exploration can still happen outside Organize mode when
  Notebook is associated, but only for answering, quiz generation, or research
  grounding.

## 6. Import / Export Architecture

Backend responsibilities:

- parse uploaded Markdown files or zip bundles,
- extract frontmatter,
- create or update Notebook entries in batches,
- rebuild link/tag index,
- generate Markdown or zip exports.
- report skipped Markdown files and skipped/non-imported Obsidian assets with
  concrete reasons.

Frontend responsibilities:

- file/folder/zip picker,
- import preview,
- final import result with imported and skipped details,
- conflict resolution UI,
- export scope selection,
- download generated archive.

Desktop responsibilities:

- native folder picker for Obsidian Vault import,
- native Save dialog for saving generated notes/research reports when an
  external Notebook Vault is bound. The chosen path must be inside the bound
  Vault and should be converted to a Notebook-relative Markdown path before
  writing through Notebook APIs.
- recursive Markdown discovery under the selected folder,
- preserve relative source paths from the selected vault root.

Save-to-Notebook UX:

- Desktop + external Vault: use the system Save dialog with a `.md` default
  file name and the bound Vault as the default location. Reject or warn when the
  selected path is outside the Vault instead of writing arbitrary files outside
  Notebook ownership.
- Desktop without external Vault: use an app-owned Notebook tree picker because
  the app-local Vault is an internal data directory, not a user-facing file
  system location.
- Web/dev fallback: use the app-owned Notebook tree picker.
- The Notebook tree picker supports selecting root or an existing folder,
  expanding/collapsing folders, and creating a folder in place.
- The save flow remembers the last selected Notebook folder when it still
  exists, previews the final Notebook-relative folder, file name, and logical
  path, and validates invalid or conflicting destinations before confirmation.
- After saving, show the destination and provide a direct action to open the
  created Notebook entry.
- "Save to Notebook" remains a Notebook-owned database/Vault operation.
  "Export Markdown" is a separate command and may use the system file dialog to
  write a portable file or archive to any user-selected local destination.
- The current folder dropdown plus free-form "new folder" field is a temporary
  fallback and should be replaced by the tree picker/native Save flow.

Safety rules:

- Import preview before destructive merge.
- Export should never require network access.
- Export should be readable outside this app.
- Import should preserve original Markdown as much as possible.

## 7. Relationship With RAG

Notebook is not a Knowledge Base and should not become a vectorized RAG corpus.
It remains a Markdown/plain-text workspace.

Recommended order:

1. Notebook links, backlinks, tags, import/export.
2. Search and `@` references over Notebook content.
3. Shared chat source association that can select either Notebook plain-text
   lookup or one LanceDB-backed Knowledge Base.

This keeps personal notes portable, inspectable, and editable while Knowledge
Base remains the place for embeddings, chunking, LanceDB storage, and RAG
retrieval.

## 8. Implementation Phases

### Phase 1: Link and Tag Index

- [x] Parse `[[wiki links]]` from Notebook Markdown.
- [x] Parse tags from Markdown.
- [x] Add derived link/tag/backlink view helpers without changing durable Notebook storage.
- [x] Add backend tests for link parsing, tag parsing, metadata tag merge, link resolution, and backlinks.
- [x] Show outgoing links and backlinks in note detail.
- [x] Add unresolved-link click-to-create flow.
- [x] Parse tags from note metadata and merge them with Markdown tags.

### Phase 2: Import

- [x] Add Markdown file import API.
- [x] Add zip import API.
- [x] Parse YAML frontmatter.
- [x] Add import preview with duplicate/conflict detection.
- [x] Add Notebook UI import action.
- [x] Add tests for Markdown/frontmatter/link preservation.
- [x] Show skipped-file details in the final import result UI.
- [x] Add batch Notebook entry creation so import persists once per operation.
- [x] Add desktop native folder import for Obsidian Vault directories.
- [x] Report unsupported Obsidian attachments/assets during import.
- [x] Add desktop native Save dialog for saving generated notes/research
  reports into a bound external Notebook Vault, restricted to Markdown paths
  inside that Vault.
- [x] Replace the save-to-Notebook folder dropdown with a Notebook tree picker
  for non-native or app-local Vault saves.
- [x] Add root/existing-folder selection, in-place folder creation,
  expand/collapse state, last-folder restoration, and final-path preview to the
  Notebook tree save flow.
- [x] Validate destination conflicts and provide an open-saved-entry action
  after a successful generated-content save.
- [ ] Keep native local export commands separate from Save to Notebook in UI
  labels, behavior, and tests.

### Phase 3: Export

- [x] Add single-note Markdown export.
- [x] Add selected-notes export.
- [x] Add whole-Notebook zip export.
- [x] Include frontmatter in exported notes.
- [x] Add Notebook UI export action.
- [x] Add tests for stable exported filenames and metadata.

### Phase 4: Agent-Assisted Organization

- [x] Add Chat composer source association for No source / Notebook / Knowledge Base.
- [x] Keep Notebook association plain-text only and separate from Knowledge Base ids.
- [x] Add Organize mode as a Chat workflow.
- [x] Add `search_notebook` product tool for plain-text Notebook lookup.
- [x] Demote code execution from visible Chat mode to a tool available inside modes.
- [x] Add "suggest links" proposal workflow through structured edit proposals.
- [x] Add "suggest tags" proposal workflow through structured edit proposals.
- [x] Add "merge duplicate notes" proposal workflow through structured edit proposals.
- [x] Keep apply/reject explicit.
- [x] Record applied organization changes as Notebook memory events.

Phase 4 first implementation uses complete Markdown replacement plus structured
proposal metadata (`proposal_kind`, suggested links, suggested tags, and merge
source ids). More specialized apply APIs can be added later if this becomes too
blunt.

### Phase 5: Graph and Advanced Portability

- [x] Add local graph for current note.
- [x] Add unresolved link review.
- [x] Add Obsidian-compatible vault export.
- [x] Evaluate file-backed vault mode.
- [x] Implement the first file-backed vault storage slice.
- [x] Store note bodies as individual Markdown files under `notebook/vault/`.
- [x] Store Notebook ids, paths, types, source metadata, and timestamps in
  `notebook/index.json`.
- [x] Prefer imported source file-name stems over `frontmatter.title` for entry
  titles.
- [x] Preserve imported relative folder paths in entry paths and zip exports.

File-backed vault mode first implementation is now active. The remaining gaps
are external file watching, attachment storage, link updates on path rename,
and a richer conflict-resolution UI for path collisions.

### Phase 6: Editor-Style Notebook Performance

- [x] Split Notebook tree/list responses from full note detail responses.
- [x] Remove Markdown bodies from the file browser/list payload.
- [x] Avoid automatic full Vault rescans on every Notebook tab open.
- [x] Add explicit `refresh`/`reconcile` API for the Vault index.
- [x] Track file stats in the index so unchanged files are skipped.
- [x] Add a desktop file watcher for bound Vault directories.
- [x] Debounce watcher events and batch index writes.
- [x] Keep folder expansion and selected note state across tab switches.
- [x] Add visible indexing status and last-refresh information.
- [x] Add virtualized visible-row rendering for the Notebook file tree.

## 9. Open Questions

- Should note titles be globally unique inside one Space?
- Should unresolved `[[links]]` create placeholder notes automatically or only on click?
- Should imported folders become tag prefixes, source metadata, or both?
- Should attachments/assets live inside Notebook storage or a shared Space asset store?
- Should Notebook export include Memory references, or only note content?
- Should the first watcher implementation be desktop-only, or should server
  deployments use periodic refresh plus explicit user refresh?
