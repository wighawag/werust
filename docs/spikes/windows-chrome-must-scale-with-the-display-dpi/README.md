# Windows: the chrome scales itself, because the manifest promised Windows it would. What landed, and what is proven by what

Task: `windows-chrome-must-scale-with-the-display-dpi`. Defect it closes: defect 1 of the first run on REAL Windows hardware, [`work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md`](../../../work/notes/findings/windows-shell-first-run-on-real-hardware-2026-07-31.md) — the page rendered correctly while the toolbar, buttons, URL bar, trust indicator and status line drew at roughly half size. Judgement calls made while closing it: [`DECISIONS.md`](DECISIONS.md). The window this defect sits on: [`windows-win32-window-and-chrome`](../windows-win32-window-and-chrome/README.md). The manifest that made it visible: [`windows-release-packaging-leg`](../windows-release-packaging-leg/README.md).

**Read this first: CI cannot verify this one.** A GitHub runner has no scaled display, no second monitor and nobody looking at the window, so no job in this repo can report the defect or its fix. What CI *can* do it now does — the scaling arithmetic is unit-tested on the Ubuntu gate, and the Windows window smoke measures the real widgets against the seam — but the claim "the chrome is the right size on a 200% display" is closed by a human on hardware and by nothing else. The steps are at the end, unhedged.

## The defect, in one paragraph

`crates/werust-windows/app.manifest` declares `PerMonitorV2`. That is a PROMISE: Windows must not bitmap-scale this process, because the process scales itself. The window did not keep it. `win32.rs::ui_font()` hard-coded `CreateFontW(-15, …)`, and every rectangle in `chrome.rs` and `window.rs` was a raw 96-DPI pixel (`MARGIN: 8`, `PROGRESS_HEIGHT: 3`, the `90 x 26` debug button, `1024x768`, `place(title, MARGIN, MARGIN, 300, 20)`, the `- 40` / `- 4` / `- 8` adjustments). So on a 150%/200% display the chrome came out at 66%/50% of its intended size while WebView2, which does its own DPI handling, drew the page correctly: a correct page in a doll's-house chrome.

Reverting the manifest is NOT the fix and was not done: unaware means Windows bitmap-scales the whole process, which restores the apparent SIZE at the cost of a blurry chrome AND a blurry page. The manifest is still right; the window now keeps its side of it.

## What landed

- **`crates/werust-windows/src/dpi.rs` (new, host-independent).** The ONE seam. It holds the chrome's metrics as DESIGNED (at 96 DPI) — `TOOLBAR_HEIGHT`, `MARGIN`, `TRUST_WIDTH`, `FONT_HEIGHT`, `DEFAULT_WIDTH/HEIGHT`, `DEBUG_*`, … — plus `Dpi::scale`, which is `MulDiv(value, dpi, 96)` spelled in plain Rust, and `Metrics::at(dpi)`, the whole table at one display's scale. It is deliberately NOT `#[cfg(windows)]`: the arithmetic is pure, so the Ubuntu `verify` gate compiles and unit-tests it, exactly as it does `profile.rs`.
- **`crates/werust-windows/src/win32.rs`.** `window_dpi()` is the ONE `GetDpiForWindow` call in the crate (per-MONITOR, never the process's system DPI). `ui_font(height)` now takes the height the seam computed instead of hard-coding `-15`. `release_font()` deletes a superseded `HFONT` down the same `DeleteObject` path `Theme::release` already used for its brushes. `control_rect()` reads a control's rectangle back in its parent's client coordinates, which is what lets the smoke MEASURE the layout.
- **`crates/werust-windows/src/chrome.rs`.** `Chrome` carries the window's current DPI; `Chrome::metrics()` is the only source of a pixel, and `relayout()` computes every rectangle from it. `Chrome::controls()` names every control the window owns, so a font push cannot silently miss one.
- **`crates/werust-windows/src/window.rs`.** The initial window size is scaled once the `HWND` exists (a 200% display no longer opens a half-size window); the font is created at the scaled height; `WM_DPICHANGED` is handled on BOTH top-level windows — the suggested rect Windows sends is honoured with `SetWindowPos`, the font is recreated and pushed to every control with `WM_SETFONT`, the old one is deleted, and the layout re-runs. The debug view's own layout (its title, CLEAR button, tab strip and list columns) goes through the same seam, at its OWN monitor's scale.
- **`crates/werust-windows/Cargo.toml`.** One added `windows` feature: `Win32_UI_HiDpi`, for `GetDpiForWindow`.
- **`crates/werust-windows/tests/windows_window_shape.rs`.** `the_chrome_scales_from_one_dpi_seam_and_follows_a_dpi_change`, in the file's existing source-shape style. Beyond the wiring, it SCANS both layout functions for any integer literal that did not go through the seam, because a missed call site is a subtly misaligned control rather than an obvious failure.
- **`crates/werust-windows/examples/window_smoke.rs`.** Measures the real widgets against `Metrics::at(Dpi::new(window.dpi()))` and prints the runner's DPI, saying plainly which of the two claims the run can support.

Out of scope, deliberately: dark mode for the common controls (`windows-chrome-dark-mode-for-common-controls`) and the phantom horizontal scrollbar (`windows-page-shows-a-phantom-horizontal-scrollbar`). The scrollbar's task lists "the same physical-versus-logical pixel mismatch" as its first hypothesis; what this work did and did not change for it is recorded in [`work/notes/observations/dpi-work-does-not-touch-the-page-container-width-2026-07-31.md`](../../../work/notes/observations/dpi-work-does-not-touch-the-page-container-width-2026-07-31.md).

## What the Ubuntu `verify` gate proves TODAY (every ordinary run)

- **The arithmetic.** `crates/werust-windows/src/dpi.rs`'s unit tests: `scale(8)` at 144 DPI is 12; 100% leaves every design metric untouched; 200% doubles the whole table including the font height and the initial window size; the rounding is `MulDiv`'s (half away from zero, so a 3-pixel progress strip becomes 5 at 150% rather than 4) and is symmetric about zero; an unreadable DPI (`0`, what `GetDpiForWindow` answers for an invalid window) falls back to the 96 baseline rather than collapsing every rectangle to nothing; a large metric at 400% neither overflows nor wraps.
- **The wiring, by reading the source it cannot compile.** That the seam exists and is not `cfg`-gated, that the design metrics live only there, that both layouts contain no unscaled pixel at all, that there is exactly ONE `GetDpiForWindow` call site, that `WM_DPICHANGED` honours the suggested rect and recreates + pushes + releases the font, that the initial sizes are scaled, and that the manifest still says `PerMonitorV2`.

What it does NOT prove: anything about pixels on a screen. There is no display.

## What the local cross-target harness proves

[`typecheck-windows-from-linux.sh`](../windows-webview2-renderer-backend/typecheck-windows-from-linux.sh) was run against this change: `cargo xwin clippy -p windows-renderer -p werust-windows --target x86_64-pc-windows-msvc --tests --examples --all-targets -- -D warnings` came back clean. So the Win32 half — the `WM_DPICHANGED` handler, the font lifecycle, `GetDpiForWindow` — type-checks against the real Windows SDK bindings. None of it has been executed here.

## What the `windows-latest` CI leg proves

`windows-renderer.yml` builds and tests both crates and runs `window_smoke`, which now measures the REAL widgets: the page starts exactly one scaled toolbar down, the URL bar is exactly the seam's toolbar row, the trust indicator is exactly the seam's width, and the window's metrics are the seam's for the DPI Windows reported.

Read that honestly. A runner is a 96-DPI virtual display, so those assertions prove the layout is **computed from the seam** rather than from constants — a real ratchet against a future raw pixel creeping back — and they prove **nothing about scaling**, because every scaled value equals its design value at 96 DPI. The smoke prints which case it is in. The identical assertions run on a human's 150%/200% display DO prove the scaling, which is why they are written as comparisons against the seam rather than against fixed numbers.

## What still awaits real Windows hardware

Stated plainly, because nothing above can substitute for it. Windows' display scale is Settings → System → Display → "Scale and layout"; changing it does not need a sign-out for a newly launched app.

1. **At 100% (96 DPI).** Launch `werust-windows.exe`. Expected: the chrome looks exactly as it did before this change (this is the no-regression case, and the one CI approximates).
2. **At 150% (144 DPI).** Set the scale, then launch. Expected: the toolbar, the nav buttons, the URL bar, the trust indicator, the status line and the window itself are all half again as large as at 100%, in the same proportions; the text is CRISP (not blurry — blurry means the manifest was lost and Windows is bitmap-scaling the process again); the trust badge's phrase is not clipped; the URL bar's progress strip is still visible during a load.
3. **At 200% (192 DPI).** Same, at double size. This is the case the human's screenshot showed as half-size chrome, so it is the direct before/after. The window should open at 2048x1536 device pixels, i.e. the same apparent size as 1024x768 at 100%.
4. **A cross-monitor drag.** With two monitors at DIFFERENT scales (e.g. a 200% laptop panel and a 100% external display), drag the browser window from one to the other and back. Expected: as the window crosses, it changes size to the scale Windows suggests, and the chrome AND its text re-scale with it, immediately, with no restart. Everything stays aligned: no control keeps the old font size, no button overlaps its neighbour, and the URL bar still takes the slack between the nav buttons and the trust indicator.
5. **The debug view across the same drag.** Open ⋮ → Debug, then drag the DEBUG window alone to the other monitor. Expected: its title, CLEAR button, tabs and list columns re-scale. Its FONT is the browser window's, by decision — see [`DECISIONS.md`](DECISIONS.md) §3 — so on a differently scaled monitor the debug view's text may be a size behind until the BROWSER window is dragged too. That is a known, recorded limitation, not a surprise.
6. **A DPI change while the app is open.** With the app running, change the display scale in Settings. Expected: same as the drag (Windows sends the same message), with no restart.
7. **Resize and maximise at each scale.** The page must still take everything between the toolbar and the status line, and a failed load's banner must still be the only thing that moves it.

Steps 4, 5 and 6 are the ones a reviewer should be most sceptical of: `WM_DPICHANGED` is reasoned about here and has never run.
