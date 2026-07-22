---
title: Fix mobile chrome layout so the URL bar is not crowded out by the toolbar buttons
slug: fix-mobile-chrome-urlbar-crowded-by-buttons
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: [2, 18]
---

## What to build

Fix a real shipped mobile UI bug: on the mobile shells the Back / Forward / Reload / Stop
buttons take most of the horizontal space in the single toolbar row, leaving the URL bar
mostly hidden and hard to read/edit. Rebalance the layout so the URL bar gets the majority of
the width and the nav controls are compact.

### Android (`crates/werust-android/app/.../BrowserActivity.kt`) \u2014 the worst offender

Today the toolbar is one horizontal `LinearLayout` with four DEFAULT `Button`s (`\u25c0 \u25b6 \u23f3 \u2715`),
each `WRAP_CONTENT` (default Android buttons have large min-width + padding), added BEFORE the
URL bar (`EditText`, `weight=1f`). Four fat buttons squeeze the weighted URL bar.

Fix (choose the cleanest that works; the goal is a legible URL bar):
- Make the nav buttons COMPACT: small fixed width (e.g. ~40-44dp square, touch-target-safe),
  minimal padding (`minWidth=0`, `minimumWidth=0`, tight horizontal padding), so four of them
  take a small, fixed slice, not most of the row. Consider `ImageButton`/icon styling or at
  least strip the default button chrome/insets.
- Keep the URL bar `weight=1f` so it EXPANDS to fill the remaining space (now the majority).
- Optional but recommended: drop STOP from the always-visible row (or merge Reload/Stop into
  ONE button that shows \u23f3 when idle and \u2715 while loading \u2014 the core already knows load state),
  reclaiming width. Back/Forward/Reload + URL bar is the essential set; Stop can be the
  reload-button's loading state.
- Ensure touch targets stay >= 48dp effective (use padding/min-height on the row) even as the
  visible button shrinks, so it is still tappable.

### iOS (`crates/werust-ios/App/Sources/WKWebViewShellController.swift`) \u2014 same class of issue

The iOS shell lays out the same URL field + four `UIButton`s. Apply the equivalent fix: the
`UITextField` should get the majority width (e.g. it is the flexible member of the horizontal
stack / has the low content-hugging + high stretch priority), and the four system buttons stay
compact (fixed/intrinsic width, not stretched). Same optional Reload/Stop merge.

Keep BOTH edges consistent in behaviour (same button set + the reload/stop merge if you do it),
since both drive the same `werust-core` chrome.

## Acceptance criteria

- [ ] On a normal phone width, the URL bar is clearly the widest element in the toolbar and its text/hint is readable and editable (not squeezed to a sliver).
- [ ] Back / Forward / Reload (and Stop, or a merged Reload/Stop) remain present and tappable, with touch targets >= ~48dp effective on Android (HIG-appropriate on iOS).
- [ ] Android: nav buttons are compact (no default oversized button width/padding); the URL `EditText` keeps `weight=1f` and visibly fills the remaining space.
- [ ] iOS: the URL `UITextField` takes the majority width; the buttons stay at intrinsic/compact width (correct content-hugging / stretch priorities).
- [ ] No behaviour change to navigation: typing a URL + Go still navigates via the core; Back/Forward/Reload/Stop still drive the core and reflect its chrome state. (If Reload/Stop are merged, the merged button shows reload when idle and stop while loading, driven by the core's load state.)
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` pass (this is mostly Kotlin/Swift UI layout; keep the Rust core + its tests unchanged/green).

## Prompt

> Goal: make the mobile URL bar legible. On the mobile shells the Back/Forward/Reload/Stop
> buttons eat most of the toolbar row, hiding the URL bar. On Android
> (`BrowserActivity.kt`) the four default `Button`s are oversized \u2014 make them COMPACT
> (small fixed square width, `minWidth=0`, tight padding, icon-style) so the weighted
> (`weight=1f`) URL `EditText` fills the majority of the row; keep touch targets >= ~48dp.
> On iOS (`WKWebViewShellController.swift`) give the `UITextField` the majority width
> (content-hugging/stretch priorities) with the buttons at intrinsic width. Recommended:
> merge Reload/Stop into one button that shows reload when idle and stop while loading
> (the `werust-core` chrome already exposes load state), reclaiming width. No navigation
> behaviour change; both edges stay consistent (same core). Keep the Rust gate green.
>
> Done = on a phone the URL bar is the widest, readable toolbar element, nav controls stay
> compact + tappable, and navigation behaviour is unchanged.
