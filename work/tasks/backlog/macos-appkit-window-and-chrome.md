---
title: "macOS: the AppKit window that PAINTS the chrome over the WKWebView backend"
slug: macos-appkit-window-and-chrome
blockedBy: [macos-wkwebview-renderer-backend]
covers: []
---

## What to build

The PRODUCT half of the macOS desktop shell, split from the engine so the chrome can be reviewed on its own. `macos-wkwebview-renderer-backend` proved the seam; this task makes it a browser a person can use.

### What the engine task actually landed (read this before planning)

- **`crates/macos-renderer`** — `MacosRenderer`, a `Renderer` impl over WKWebView via `objc2`, with no widening of the trait. It already carries a BARE OFF-SCREEN host window purely so the engine can be run headlessly; **this task replaces that host with the real `NSWindow`**, it does not add a second one.
- **`crates/webview-shared`** — the toolkit-free half moved out of `crates/webview-renderer` (`LoadLifecycle`/`SharedLifecycle`, the `navigate` URL rule, and the ADR-0008 off-thread `ipfs://` boundary). Reuse it; do not re-implement any of it in the window.
- **`crates/macos-origin-probe`** plus `.github/workflows/macos-renderer.yml` on the `macos-14` runner. The leg BUILDS the macOS crates, runs their tests, exercises both trust hooks on a live WKWebView with a negative control, and re-measures the origin verdict.
- **The origin question is SETTLED and measured:** `registered-ipfs-scheme` on macOS 14.8.7 / AppleWebKit 605.1.15, real `ipfs://<cid>` tuple origin, secure context, same-origin `fetch` that fires the handler, working `pushState`. Do not re-litigate it, and do not add an origin workaround.

### You CAN get a real macOS run mid-task, so do not ship a prediction

The engine task was blocked at Gate 2 for recording a PREDICTION rather than a measurement, and that trap is now avoidable: `.github/workflows/macos-renderer.yml` is on `main`, so `gh workflow run macos-renderer.yml --ref <your work branch>` is legal and runs the leg against your branch's code. If this task adds a crate or an example, EXTEND that workflow's path filters and its steps so the new surface is actually exercised, then dispatch a run and record what it proved. Anything not exercised must be listed honestly as awaiting hardware, not implied to be green.

**Naming, recorded but NOT this task's job:** `crates/macos-renderer` is platform-named while `crates/webview-renderer` is now only the WebKitGTK backend and `crates/webview-shared` is the generic home. That trio reads wrong with a third backend coming, but renaming it here would bury the chrome work in a refactor. Leave it.

A native `Werust.app` window: an `NSWindow` with a desktop-shaped toolbar (URL bar, back/forward/reload/stop, the trust indicator, the invalid-entry badge, the ⋮ menu), the error and load-progress surfaces, and the tabbed debug view, hosting the WKWebView from the backend task and driven by the shared `BrowserShell`. No browsing DECISION lives in Swift/ObjC: the edge paints and forwards.

**This task PAINTS, it does not derive.** `desktop-chrome-presentation-into-core` moved the display rules into the toolkit-free core (`status_line`, `trust_indicator` / `_detail` / `_css_class`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, plus the exported `TRUST_INDICATOR_CSS_CLASSES` / `ERROR_BANNER_CSS_CLASSES` sets). Consume them. Do not re-implement a single one in Swift, and do not hand-roll a class list: that duplication is exactly what the Kotlin and Swift twins already cost this project (the trust EXPLANATION shipped desktop-only for months because of it).

**A SECOND extraction this task owns** (found at Gate-3 of `desktop-chrome-presentation-into-core`): the DEBUG-VIEW row presentation is still private in the GTK edge (`console_level_css_class`, `console_source_line`, `console_row_text`, `network_status_text` / `_mime_text` / `_size_text` / `_trust_label` / `_trust_css_class` in `crates/werust/src/main.rs`). It is the same class of thing (pure functions of a captured entry), and this task's debug view must paint from a shared derivation, so extract them into `werust-core` behaviour-preservingly (tests moving with them) BEFORE painting, and have BOTH the GTK and macOS debug views consume them.

**Follow the existing product decisions rather than reinventing them:** ADR-0009 (follow the OS colour scheme, never force dark), ADR-0010 (`target="_blank"` / `window.open` navigate in place until tabs exist, via the shared `renderer::new_window_action`), ADR-0006/0007 (the trust posture is a product surface and a mutable name is never labelled "verified"), and the loading rule from `loading-progress-in-the-url-bar-not-a-banner` (in-flight progress lives in the URL bar and must NOT displace the page; only a FAILURE may take a banner). A macOS window also has real menu-bar conventions: use them where they are conventional, but the ⋮ menu's CONTENT still comes from the core's `BrowserMenu`.

**Scope: unsigned, unpackaged.** No code signing, no notarization, no `.app` bundling or release attachment; that is `macos-release-packaging-leg`. The `macos` parity-matrix column and the stub tasks it forces are `macos-parity-column-and-stub-tasks`, which runs after this so the cells describe what really shipped.

**Verification honesty (ADR-0011 Amendment 1):** the visible behaviour cannot be checked from the development machine, so record manual steps in a spike README and state explicitly what CI proved versus what awaits a Mac.

ADR sizing: 8 to 14 person-days, lower because the presentation extraction already landed.

## Acceptance criteria

- [ ] A native macOS window renders a page through the WKWebView backend, with the URL bar, back/forward/reload/stop, trust indicator, invalid-entry badge, ⋮ menu, error surface, load progress and debug view all present.
- [ ] Every one of those surfaces reads the SHARED derivation from `werust-core` (including the exported CSS-class sets); nothing is re-derived in Swift/ObjC.
- [ ] The DEBUG-VIEW row helpers are extracted into `werust-core` behaviour-preservingly, and BOTH the GTK and macOS debug views paint from them.
- [ ] The ⋮ menu's items come from the core's `BrowserMenu`, not a hand-written macOS list.
- [ ] ADR-0009 (OS colour scheme), ADR-0010 (new windows navigate in place) and the URL-bar-progress rule are honoured, not re-decided.
- [ ] Manual verification steps are recorded, and what CI proved versus what awaits a Mac is stated explicitly.
- [ ] The repo `verify` gate on Ubuntu stays green.

## Prompt

> Goal: the macOS window: an `NSWindow` with a desktop toolbar (URL bar, nav controls, trust indicator, invalid-entry badge, ⋮ menu), the error and load-progress surfaces and the tabbed debug view, hosting the WKWebView from `macos-wkwebview-renderer-backend` and driven by the shared `BrowserShell`. It PAINTS: every display rule already lives in `werust-core` (`status_line`, `trust_indicator*`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, plus the exported CSS-class sets), so consume them and re-implement NOTHING in Swift, because that duplication is exactly what the Kotlin and Swift twins already cost this project. This task also OWNS extracting the debug-view row helpers (`console_row_text`, `network_status_text`, `network_trust_css_class`, …) out of `crates/werust/src/main.rs` into core, behaviour-preservingly, so both the GTK and macOS debug views paint from one derivation. Honour ADR-0009 (follow the OS colour scheme), ADR-0010 (new windows navigate in place) and the rule that in-flight progress lives in the URL bar and never displaces the page. Unsigned and unpackaged; the parity column and the release leg are separate tasks. Record manual steps and say plainly what CI proved versus what awaits a Mac.
