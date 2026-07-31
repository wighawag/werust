---
title: "The Win32 chrome draws at 96 DPI on a high-DPI display, so it is half-size: scale it, now that the manifest promises Windows we do"
slug: windows-chrome-must-scale-with-the-display-dpi
blockedBy: []
covers: []
---

## What to build

Found by the human on REAL Windows hardware, 2026-07-31, with a screenshot (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`): the page renders at the correct size but the werust chrome — toolbar, buttons, URL bar, trust indicator, status line — is drawn at roughly half size.

## The cause, which is ours and is one day old

`windows-release-packaging-leg` added `crates/werust-windows/app.manifest` declaring **`PerMonitorV2`** DPI awareness. That declaration is a PROMISE to Windows: do not bitmap-scale this process, it scales itself. The window does not keep that promise:

- `crates/werust-windows/src/win32.rs::ui_font()` hard-codes `CreateFontW(-15, …)` — a fixed pixel height.
- `crates/werust-windows/src/chrome.rs` uses raw 96-DPI metrics: `MARGIN: i32 = 8`, `PROGRESS_HEIGHT: i32 = 3`, `BUTTON_WIDTH`, `BADGE_WIDTH`, `TRUST_WIDTH`, the row height, the `+ 6` / `+ 2` gaps.
- `crates/werust-windows/src/window.rs` likewise: `DEFAULT_WIDTH: 1024` / `DEFAULT_HEIGHT: 768`, `DEBUG_WIDTH/HEIGHT`, `place(debug.clear, width - MARGIN - 90, MARGIN, 90, 26)`, `place(title, MARGIN, MARGIN, 300, 20)`, the `height - 40 - MARGIN` and `- 4` / `- 8` adjustments.

On a 150% or 200% display everything above comes out at 66% or 50% of its intended size, while WebView2 — which does its own DPI handling — draws the page correctly. Hence a correct page in a doll's-house chrome.

**Do not "fix" this by removing the manifest.** Reverting to unaware would restore correct SIZE by making Windows bitmap-scale the process, at the cost of a blurry chrome AND a blurry page, which is the outcome the manifest was added to prevent. Keep the promise instead.

## What to build

1. **Scale every chrome metric by the window's DPI.** Take the scale from `GetDpiForWindow(hwnd)` (per-monitor, not the system DPI) against the 96 baseline — `MulDiv(value, dpi, 96)` is the idiomatic spelling. Every hard-coded pixel in the three files above goes through it, including the font height and the initial window size. Prefer ONE helper (a `Dpi`/`scale()` seam) over scattering `MulDiv` at each call site: there are dozens of sites and a missed one is a subtly misaligned control rather than an obvious failure.
2. **Handle `WM_DPICHANGED`.** Dragging the window between monitors of different scale must re-scale it: use the suggested rect the message carries (`lparam`) with `SetWindowPos`, recreate the font at the new size, and re-run the layout. Without this the window is correct only on the monitor it opened on, which on a laptop-plus-external-monitor desk is most of the time.
3. **Recreate, do not just resize, the font.** `CreateFontW` height is fixed at creation, so a DPI change needs a NEW `HFONT` pushed to every control via `WM_SETFONT` (the existing `win32::set_font` already does the push). Delete the old one — this is a `DeleteObject` path the crate already has for its brushes, so follow that pattern rather than leaking.
4. **Check the WebView2 side while you are here.** The sibling defect in the same finding (a horizontal scrollbar with nothing to scroll) may share this root cause: if the container's bounds and WebView2's `RasterizationScale` disagree about physical versus logical pixels, the page's viewport ends up slightly wider than the visible area. That is a separate task (`windows-page-shows-a-phantom-horizontal-scrollbar`), but if your DPI work fixes it, say so there rather than letting two tasks fight over one line.

**Verification, and be honest about its ceiling.** A CI runner has no DPI, so the `windows-renderer` leg CANNOT confirm this. What it CAN do, and should: exercise the scaling arithmetic as pure functions (a `scale(8, 144) == 12` unit test on the Ubuntu gate), and assert in the window smoke that the layout is computed from the DPI seam rather than from raw constants. The real proof is a human on a scaled display, so record manual steps (100%, 150%, 200%, and a drag between two monitors at different scales) in the spike README and state plainly that CI cannot close this one.

**Scope:** DPI scaling of the Win32 chrome and its font, `WM_DPICHANGED`, the initial window size, and the tests above. Not in scope: dark mode for common controls (`windows-chrome-dark-mode-for-common-controls`), and the scrollbar (its own task).

## Acceptance criteria

- [ ] Every hard-coded chrome metric and the UI font scale from `GetDpiForWindow`, through ONE seam rather than scattered conversions.
- [ ] `WM_DPICHANGED` re-scales the window: the suggested rect is honoured, the font is recreated and pushed to every control, and the layout re-runs.
- [ ] The old `HFONT` is deleted rather than leaked on every DPI change.
- [ ] The initial window size is DPI-scaled, so a 200% display does not open a half-size window.
- [ ] The scaling arithmetic is unit-tested on the Ubuntu gate, and the window smoke asserts the layout comes from the DPI seam.
- [ ] Manual steps are recorded (100/150/200% and a cross-monitor drag), and the README states plainly that CI cannot verify this.
- [ ] The manifest still declares `PerMonitorV2` (do not revert it).
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: on real hardware the Windows chrome draws at half size while the page is correct. Cause: `app.manifest` (landed yesterday) declares `PerMonitorV2`, promising Windows that this process scales ITSELF — and the Win32 code does not. `win32.rs::ui_font()` hard-codes `CreateFontW(-15, …)` and `chrome.rs`/`window.rs` are full of raw 96-DPI metrics (`MARGIN: 8`, `PROGRESS_HEIGHT: 3`, `90 x 26` buttons, `1024x768` default, `place(title, MARGIN, MARGIN, 300, 20)`, the `- 40` / `- 4` / `- 8` adjustments). Do NOT revert the manifest: unaware means Windows bitmap-scales the process and both the chrome and the PAGE go blurry. Instead scale everything from `GetDpiForWindow(hwnd)` against 96 (`MulDiv`), through ONE `scale()`/`Dpi` seam rather than dozens of call sites, including the font height and the initial window size; handle `WM_DPICHANGED` (honour the suggested rect via `SetWindowPos`, recreate the `HFONT` and push it with `WM_SETFONT`, delete the old one following the crate's existing brush-cleanup pattern, then re-run layout) so a drag between differently-scaled monitors works. CI has no DPI and cannot confirm this: unit-test the scaling arithmetic on the Ubuntu gate, assert in the window smoke that layout comes from the seam, record manual steps for 100/150/200% plus a cross-monitor drag, and say plainly that CI cannot close it. If your work also fixes the phantom horizontal scrollbar (its own task, possibly the same physical-versus-logical pixel root cause), say so there.
