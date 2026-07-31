---
title: "The page shows a horizontal scrollbar with nothing to scroll"
slug: windows-page-shows-a-phantom-horizontal-scrollbar
blockedBy: []
covers: []
---

## What to build

Found by the human on REAL Windows hardware, 2026-07-31 (`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`): a horizontal scrollbar is shown even when there is nothing to scroll, on `https://example.com/`, which has no horizontal overflow in any browser.

**Start by root-causing it, not by suppressing it.** A scrollbar is a SYMPTOM: the page's layout viewport is wider than the visible area. Hiding the bar (CSS injection, `overflow-x`, a WebView2 setting) would leave the page genuinely mis-sized and merely stop it telling you. Find out why the viewport is too wide.

**What is already ruled out:** there is no `WS_HSCROLL` anywhere in `crates/werust-windows` — the only scroll-related style in the crate is `ES_AUTOHSCROLL` on the URL edit, which is unrelated. So this is not a Win32 scrollbar on a werust control; it is inside the WebView2 page.

**Hypotheses, in the order worth testing:**

1. **A physical-versus-logical pixel mismatch, i.e. the same root cause as the DPI defect.** `windows-chrome-must-scale-with-the-display-dpi` is the sibling finding: the chrome draws at 96 DPI on a scaled display. If the container's bounds are set in one pixel space while WebView2's `RasterizationScale` works in another, the page's CSS viewport comes out slightly wider than the window. **Do this task AFTER that one, or at least re-check on top of it** — if the DPI fix makes this disappear, the right outcome is to say so here and close it, not to add a second fix.
2. **The page container HWND is placed wider than the host's client area.** `crates/werust-windows/src/window.rs` re-parents the engine's container into the shell window and lays it out; `crates/windows-renderer/src/backend.rs`'s container `wndproc` then sets the controller's bounds from `GetClientRect` on `WM_SIZE`. If the container is placed at, say, `MARGIN` on the left but given the FULL width, its right edge overhangs and the page is laid out wider than what is visible. Compare, in numbers, the shell's client rect, the container's window rect, and the rect the controller is given.
3. **A border or non-client edge counted twice.** `GetClientRect` excludes the border; a placement computed from `GetWindowRect` does not. Mixing the two by a few pixels is exactly enough to trigger a scrollbar and nothing else.

**Verify with numbers, not by eye.** Log or assert the three rects (shell client, container window, controller bounds) plus the page's own `window.innerWidth` / `document.documentElement.clientWidth` and `scrollWidth` via `evaluate_javascript`, so the discrepancy is a measured quantity and the fix is provably exact rather than "looks better now". A one-pixel overhang and a fifty-pixel one have different causes.

**A note on evidence:** CI cannot see this (a headless runner has no DPI and nobody looks at the window), but the *measurement* above CAN run on the `windows-renderer` leg's window smoke — asserting `scrollWidth <= clientWidth` on a page known to have no overflow is a real regression test that needs no human. Add it, so this cannot come back silently.

**Also seen in the same screenshot, and worth checking while you are in this code:** a wide dark band appears in the toolbar row between the end of the URL text field and the trust indicator. It may be the URL bar's progress strip (`chrome.rs`, `PROGRESS_HEIGHT`, `PBM_SETBARCOLOR`) being visible while idle — which the repo's own rule forbids, since in-flight progress must show only during a load — or an unpainted region of the toolbar. Confirm what it is; if it is the progress strip showing at idle, that is a second real defect and `paint.progress_visible` is the thing to check.

## Acceptance criteria

- [ ] The horizontal scrollbar is gone on a page with no horizontal overflow, because the page is correctly SIZED, not because a scrollbar was suppressed.
- [ ] The root cause is stated with numbers (the three rects and the page's own client/scroll widths), not described qualitatively.
- [ ] If the DPI task already fixed it, that is recorded here and no second fix is added.
- [ ] A regression test in the `windows-renderer` window smoke asserts `scrollWidth <= clientWidth` on a no-overflow fixture.
- [ ] The dark band in the toolbar row is identified, and if it is the progress strip visible at idle it is fixed or cut as its own task.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Prompt

> Goal: on real Windows hardware the page shows a horizontal scrollbar with nothing to scroll (`https://example.com/`, which overflows in no browser). ROOT-CAUSE it; do not suppress the bar, because the bar is the symptom of a page laid out wider than the visible area. Ruled out already: no `WS_HSCROLL` exists in `crates/werust-windows` (only `ES_AUTOHSCROLL` on the URL edit), so it is inside WebView2. Test these in order: (1) the same physical-versus-logical pixel mismatch as the sibling DPI defect — do this task AFTER `windows-chrome-must-scale-with-the-display-dpi` and, if that fix makes this disappear, RECORD that and close it rather than adding a second fix; (2) the page container HWND being placed wider than the shell's client area (`window.rs` re-parents and places it; `backend.rs`'s container `wndproc` sets controller bounds from `GetClientRect` on `WM_SIZE`); (3) a border counted twice by mixing `GetClientRect` and `GetWindowRect`. Verify with NUMBERS — the shell client rect, the container window rect, the controller bounds, and the page's own `innerWidth`/`clientWidth`/`scrollWidth` via `evaluate_javascript` — so the fix is provably exact. Add a `scrollWidth <= clientWidth` assertion to the window smoke on a no-overflow fixture: that needs no human and stops it returning. While in this code, identify the wide dark band visible in the toolbar row between the URL field and the trust indicator in the human's screenshot; if it is the URL bar's progress strip visible at IDLE, that breaks the repo's own progress rule and is a second defect (check `paint.progress_visible`).
