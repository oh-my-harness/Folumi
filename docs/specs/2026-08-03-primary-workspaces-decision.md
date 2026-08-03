# Folumi Primary Workspaces Decision

> Status: Accepted
>
> Decision date: 2026-08-03
>
> Scope: Primary navigation and the product boundaries of Knowledge Base,
> Notebook, Memory, and Settings

## Decision

Folumi has five primary workspaces:

1. **Assistant** — conversations and agent tasks.
2. **Knowledge Base** — read-only source documents and their RAG indexes.
3. **Notebook** — recording, organizing, reading, and editing user-owned
   Markdown notes.
4. **Memory** — enabling, reviewing, editing, forgetting, and migrating the
   assistant's long-term memory.
5. **Settings** — provider, retrieval, storage, governance, appearance, and
   assistant-profile configuration.

This decision supersedes the earlier contraction assumptions that:

- Knowledge Base and Notebook should share one combined page;
- Memory should be available only inside Assistant settings; and
- the product should expose only Assistant, Knowledge Base, and Settings as
  primary navigation.

Those assumptions must not be restored by a UI cleanup, navigation refactor,
or component consolidation without a new explicit product decision.

## Required boundaries

### Knowledge Base

- Contains only Sources used for retrieval-augmented generation.
- Owns source import, processing state, duplicate detection, retry, index
  rebuild, search, preview, citation, and original-source navigation.
- Does not embed editable Notebook notes or Memory management in its page.
- Does not gain write access to the original source material through Notebook
  tools.

### Notebook

- Is a standalone primary workspace and a core Folumi capability.
- Supports recording, managing, viewing, editing, moving, importing, exporting,
  deleting, and restoring Markdown notes.
- Keeps notes user-owned and separate from Knowledge Base source documents.
- May be selected or referenced from Assistant, but that integration must not
  make Notebook a subsection of Knowledge Base.

### Memory

- Is a standalone primary workspace.
- Contains the master switch and user-visible controls for inspecting, editing,
  forgetting, and migrating long-term memory.
- Keeps provenance visible when available.
- Continues to use runtime access control and mutation gates; a more prominent
  UI does not expand agent authority.
- Must not depend on navigating through Settings as its only user-facing entry.

### Settings

- May configure the Notebook Vault location and other storage behavior, but
  note content is managed in Notebook.
- May configure the Assistant profile, but Memory content and its master switch
  are managed in Memory.
- Must not become a substitute content workspace for Notebook or Memory.

## Data ownership

The navigation decision does not merge storage models:

| Domain | Authority | Mutation rule |
| --- | --- | --- |
| Sources | Original imported documents | Read-only to Assistant tools; indexes are rebuildable derivatives |
| Notes | User-owned Markdown | Explicit UI edits or bounded Notebook tools with revision checks |
| Memory | Runtime-backed user and assistant continuity records | Visible user control plus runtime access and mutation policy |
| Sessions | Runtime sessions | Conversation lifecycle and runtime persistence |

Assistant may use explicitly selected Knowledge Base sources or Notebook notes
and may recall enabled Memory. Cross-workspace use does not permit silent data
copying or a shared undifferentiated document store.

## Acceptance criteria

A change conforms to this decision only when all of the following remain true:

- the sidebar exposes Assistant, Knowledge Base, Notebook, Memory, and Settings;
- Knowledge Base has no Notes tab and no Memory management surface;
- Notebook can be opened directly and supports its normal note-management
  workflow without first opening Knowledge Base;
- Memory can be opened directly and exposes its master switch and item controls;
- source links open Knowledge Base, while note links open Notebook;
- Settings contains configuration rather than the primary Notebook or Memory
  content-management experience; and
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
