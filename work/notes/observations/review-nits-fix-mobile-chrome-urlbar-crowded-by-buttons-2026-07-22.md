---
title: review-gate non-blocking nits for 'fix-mobile-chrome-urlbar-crowded-by-buttons' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: fix-mobile-chrome-urlbar-crowded-by-buttons
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-mobile-chrome-urlbar-crowded-by-buttons' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The Reload/Stop no-merge decision is recorded in work/notes/observations/ titled 'Decision:', but by bucket polarity a decision WE made with rationale is ADR-shaped (docs/adr/), not an observation. Ratify the decision (it is sound, reversible, keeps mobile consistent with the desktop enable/disable model) and consider re-homing the note as an ADR.
  (work/notes/observations/mobile-chrome-keep-four-buttons-no-reload-stop-merge.md; desktop crates/werust/src/main.rs uses set_sensitive on is_loading() for four separate buttons, confirming the cited model.)
- Android compact button is a fixed 40x40dp; the toolbar row has minimumHeight=48dp + CENTER_VERTICAL, so the ROW is 48dp tall but the tappable button itself is only 40x40 (8dp of the row height and the 40dp width are below Material 48dp). Acceptance says '>= ~48dp effective' — the '~' softens it, but the effective target is ~40dp. Confirm this is acceptable or bump NAV_BUTTON_DP toward 44-48.
  (BrowserActivity.kt compactNavButton uses LayoutParams(dp(40),dp(40)); TOUCH_TARGET_DP=48 applies to the row minimumHeight, not the button.)
