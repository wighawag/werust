# The mobile debug view: Console + Network tabs from the menu's Debug entry (Android + iOS)

Task: `debug-view-console-network-tabs-mobile`. Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`. Decisions: [`DECISIONS.md`](DECISIONS.md).

The mobile browser menu's Debug entry (the ⋮ menu at the right end of the toolbar, `general-browser-menu-with-version-and-debug-entry`) now opens the real in-app debug view on BOTH mobile platforms: a FULL-SCREEN tabbed screen with a CONSOLE tab and a NETWORK tab, rendered live from the shared capture store (`werust_core::debug::DebugCapture`, `debug-capture-store-console-and-network-in-core`) over the FFI `debug_json` document, fed by the mobile capture points (`debug-console-network-capture-per-platform`). This is the no-tether payoff: a phone user with no desktop sees console + network in-app. The native remote inspector (chrome://inspect over USB on Android, Safari Web Inspector over USB on iOS) is untouched and stays as the deep devtools (the typeable console REPL, DOM, sources).

## Where it lives

Android, in `crates/werust-android/app/src/main/java/com/github/wighawag/werust/`:

| Piece | What it is |
| --- | --- |
| `BrowserActivity.openDebugView()` | the hook the menu's Debug entry calls (the menu task's "not built yet" placeholder is gone); shows the overlay and enables the Back-closes-debug callback |
| `BrowserActivity.debugBackCallback` | the SYSTEM Back handler that closes the debug view while it is open (registered after the history callback so it wins only then) |
| `DebugView` (`DebugView.kt`) | the full-screen overlay view: a header (`Console + Network capture` title + `Clear` + `✕`), two tab toggle buttons, one `ListView` |
| `consoleRowText` / `consoleLevelColor` | the Console-tab mapping (level + message + source:line, coloured per level) |
| `networkSummaryText` / `networkTrustLabel` / `trustColor` / `sizeText` | the Network-tab mapping (method / status / mime / size / trust / url), unknowns rendered as `?` |
| `BrowserActivity.refreshChrome` + `onConsoleMessage` | the EXISTING event-driven refresh points the open view is refreshed from (no new timer, no poll) |

iOS, in `crates/werust-ios/App/Sources/WKWebViewShellController.swift` (kept in the existing file so no Xcode project edit is needed, DECISIONS.md Decision 7):

| Piece | What it is |
| --- | --- |
| `openDebugView()` | the hook the menu's Debug entry calls; presents `DebugViewController` full-screen |
| `DebugViewController` | the full-screen controller: a header (title + `Clear` + `Done`), a `UISegmentedControl` (Console/Network), one `UITableView` |
| `consoleRowText` / `consoleLevelColor` / `networkSummaryText` / `networkTrustLabel` / `trustColor` / `sizeText` | the same render-from-store mapping as Android (static, pure) |
| `refreshChrome` + `DebugCaptureHandler.onCapture` | the EXISTING event-driven refresh points (the capture handler's new callback fires on the main thread per captured envelope) |

Both CONSOLE tabs show one row per captured console entry: `[<level>] <message> (<source>:<line>)`, coloured by level (error red, warn amber, info blue, debug grey). Both NETWORK tabs show one row per captured request: a summary line (method, status, MIME, size, the honest per-request trust posture) coloured by posture, with the URL on a detail line. The trust speaks the mobile trust indicator's EXACT vocabulary (ADR-0006): its glyph plus the core's wire name (`✓ content-verified`, `⚠ unverified-origin`, `◈ name-via-trusted-rpc`, `◇ mutable-name`), so a content-verified ipfs:// row is the same green the indicator's verified badge is. No new trust label exists, and an unrecognised posture fails closed to `⚠ unverified-origin` (DECISIONS.md Decision 4).

The `Clear` action calls the store's clear over the FFI (`debugClear()`, both buffers); the view repaints empty and new captures keep flowing in. Rows are newest-at-bottom with stick-to-bottom scroll. On Android the system Back button or the ✕ closes the view; on iOS the Done button dismisses it. iOS network coverage is honestly partial (the capture task's Decision 3: custom schemes + main-frame navigations + page-issued fetch/XHR; never browser-internal subresource loads); the view renders whatever is captured.

## What the automated gate covers, and what it cannot

In the pure-Rust `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`):

- `crates/werust-core/tests/debug_view_mobile_wiring_shape.rs` (new): the wiring shape on both edges (the Debug hook opens a full-screen Console + Network view over the FFI `debug_json`; Clear drives `debugClear()`; the view refreshes from the existing chrome-refresh point plus the console capture event with NO `postDelayed`/`Handler`/`Timer` poll; the Network tab reuses the four wire names + the indicator's four glyphs; the view builds no input widget; the parity-matrix row is implemented on all three; the remote-inspector gates are untouched).
- `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs` (updated): the menu still routes the Debug id to the named hook on every edge; the retired placeholder assertion is replaced by a pointer to the two view guards.
- `crates/werust-core/tests/debug_view_desktop_wiring_shape.rs` (updated): the desktop cell stays implemented; the mobile cells are now the mobile guard's job.
- `crates/werust-core/tests/platform_capability_parity.rs`: the `debug-view-console-network` row is now `implemented` on desktop, iOS and Android.
- The FFI debug document itself (console + network round-trip including the trust wire names, off the session lock) stays covered by the mobile Rust cores' tests (`debug_json_round_trips_console_and_network_entries_including_their_trust` in both), and the capture points by `debug_capture_edge_wiring_shape.rs`.
- Beyond the gate: the Android Kotlin was compiled for real during this task (`./gradlew compileDebugKotlin -x cargoBuildRustCore`, SDK + NDK present on the build machine), so the Kotlin side is compiler-checked, not just shape-checked.

What no automated test here covers: that the real views appear, paint, scroll and clear on a device, and the iOS Swift compile (no Xcode on this machine; the shape guard is its net). Those are the manual steps below.

## Manual verification steps (Android: emulator or device)

Not yet executed in this task (no emulator session was run; the Kotlin compile above is the automated half). Each step is written so it can be executed and its result recorded here.

1. `cd crates/werust-android && ./gradlew :app:assembleDebug` (cross-compiles the Rust core via `cargoBuildRustCore`), then install on an emulator or device (`adb install app/build/outputs/apk/debug/app-debug.apk`) and launch werust.
2. Tap the ⋮ menu at the right end of the toolbar and tap `Debug`: the full-screen debug view opens over the page (toolbar covered), with a `Console`/`Network` toggle, a `Clear` button and a `✕`.
3. Console tab: after the default page load (and after navigating to any page that logs), console entries appear as `[<level>] <message> (<source>:<line>)`; errors are red and bold, warnings amber. New entries append at the bottom and the list auto-scrolls while it is at the bottom; scrolling up and letting more entries arrive does not yank the view back down.
4. Network tab: after a load, requests appear with method, status (or `?` where unknown, e.g. Android's passed-through https rows), MIME, size, trust and URL. An `https://` page's rows read `⚠ unverified-origin` (amber); on an `ipfs://` page (or an ENS name), the verified rows read `✓ content-verified` (green), matching the toolbar trust indicator for the same page (an ENS page's main-document row reads `◈ name-via-trusted-rpc`, never contradicting the indicator).
5. Live update: with the debug view open, reload or navigate (system Back closes the debug view first, so use the on-screen controls or reopen Debug after): new console + network rows appear at each page event, with no view reopen needed; console logs from a busy page appear as they are captured.
6. Clear: tap `Clear`: both tabs empty immediately; new captures keep flowing in afterwards.
7. Back: with the debug view open, press the SYSTEM Back button (or the ✕): the debug view closes and the page is back; pressing Back again behaves as before (page history, then exit). Reopen Debug: a fresh view opens showing everything captured since (the store is not cleared by closing).
8. Read-only: tapping or long-pressing rows never edits anything; there is no input field in the debug view.
9. Remote-inspector coexistence: with a debug build, chrome://inspect from a desktop Chrome still lists the page and its console/network, independent of the in-app debug view.

## Manual verification steps (iOS: Simulator, from a Mac)

Not yet executed in this task (no Xcode on this machine). Each step is written so it can be executed and its result recorded here.

1. On a Mac with Xcode: `crates/werust-ios/build-and-run.sh` (builds the Rust core + the Swift shell, boots an iOS 17 Simulator, installs, launches). This also compiler-checks the Swift side.
2. Tap the ⋮ menu at the right end of the toolbar and tap `Debug`: a full-screen debug view is presented, with a `Console`/`Network` segmented control, a `Clear` button and a `Done` button.
3. Console tab: after the default page load (and after navigating to any page that logs), console entries appear as `[<level>] <message> (<source>:<line>)`, coloured by level. (iOS console capture is the injected shim, so source/line are best-effort; Android's native callback is exact. That asymmetry is recorded in the capture task's Decision 1.)
4. Network tab: requests appear with method, status, MIME, size, trust and URL. Expect FEWER rows than Android for the same page: iOS captures the custom-scheme tasks (`ipfs://`, `werust://`, with their real verified posture), the main-frame navigations (including `https://` pages), and page-issued fetch/XHR; browser-internal subresource loads (`<img>`, `<script>`, CSS) are NOT seen (the capture task's Decision 3). An `ipfs://` row reads `✓ content-verified`; an `https://` main-frame row carries the page's own posture, matching the toolbar trust indicator.
5. Live update: with the debug view open, navigate or reload (Done first, then reopen Debug): new rows appear at each page event; console logs from a busy page appear as they are captured (the capture channel event refreshes the open view).
6. Clear: tap `Clear`: both tabs empty immediately; new captures keep flowing in afterwards.
7. Done: tap `Done`: the view dismisses back to the page; reopening Debug shows everything captured since (the store is not cleared by dismissing).
8. Read-only: there is no input field in the debug view; rows cannot be edited.
9. Remote-inspector coexistence: with a debug build, Safari's Web Inspector over USB still inspects the page, independent of the in-app debug view.
