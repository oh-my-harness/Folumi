# Desktop Release QA Checklist

This checklist applies to the current desktop release, including `v0.3.5`.

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
- [ ] Configure one embedding provider.
- [ ] Create a knowledge base and upload a text file.
- [ ] Create a knowledge base and upload a PDF file.
- [ ] Ask a Knowledge-grounded question and confirm citation links appear only
      after `knowledge_search` followed by `knowledge_read`.
- [ ] Run a Research task from Assistant and save the resulting report as a Note.
- [ ] Reference one exact Note in Assistant and confirm the agent reads it on demand.
- [ ] Create, edit, move, delete, and restore a Note; confirm all paths remain inside the configured Vault.
- [ ] Turn Memory off and confirm a new conversation neither recalls nor proposes long-term Memory.
- [ ] Turn Memory on, review one item, edit it, then forget it.
- [ ] Preview legacy Tutor continuity, import one selected item twice, and confirm Assistant Continuity contains only one copy.
- [ ] Download the legacy archive and confirm it contains any existing Tutor definitions and Quiz data without reactivating either feature.
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
