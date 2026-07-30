---
title: review-gate non-blocking nits for 'macos-parity-column-and-stub-tasks' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: macos-parity-column-and-stub-tasks
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-parity-column-and-stub-tasks' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the guard being STRENGTHENED: the task only forbade weakening, but the agent also hardcoded macos into the expected-platform list so the column cannot be silently dropped. Correct in my read, but it is an unasked-for change to a shared guard the sibling windows column also touches.
  (crates/werust-core/tests/platform_capability_parity.rs:297 for expected in [desktop, macos, ios, android]; rationale in work/notes/observations/macos-parity-column-decisions-2026-07-31.md section 3)
- Ratify the meaning of implemented used here: wired (ADR-0005 wording) rather than witnessed on a Mac. Six macOS cells rest on compilation plus shape guards plus shared-core tests with the runtime half unwitnessed (follow-os-color-scheme, spa-url-tracking, scheme-less-entry-routing, blank-window-open-navigates-in-place, ipfs-redirects-3xx-navigation, retrieval-backend). Each states its limit inline, and this repo has bounced macOS work before for prediction-instead-of-measurement, so the standard is worth a human yes.
  (docs/platform-capability-matrix.toml rows carry HONEST LIMIT prose pointing at manual steps 5/8/9 of docs/spikes/macos-appkit-window-and-chrome/README.md; the measured cells do map to real assertions in crates/werust-macos/examples/window_smoke.rs)
- Nothing on the work board OWNS the human-on-a-Mac sweep those unwitnessed halves defer to; they point only at README manual steps. Should a follow-up task carry it, or is prose-only acceptance deliberate?
  (stubbed is reserved for wiring gaps, so an unwitnessed-but-wired cell has no board slot; docs/spikes/macos-appkit-window-and-chrome/README.md section What still awaits a Mac)
- Coherence: the desktop platform key now means Linux/GTK only while a second desktop edge sits beside it (and windows is coming). The agent documented this in the file header, the guard comment and an observation note, but did not pin the term in the CONTEXT.md glossary. Decide rename to linux versus pin desktop before a third desktop column lands.
  (docs/platform-capability-matrix.toml platforms line; work/notes/observations/desktop-platform-key-now-means-linux-only-2026-07-31.md; CONTEXT.md has no platform-key entry)
- Drift into the sibling task: work/tasks/backlog/windows-parity-column-and-stub-tasks.md still asserts platforms = [desktop, ios, android] and carries no forward-pointer that the macos column landed. Its builder's drift-check should catch it, but a one-line update now is cheaper.
  (work/tasks/backlog/windows-parity-column-and-stub-tasks.md:12)
