---
title: Gate-3 verdict — fix-mobile-chrome-urlbar-crowded-by-buttons — APPROVE
date: 2026-07-22
kind: observation
reviewOf: fix-mobile-chrome-urlbar-crowded-by-buttons
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main)

Roadmap item #1 (the mobile URL-bar-crowding fix). `do` ran Gate-1 + Gate-2 green.

### Acceptance criteria — met

- ✅ Android: nav buttons are compact (`minWidth=0`/`minimumWidth=0`, zero padding, fixed
  square) so the URL `EditText` (`weight=1f`) fills the MAJORITY of the toolbar row; the row
  keeps `minimumHeight=48dp` for the touch target.
- ✅ iOS: the URL `UITextField` hugs weakly + low compression-resistance (stretches first =
  majority width); the buttons hold `.required` content-hugging (intrinsic/compact width).
- ✅ Back/Forward/Reload/Stop all remain present + tappable; Stop enabled while loading,
  Reload while idle (driven by the `werust-core` chrome's load state) — consistent with the
  desktop four-button enable/disable model on both edges.
- ✅ No navigation behaviour change; Rust gate green.

### Nit triage

1. The no-merge (keep four buttons) decision is filed under `notes/observations/` but is
   ADR-shaped (a decision WE made with rationale). Sound + reversible; re-home to `docs/adr/`
   at some point. Non-blocking.
2. **Android button is 40x40dp, under Material's 48dp** — the ROW is 48dp (vertical target
   fine) but the button box is 40dp. Real, cheap accessibility nit; acceptance said ">= ~48dp
   effective" so it squeaks by, but bumping `NAV_BUTTON_DP` toward 44-48 is the right call.
   Captured as the follow-up below (primary bug \u2014 crowded URL bar \u2014 is fixed; not reopening).

### Follow-up captured (small, not reopened)

Bump Android `NAV_BUTTON_DP` 40 -> ~44-48 for Material touch-target compliance, and re-home
the keep-four-buttons decision note as an ADR. Low priority; the shipped fix already makes
the URL bar legible (the goal). Fold into the next mobile-chrome touch if there is one.

### Result

The mobile URL bar is now the widest, readable toolbar element \u2014 testability of every
future feature (ENS, privacy, etc.) on mobile is improved, which was the point of doing this
first.
