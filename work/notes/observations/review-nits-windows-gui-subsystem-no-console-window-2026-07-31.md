---
title: review-gate non-blocking nits for 'windows-gui-subsystem-no-console-window' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-gui-subsystem-no-console-window
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-gui-subsystem-no-console-window' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- AttachConsole(ATTACH_PARENT_PROCESS) also ATTACHES werust to that console's lifetime: closing the launching terminal (or Ctrl+C in it) now sends a console control event to werust and terminates the browser, which a GUI-subsystem process that never attached would have survived. Is that trade-off intended, and should it be recorded in DECISIONS.md and added to the manual-verification list (launch from cmd, close cmd, does the window survive)?
  (crates/werust-windows/src/startup.rs attach_parent_console; DECISIONS.md sections 2 and 4 discuss redirection and AllocConsole but never console lifetime; README 'What still awaits real Windows hardware' steps 1-4 do not cover it)
- DECISIONS.md section 6 rejects a CI PE-header subsystem check because it 'needs a new workflow step on the default branch before it can be dispatched', but the release job already runs docs/spikes/windows-release-packaging-leg/check-windows-artifact.sh over the built exe in an EXISTING step, and that script already reads the PE bytes. Adding a subsystem-field assertion there is an edit to an existing script, not a new step. Should the strongest available guard be taken now rather than deferred?
  (.github/workflows/release.yml 'BUILD-leg check: a real exe...' step; check-windows-artifact.sh already does magic()/contains() over the PE)
- Ratify the user-visible default change: the Windows startup banner now prints ONLY when a console was attached, so a double-clicked launch prints nothing and Windows diverges from the GTK and AppKit shells, which print it unconditionally. Recorded in DECISIONS.md section 3; the human should confirm the divergence is wanted.
  (crates/werust-windows/src/main.rs run(): println! guarded by 'if console'; crates/werust/src/main.rs:157 and crates/werust-macos/src/main.rs:25 print unconditionally)
- Ratify the exclusive surface rule (stderr XOR message box): a launch with NO parent console but a redirected stdout (for example PowerShell Start-Process -RedirectStandardOutput, or a scheduled task) yields console=false, so the banner is suppressed even though stdout leads to a file, and a startup failure raises a MODAL dialog in a context where nobody is there to dismiss it, which is the very objection DECISIONS.md raises against 'always both'.
  (startup.rs attach_parent_console returns stdout AND stderr usable; main.rs picks report_startup_failure when false; DECISIONS.md section 2 'Rejected: always both ... would block a scripted run')
