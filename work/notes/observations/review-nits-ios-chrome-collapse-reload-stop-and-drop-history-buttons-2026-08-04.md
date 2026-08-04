---
title: review-gate non-blocking nits for 'ios-chrome-collapse-reload-stop-and-drop-history-buttons' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: ios-chrome-collapse-reload-stop-and-drop-history-buttons
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'ios-chrome-collapse-reload-stop-and-drop-history-buttons' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the iOS bundle check now enforces a LIST of required C-ABI symbols and that same script runs in the RELEASE job, so a release artifact missing _werust_ios_activate_reload_stop_control now fails the release leg too. Is that the intended second consumer, and is a per-control symbol inventory (which grows with every future control) the right shape for a script whose stated job was proving the core got linked at all?
  (docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh required_symbols array; recorded by the agent in docs/spikes/ios-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md section 6)
- Ratify an unrecorded in-scope decision: the spinner is removed from the VoiceOver tree (isAccessibilityElement = false, accessibilityElementsHidden = true). It matches the Android TalkBack choice so it is coherent, but it is a user-visible accessibility default that lives only in a Swift comment; DECISIONS.md section 2 (which README device-step 6 cites for it) covers the slot and alpha, not accessibility, and no assertion pins it.
  (crates/werust-ios/App/Sources/WKWebViewShellController.swift layoutChrome, spinner setup; README device-step 6 points at DECISIONS.md section 2)
- The new guard says the scanner is the FOURTH near-identical copy and is filed as work/notes/observations/kotlin-source-scanner-duplicated-across-edge-guards-2026-08-04.md, but that append-only note still says at least THREE and does not list the iOS copy. Append the fourth so the note a reader lands on matches the code that cites it.
  (crates/werust-ios/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs doc comment on scan(); the note lists only the two Android guards plus mobile_chrome_presentation_shape.rs)
- Ratify: werust_ios_reload / werust_ios_stop and the Swift reload() / stop() bindings are now unreachable from the app (the guard forbids the painter calling them). Keeping them is deliberate and argued as the C-ABI being a mechanical mirror of CoreSession; confirm that is the standing rule rather than dead surface to sweep.
  (crates/werust-ios/Sources/werust_mobile.h; DECISIONS.md section 3 sub-decision)
