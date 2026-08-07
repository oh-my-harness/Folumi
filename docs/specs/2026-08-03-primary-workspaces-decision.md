# Folumi Primary Workspaces Decision

> Status: Accepted
>
> Decision date: 2026-08-03
>
> Amended: 2026-08-03 — Assistant profile configuration moved from Settings
> to Memory by explicit product request.
>
> Amended: 2026-08-03 — Memory uses separate Long-term Memory and Assistant
> Profile tabs, with Long-term Memory as the default.
>
> Amended: 2026-08-03 — The obsolete layered Memory pipeline was retired and
> replaced by the first Saved Memory implementation; Assistant Profile remains
> a separate tab.
>
> Amended: 2026-08-03 — The replacement proposal now defines global Saved
> Memory and opt-in History Recall with explicit conflict/expiry rules. Saved
> Memory and the opt-in runtime History Recall baseline are implemented.
> Runtime Sessions, rather than a product
> workspace entity, own conversation-local continuity.
>
> Amended: 2026-08-07 — User-facing navigation now describes product intent
> rather than storage implementation: Knowledge Base is labeled **Sources**
> (中文“资料”), Memory is labeled **Personalization** (中文“个性化”), and
> Notebook remains **Notebook** (中文“笔记”). Internal domain and API names do
> not change.
>
> Scope: Primary navigation and the product boundaries of Knowledge Base,
> Notebook, Memory, and Settings

## Decision

Folumi has five primary workspaces:

1. **Assistant** — conversations and agent tasks.
2. **Sources** — read-only source documents and their RAG indexes; the
   internal domain remains Knowledge Base.
3. **Notebook** — recording, organizing, reading, and editing user-owned
   Markdown notes.
4. **Personalization** — configuring the single Assistant profile and
   enabling, reviewing, editing, and forgetting durable personal context; the
   internal domain remains Memory.
5. **Settings** — provider, retrieval, storage, governance, and appearance
   configuration.

This decision supersedes the earlier contraction assumptions that:

- Knowledge Base and Notebook should share one combined page;
- Memory should be available only inside Assistant settings; and
- the product should expose only Assistant, Knowledge Base, and Settings as
  primary navigation.

Those assumptions must not be restored by a UI cleanup, navigation refactor,
or component consolidation without a new explicit product decision.

## User-facing vocabulary

The interface distinguishes the three durable information domains by user
intent:

- **Sources / 资料**: external material the Assistant may retrieve, quote, and
  verify. A collection is labeled **Source collection / 资料集**.
- **Notebook / 笔记**: Markdown content the user writes, organizes, and owns.
- **Personalization / 个性化**: how the Assistant knows the user, references
  past conversations, and presents itself.

Personalization uses two tabs: **About me / 关于我** for personal information
and conversation continuity, and **Assistant setup / 助手设定** for identity
and behavior. Within About me, Saved Memory is labeled **Personal
information / 个人信息**, and History Recall is labeled **Reference past
conversations / 参考过往对话**.

These labels do not rename runtime contracts, database tables, API routes,
tool names, or code-level `KnowledgeBase`, `Notebook`, and `Memory` types.

## Required boundaries

### Sources (internal domain: Knowledge Base)

- Contains only Sources used for retrieval-augmented generation.
- Owns source import, processing state, duplicate detection, retry, index
  rebuild, search, preview, citation, and original-source navigation.
- Does not embed editable Notebook notes or Personalization management in its page.
- Does not gain write access to the original source material through Notebook
  tools.

### Notebook

- Is a standalone primary workspace and a core Folumi capability.
- Supports recording, managing, viewing, editing, moving, deleting, and restoring
  Markdown notes in user-selected local folders.
- Keeps notes user-owned and separate from Knowledge Base source documents.
- May be selected or referenced from Assistant, but that integration must not
  make Notebook a subsection of Knowledge Base.

### Personalization (internal domain: Memory)

- Is a standalone primary workspace.
- Uses two page-level tabs instead of one mixed form: **About me** is the
  default and owns personal information, conversation continuity, and their
  controls; **Assistant setup** owns the name and behavior instructions used
  by new conversations.
- Does not contain legacy Tutor/Quiz migration, import, or archive controls.
- Does not automatically capture Assistant, Notebook, or Sources
  activity. Saved Memory is created by direct user action or by a separately
  confirmed assistant mutation.
- Keeps provenance visible when available.
- Continues to use runtime access control and mutation gates; a more prominent
  UI does not expand agent authority.
- Must not depend on navigating through Settings as its only user-facing entry.

### Settings

- May configure the Notebook Vault location and other storage behavior, but
  note content is managed in Notebook.
- Does not expose a duplicate Assistant setup; Assistant identity, behavior,
  personal information, and its controls are managed in Personalization.
- Must not become a substitute content workspace for Notebook or Personalization.

## Data ownership

The navigation decision does not merge storage models:

| Domain | Authority | Mutation rule |
| --- | --- | --- |
| Sources | Original imported documents | Read-only to Assistant tools; indexes are rebuildable derivatives |
| Notes | User-owned Markdown | Explicit UI edits or bounded Notebook tools with revision checks |
| Assistant profile | Product settings | User-managed only from Personalization; applied to new runtime sessions |
| Memory | Runtime-backed user and assistant continuity records | Visible user control plus runtime access and mutation policy |
| Sessions | Runtime sessions | Conversation lifecycle and runtime persistence |

Assistant may use explicitly selected Sources or Notebook notes and may use
enabled Personalization data through the internal Knowledge Base and Memory
contracts. Cross-workspace use does not permit silent data copying or a shared
undifferentiated document store.

## Acceptance criteria

A change conforms to this decision only when all of the following remain true:

- the sidebar exposes Assistant, Sources, Notebook, Personalization, and
  Settings, using 资料、笔记、个性化 in Chinese;
- Sources has no Notes tab and no Personalization management surface;
- Notebook can be opened directly and supports its normal note-management
  workflow without first opening Sources;
- Personalization can be opened directly, defaults to About me, exposes a
  separate Assistant setup tab, and provides visible Personal information
  controls;
- Reference past conversations (internal History Recall) remains visibly
  separate and disabled by default, uses the runtime cross-session boundary
  when enabled, and never promotes recalled
  turns into Saved Memory; Saved Memory remains global, while Session IDs are
  provenance and recall boundaries rather than Memory scopes;
- source links open Sources, while note links open Notebook;
- Settings does not contain Assistant setup or the primary Notebook/
  Personalization content-management experience; and
- onboarding, help, README, manual, and product requirements describe the same
  information architecture.

The frontend product-shell regression test should enforce these structural
requirements. Desktop QA should verify the same behavior interactively.

## Change control

Changing any boundary above requires an explicit product decision. The change
must update this record (or replace it with a clearly linked superseding
decision), the product requirements specification, user documentation, and
regression tests in the same change. Implementation convenience alone is not a
reason to merge these workspaces.
