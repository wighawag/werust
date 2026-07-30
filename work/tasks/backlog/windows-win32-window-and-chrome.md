---
title: "Windows: the Win32 window that PAINTS the chrome over the WebView2 backend"
slug: windows-win32-window-and-chrome
blockedBy: [windows-webview2-renderer-backend]
covers: []
---

## What to build

The PRODUCT half of the Windows desktop shell, split from the engine so the chrome can be reviewed on its own. `windows-webview2-renderer-backend` proved the seam; this task makes it a browser a person can use. Funded by Amendment 1 of `docs/adr/0011-webview2-for-windows.md`.

A native Windows window: a Win32 top-level window with a desktop-shaped chrome (URL bar, back/forward/reload/stop, the trust indicator and its EXPLANATION, the invalid-entry badge, the ⋮ menu), the error and load-progress surfaces, and the tabbed debug view, hosting the WebView2 from the backend task and driven by the shared `BrowserShell`. No browsing DECISION lives in the Windows edge: it paints and forwards.

### This task PAINTS, it does not derive

Every display rule already lives in the toolkit-free core (`desktop-chrome-presentation-into-core`, then `macos-appkit-window-and-chrome`, then `one-derivation-close-the-aggregate-and-tooltip-gaps`): `status_line`, `trust_indicator` / `_detail` / `_css_class`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, the exported CSS-class sets and the `CssClassFamily` aggregate, plus the DEBUG-VIEW row helpers (`console_level_css_class`, `console_source_line`, `console_row_text`, `network_status_text` / `_mime_text` / `_size_text` / `_trust_label` / `_trust_css_class`) that the macOS task extracted into core. CONSUME them. Re-implement not one of them in the Windows edge, and do not hand-roll a class list.

This is the constraint the project has already paid for twice: the Kotlin and Swift chrome twins drifted, and the trust EXPLANATION (`trust_indicator_detail`, the text saying what a posture MEANS) shipped desktop-only for months because of it. A fourth hand-written copy is the specific failure ADR-0011's Consequences section warns about. If a rule you need is NOT yet in core, EXTRACT it there behaviour-preservingly (tests travelling with it) and have the GTK and macOS edges consume it too — never derive it locally.

**Follow the existing product decisions rather than reinventing them:** ADR-0009 (follow the OS colour scheme, never force dark — WebView2 gives this natively via `PreferredColorScheme = AUTO`, and the Win32 chrome must follow the same system setting rather than a second source), ADR-0010 (`target="_blank"` / `window.open` navigate in place until tabs exist, via the shared `renderer::new_window_action` fed by `add_NewWindowRequested` + `put_Handled(TRUE)`), ADR-0006/0007 (the trust posture is a product surface and a mutable name is NEVER labelled "verified"), and the rule from `loading-progress-in-the-url-bar-not-a-banner`: in-flight progress lives in the URL bar and must NOT displace the page; only a FAILURE may take a banner. The ⋮ menu's CONTENT comes from the core's `BrowserMenu`, not a hand-written Windows list. Real devtools are `OpenDevToolsWindow` (`docs/spikes/windows-platform-research/README.md` section 5), not a re-implementation.

**Toolkit choice is yours to make and RECORD.** The research prescribes a plain Win32 window (no GTK-on-Windows, no cross-platform toolkit — werust has deliberately adopted none). Whatever you use to draw the chrome widgets, record the decision and its alternative in the spike `DECISIONS.md`, and keep the dependency surface small: this is the trust-carrying path.

### You CAN get a real Windows run mid-task, so do NOT ship a prediction

`.github/workflows/windows-renderer.yml` is on `main` (`windows-renderer-ci-leg`) and the engine task extended it, so `gh workflow run windows-renderer.yml --ref <your work branch>` is legal and runs the leg against YOUR code. EXTEND it again for this half — the macOS sibling's `cargo run -p werust-macos --example window_smoke` is the model: construct the REAL window off-screen, load a pinned in-memory hash-verified `ipfs://` fixture through the production verifying route, and assert what the real WIDGETS hold (URL bar, trust indicator AND its explanation, status line, ⋮ menu built from `BrowserMenu`, debug view catching the page's own `console.log`), then a negative control whose bytes do not hash to their CID and which must FAIL, raise the error banner and displace the page. Offline and deterministic. Then dispatch a run and record what it proved, verbatim, with the run URL. Both macOS tasks were correctly blocked at Gate 2 for recording a prediction as a measurement; do not repeat it.

**Verification honesty (ADR-0011 Amendment 1):** a CI runner is not a Windows box in front of a human, so nothing about rendering, input, focus, HiDPI or window management is verified by it. Record manual verification steps in a spike README and state explicitly what CI proved versus what awaits real hardware.

**Scope: unpackaged, unsigned, no parity column.** No installer, no code signing, no zip attached to a Release, and no `windows` column in `docs/platform-capability-matrix.toml`. Those are follow-on tasks cut AFTER this lands, mirroring the macOS sub-task structure (`macos-parity-column-and-stub-tasks`, `macos-release-packaging-leg`) so their cells and artifacts describe what really shipped. Author them at the end of this task if they do not exist yet; do not build them here.

ADR sizing: 8 to 14 person-days, lower because the presentation extraction already landed.

## Acceptance criteria

- [ ] A native Windows window renders a page through the WebView2 backend, with the URL bar, back/forward/reload/stop, trust indicator, invalid-entry badge, ⋮ menu, error surface, load progress and debug view all present, driven by the shared `BrowserShell`.
- [ ] Every one of those surfaces reads the SHARED derivation from `werust-core` (including the trust EXPLANATION and the exported CSS-class sets / `CssClassFamily`); nothing is re-derived in the Windows edge, and anything missing from core is EXTRACTED there and consumed by the GTK and macOS edges too.
- [ ] The ⋮ menu's items come from the core's `BrowserMenu`, and devtools is `OpenDevToolsWindow`.
- [ ] ADR-0009 (OS colour scheme), ADR-0010 (new windows navigate in place, via `renderer::new_window_action`) and the URL-bar-progress rule (only a FAILURE may displace the page) are honoured, not re-decided.
- [ ] The `windows-renderer` leg is extended with a real off-screen window smoke that asserts what the WIDGETS hold plus a failing negative control, and a run against this branch is recorded (run URL + verbatim output). No prediction is presented as a measurement.
- [ ] Manual verification steps are recorded, and what CI proved versus what awaits real Windows hardware is stated explicitly.
- [ ] The follow-on parity-column and packaging tasks are AUTHORED (not built) if they do not already exist.
- [ ] The Ubuntu `verify` gate stays green.

## Prompt

> Goal: the Windows window: a Win32 top-level window with a desktop toolbar (URL bar, nav controls, trust indicator + its EXPLANATION, invalid-entry badge, ⋮ menu), the error and load-progress surfaces and the tabbed debug view, hosting the WebView2 from `windows-webview2-renderer-backend` and driven by the shared `BrowserShell`. It PAINTS: every display rule already lives in `werust-core` (`status_line`, `trust_indicator*`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, the CSS-class sets and `CssClassFamily`, and the debug-view row helpers), so consume them and re-implement NOTHING locally — that duplication already cost this project the trust explanation shipping desktop-only for months, and a fourth copy is exactly what ADR-0011 warns about. Anything missing from core gets EXTRACTED there and consumed by GTK and macOS too. Honour ADR-0009 (follow the OS colour scheme; WebView2 has `PreferredColorScheme = AUTO`), ADR-0010 (new windows navigate in place via `renderer::new_window_action` fed by `add_NewWindowRequested`), and the rule that in-flight progress lives in the URL bar and never displaces the page. Menu content comes from `BrowserMenu`; devtools is `OpenDevToolsWindow`. Record the toolkit decision and its alternative. `.github/workflows/windows-renderer.yml` is on main, so extend it with an off-screen window smoke (the model is `cargo run -p werust-macos --example window_smoke`: pinned in-memory hash-verified `ipfs://` fixture, assert what the real widgets hold, then a negative control that must fail and raise the banner), dispatch it against your branch, and record the real result. Unsigned, unpackaged, no parity column — author those follow-ons, do not build them. Record manual steps and say plainly what CI proved versus what awaits real Windows hardware.
