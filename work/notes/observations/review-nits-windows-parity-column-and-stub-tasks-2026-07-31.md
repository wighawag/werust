---
title: review-gate non-blocking nits for 'windows-parity-column-and-stub-tasks' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-parity-column-and-stub-tasks
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-parity-column-and-stub-tasks' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify pointing the stubbed follow-os-color-scheme cell at the EXISTING windows-release-packaging-leg task instead of cutting a dedicated manifest task, and ratify that this diff added an acceptance criterion to that unclaimed backlog task (flip the cell when the manifest lands and the manual check confirms). This is a cross-task interaction: the packaging task is now load-bearing for a matrix cell.
  (docs/platform-capability-matrix.toml follow-os-color-scheme windows cell; work/tasks/backlog/windows-release-packaging-leg.md new paragraph + new AC; rationale in decision 1 of the decisions note. ADR-0005 linkage nuance says a stub should name a task that genuinely covers ITS completion, which the added AC satisfies.)
- Ratify the meaning of implemented used here: wired-and-shape-guarded rather than witnessed on Windows. Seven cells (spa-url-tracking, back-forward, retrieval-backend, scheme-less-entry-routing, blank-window-open-navigates-in-place, ipfs-web-pathing-fallback, ipfs-redirects-3xx-navigation) rest on compilation plus the source-shape guard plus shared-core tests, each stating its limit inline. This is the SECOND instance of the same ratification the macOS column needed, so it is now a standing standard worth one human yes.
  (decision 3 of work/notes/observations/windows-parity-column-decisions-2026-07-31.md; identical nit recorded at work/notes/observations/review-nits-macos-parity-column-and-stub-tasks-2026-07-31.md)
- The decisions note claims each of the FOUR task bodies names its sibling; that is not what landed. Only the two NEW Windows tasks name their macOS sibling by slug. macos-trust-surface-bless-affordance.md still says the windows column does not exist yet and tells its builder not to touch the Win32 window because of that, which this diff made stale. One line in each macOS task would close it.
  (work/tasks/backlog/macos-trust-surface-bless-affordance.md:16 and its Prompt; work/tasks/backlog/macos-debug-network-capture-main-document-and-scheme-handled.md names Windows only generically. Behavioural guidance in both is still correct; only the rationale is stale.)
- Coherence: the desktop platform key now names one of THREE desktops and is still not pinned in the CONTEXT.md glossary. The macOS review explicitly asked for the rename-to-linux versus pin-desktop decision to be made BEFORE a third desktop column landed; it has now landed. The task correctly forbade the rename as a side-effect, so this is a human decision to schedule, not a defect in the diff.
  (docs/platform-capability-matrix.toml platforms line and header note (updated to say three desktops); work/notes/observations/desktop-platform-key-now-means-linux-only-2026-07-31.md; CONTEXT.md still has no platform-key entry)
- Nothing on the work board OWNS the human-on-a-Windows-box sweep the unwitnessed halves defer to (HiDPI, input and focus routing, window management, the debug view row colours). They point only at manual steps in the spike README. windows-release-packaging-leg owns just the two manifest re-checks. This is the same gap macOS has, now on a third platform: is prose-only acceptance deliberate, or should a manual-sweep task exist?
  (docs/spikes/windows-win32-window-and-chrome/README.md section on what still awaits real Windows hardware; stubbed is reserved for wiring gaps so an unwitnessed-but-wired cell has no board slot)
- The web-inspector cell describes the F12 reach (URL-bar subclass forwarding when the chrome has focus) but carries no HONEST LIMIT sentence for it, unlike its neighbours. The spike lists keyboard routing as unmeasured and puts F12 at manual step 8; the MEASURED sentence is correctly scoped to OpenDevToolsWindow, but a fast reader will take the whole cell as witnessed.
  (docs/platform-capability-matrix.toml web-inspector windows comment; docs/spikes/windows-win32-window-and-chrome/README.md check 8 and manual step 8)
- Small pre-existing contradiction the new cell surfaces: the matrix says Windows has no devtools entry in the menu, deliberately, while crates/werust-windows/src/debugview.rs:8 says real Chrome DevTools are one menu entry away here. The matrix is the accurate one; the source doc comment should be corrected by whoever next touches that file.
  (crates/werust-windows/src/debugview.rs:8 versus the web-inspector windows cell (not part of this diff))
