# Desktop Release QA Checklist

This checklist applies to the current desktop release, including `v0.4.2`.

Use this checklist after running:

```powershell
.\scripts\build-desktop.ps1
.\scripts\qa-desktop.ps1
```

## Environment

- Date:
- Release version/tag:
- Windows version:
- Build target:
- Artifact path:
- Clean profile or clean app data directory:

## Manual Checks

- [ ] Install or unpack the app on a clean Windows profile or after clearing the app data directory.
- [ ] Start the app without running `cargo run`, `cargo tauri dev`, or `npm run dev`.
- [ ] Confirm the app opens the React UI and does not show Vite proxy errors.
- [ ] On Windows, confirm the native title bar remains, its upper-left caption
      icon and title are not visible, and system drag, Snap Layouts,
      double-click maximize/restore, minimize, maximize/restore, close, keyboard
      behavior, and edge resize all work.
- [ ] Switch between cool-light and graphite-dark and confirm the native title
      bar follows the selected theme without revealing the caption text.
- [ ] Confirm Settings shows the desktop data directory and the Open button opens it.
- [ ] Change one setting, restart the app, and confirm it was restored from `settings.json`.
- [ ] Configure one LLM provider.
- [ ] Send a chat message and confirm streaming output.
- [ ] Confirm the Assistant header does not duplicate the composer’s source selector and only keeps the compact Temporary Chat control; no USD cost or session-budget UI remains.
- [ ] Configure one embedding provider.
- [ ] Create a knowledge base and upload a text file.
- [ ] Create a knowledge base and upload a PDF file.
- [ ] Ask a Knowledge-grounded question and confirm citation links appear only
      after `knowledge_search` followed by `knowledge_read`.
- [ ] Ask Chat to research a sourced topic, verify citations, and save the answer to Notebook.
- [ ] Reference one exact Note in Assistant and confirm the agent reads it on demand.
- [ ] Create, edit, move, delete, and restore a Note; confirm all paths remain inside the configured Vault.
- [ ] Turn Memory off and confirm a new conversation neither recalls nor proposes long-term Memory.
- [ ] Confirm Memory defaults to the Long-term Memory tab, then switch to Assistant Profile, change the name and behavior instructions, and verify a new conversation uses them while Settings has no duplicate Assistant tab.
- [ ] Confirm Saved Memory, assistant-initiated writes, and History Recall switches are aligned in one Memory settings card, and the Memory items search placeholder has no leading icon overlap.
- [ ] Turn Memory on, review one item, edit it, then forget it.
- [ ] With Saved Memory and History Recall enabled, save a preferred name, start a new conversation, ask the short follow-up `我呢`, and confirm the agent searches Saved Memory and reads the exact result before answering instead of claiming not to know.
- [ ] Mention a transient event in one conversation, start another conversation, ask indirectly about that event (for example `我早上吃了什么`), and confirm the agent searches History Recall without needing a reminder. If the first query misses, confirm it retries once with simpler content keywords or a federated search.
- [ ] For both recall checks, confirm the tool trace remains visible but the answer does not narrate `我查一下记忆` or equivalent implementation steps.
- [ ] Confirm Memory contains no legacy Tutor/Quiz migration or archive controls.
- [ ] Confirm requests to `/api/migration/legacy` return `404 Not Found`.
- [ ] Close and restart the app.
- [ ] Confirm sessions, Sources, Notes, and Memory still exist after restart.
- [ ] Record the child `tutor-web` PID, close the app normally, and confirm that
      PID exits and its local port is released within five seconds.
- [ ] Start the app again, force-stop only the desktop parent process, and
      confirm the child `tutor-web` PID exits within five seconds.
- [ ] Inspect visible logs and trace output for accidental API key exposure.

## Result

- [ ] Pass
- [ ] Fail

Notes:
