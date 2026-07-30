---
title: review-gate non-blocking nits for 'windows-win32-window-and-chrome' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: windows-win32-window-and-chrome
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-win32-window-and-chrome' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The macOS-from-Linux type-check harness is BROKEN by this extraction and nothing gates it: docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh still does ln -sf $REPO/crates/werust-macos/src/paint.rs (a file this PR deletes, so the symlink dangles) and its scratch window Cargo.toml lists only renderer, werust-core and macos-renderer, so the real werust-macos/src/lib.rs line 'pub use desktop_paint as paint;' cannot resolve and cargo clippy -p werust-macos in that scratch workspace fails. The Windows sibling harness WAS updated in this PR; this one was missed, and no backlog task owns it (macos-spike-doc-accuracy-and-harness-guard covers other items). This repo writes all Apple/Windows code blind from Linux, so the next macOS task hits it immediately. Fix it here (add the desktop-paint path dep to the scratch manifest and drop or repoint the paint.rs symlink) or file it explicitly?
  (docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh:529-561 vs crates/werust-macos/src/lib.rs (pub use desktop_paint as paint) and the deleted crates/werust-macos/src/paint.rs)
- Ratify an in-scope engine default this task changed: crates/windows-renderer now calls SetAreDevToolsEnabled(cfg!(debug_assertions)), so a RELEASE build of the Windows engine is no longer inspectable where WebView2 previously defaulted it to on. It is recorded (DECISIONS.md 3b) and it follows the web-inspector capability rule the other platforms already apply, but it is a user-visible behaviour change inside another task's crate. Ratify?
  (crates/windows-renderer/src/backend.rs, settings block; DECISIONS.md 3b)
- Ratify the second engine change: the container's own window proc now handles WM_SIZE and calls SetBounds on a controller BORROWED through GWLP_USERDATA (raw pointer, ManuallyDrop, cleared in Drop before Close). It is the right layer (no seam widening) and is recorded in DECISIONS.md 3a, but the resize path itself is only exercised by the initial layout on CI, so the borrowed-pointer teardown ordering has never been driven by a real resize. Accept as recorded, with resize on the awaits-hardware list?
  (crates/windows-renderer/src/backend.rs wndproc WM_SIZE and Drop; README section What still awaits real Windows hardware)
- Ratify a CI-cost decision that is not in DECISIONS.md: crates/desktop-paint/** was added to the pull_request path filter of BOTH windows-renderer.yml and macos-renderer.yml, so any PR touching the shared painter half now gates on a Windows AND a macOS runner. That widens the deliberately narrow PR filter to a crate that is not platform-shaped. It is explained in the workflow header comment, but the narrow-filter guard (windows_renderer_leg_shape.rs) does not pin it, so a later broadening has no test to change. Ratify or restrict to push?
  (.github/workflows/windows-renderer.yml and macos-renderer.yml pull_request paths; crates/werust-core/tests/windows_renderer_leg_shape.rs the_pull_request_filter_stays_narrow_and_push_carries_the_rest)
- Coherence of the new crate NAME: desktop-paint is glossed in CONTEXT.md as the half shared by NATIVE-WIDGET edges (AppKit, Win32) and the GTK desktop is deliberately NOT a consumer, yet the name says desktop. A later author adding a desktop edge, or reworking GTK, will reasonably assume it belongs there. Ratify the name or rename it to something that says native-widget (the webview-shared analogue it is modelled on names its layer, not its form factor)?
  (crates/desktop-paint/Cargo.toml, CONTEXT.md glossary entry for chrome presentation / painter, DECISIONS.md 2)
- Ratify a user-visible default: shipping without a comctl32 v6 manifest means the chrome draws classic-styled on Windows 11 and system-drawn push BUTTONs do not follow dark mode, so a dark-mode window has dark surfaces and light buttons, a partial ADR-0009 gap. It is recorded (DECISIONS.md 4 and 6, README awaits-hardware) and owned by the authored follow-on windows-release-packaging-leg, and nothing ships to users until that leg lands. Ratify the deferral?
  (docs/spikes/windows-win32-window-and-chrome/DECISIONS.md sections 4 and 6; work/tasks/backlog/windows-release-packaging-leg.md acceptance criteria)
- The repo README has no Windows section at all (no mention of the word Windows), while the macOS sibling task added a 'The macOS shell (werust-macos)' section describing how to run it and pointing at its spike README. A person landing on the repo cannot discover cargo run -p werust-windows. Add the parallel section?
  (README.md line 22 (macOS shell section) versus no windows-renderer / werust-windows entry)
- Small claim mismatch in the spike README: it records the local cross-target run as cargo xwin clippy -p werust-windows -p windows-renderer --tests --examples, but the committed harness runs cargo xwin check (typecheck-windows-from-linux.sh ends in exec cargo xwin check). Either the recorded command was run by hand outside the harness (say so) or the harness should be the clippy it claims.
  (docs/spikes/windows-win32-window-and-chrome/README.md, section What the LOCAL type-check proves; docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh final exec)
