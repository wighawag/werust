# Console + network capture, per platform

Task: `debug-console-network-capture-per-platform`. Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`. Decisions: [`DECISIONS.md`](DECISIONS.md). Feeds the store from `debug-capture-store-console-and-network-in-core` ([its decisions](../debug-capture-store-console-and-network-in-core/DECISIONS.md)).

werust now CAPTURES real console messages and real network requests on all three platforms, into the ONE shared bounded store in `werust-core`. Capture is always-on (Phase 1), bounded (oldest-evicted, per-field truncated), and READ-ONLY: it never answers a request, alters a load, touches the `ipfs://` verification, or changes a trust posture. The tabbed debug VIEW that renders the store is the separate follow-on `debug-view-console-network-tabs-{desktop,mobile}`.

## Where each capture point lives

| Platform | Console | Network | Code |
| --- | --- | --- | --- |
| Desktop (WebKitGTK) | the SHARED injected `console.*` shim over the capture script-message channel (WebKitGTK 6 has **no** console signal) | `WebView::connect_resource_load_started`, reading the `URIResponse`, pushing ONE row from `finished` (with `failed` only flagging the outcome) — sees `https://` too, which the `ipfs://` scheme handler never does | `install_debug_capture` in `crates/webview-renderer/src/backend.rs`, wired in `crates/werust/src/main.rs` |
| Android | the REAL native `WebChromeClient.onConsoleMessage` (no shim) | the existing `shouldInterceptRequest`, recording BOTH the intercepted (`ipfs://`) and the passed-through (`return null`) requests — it already sees every request | `CoreWebChromeClient` / `CoreWebViewClient` in `BrowserActivity.kt`, over `WerustCore.kt` -> `crates/werust-android/rust/src/lib.rs` |
| iOS (WKWebView) | the SHARED injected `console.*` shim (WKWebView has no console callback) | the reachable points only: both `WKURLSchemeHandler`s, the `WKNavigationDelegate` main-frame navigation, and a best-effort `fetch`/`XHR` shim | `install_debug_capture` in `crates/werust-ios/rust/src/lib.rs`; `DebugCaptureHandler` + the handlers/delegate in `WKWebViewShellController.swift`, over `WerustCore.swift` |

The mechanism differs per platform ON PURPOSE (Decision 1): two platforms have no native console callback, and Android's real one is strictly better than a shim. What does NOT differ is the vocabulary — every level goes through the core's one `ConsoleLevel::from_platform`, and every entry through the core's `console_entry` / `network_entry`, so the tab reads identically whatever captured it.

## iOS coverage is honestly partial

**Sees:** the custom-scheme tasks (`ipfs://`, `werust://`) with real status/MIME and real verified posture, the main-frame navigation (including `https://` pages), and page-issued `fetch`/`XMLHttpRequest`.

**Does not see:** browser-internal subresource loads (`<img>`, `<script>`, `<link>`, CSS `url()`, fonts, media, navigation preloads). WKWebView exposes no per-resource load callback, so there is no API to observe them without a proxy. The spec accepts this for Phase 1; Decision 3 records it in full.

## Trust stays honest (ADR-0006)

Every `NetworkEntry` reports what that ONE request actually did, and no capture point can upgrade a posture:

- `content-verified` ONLY where the bytes really came back through the hash-verified content-addressed path (the `ipfs://` scheme handler / resolution succeeding). A failed `ipfs://` request, a `werust://` internal page, and every page-side shim row are honestly `unverified-origin`.
- The MAIN-DOCUMENT row takes the LOAD's own two-axis posture, so it can never contradict the chrome trust indicator on the same screen (Decision 5 — the obligation the store's Decision 4 handed here). WHICH row that is comes from the codebase's ONE main-frame predicate (`BrowserShell::is_main_frame`, over the `_redirects` sink), never a per-edge URL compare: comparing against the chrome's DISPLAYED url would never fire on an ENS page, which is exactly the case the reconciliation exists for.
- One request produces ONE row: the point that knows the outcome records it, the others skip it (Decision 4). On desktop that specifically means pushing from `finished` only: WebKit emits `failed` and then ALSO `finished` for a failed resource, so pushing from both recorded it twice and the second row claimed the success the first disproved.

## No UI-thread block (the Android ANR fix respected)

Each point runs where the platform event already runs, and does only a bounded ring-buffer insert. Critically, Android's capture pushes go through a CLONED `DebugCapture` handle held beside the session mutex, never through the session lock: `onConsoleMessage` is on the UI thread while `resolve_ipfs` can hold that lock for seconds on a worker thread during a CAR retrieval. Decision 6 has the full reasoning and the one deliberately-scoped exception.

## What the automated gate covers, and what it cannot

In the pure-Rust `verify` gate (`cargo fmt && clippy && build && test` — no Android SDK, no Xcode, no display, no network):

- `crates/werust-core/src/debug.rs` tests: the platform level mapping, both shims' shape (own channel, chains to the original console, skips the natively-recorded schemes, no double-install), the envelope parse (including hostile/unreadable bodies dropped, never fabricated), the entry mapping (absent stays absent, bounded via the constructors), and the trust rules.
- `crates/webview-renderer/src/lib.rs` tests: the desktop resource-load -> `NetworkEntry` mapping, the per-request posture, the main-document two-axis row, that a FAILED resource yields exactly ONE row and it is never content-verified, that the main-frame check is the shared core predicate (and survives WebKit's authority-less `ipfs:///<cid>` form), and that desktop uses the shared shim and does NOT inject the fetch/XHR one.
- `crates/werust-android/rust/src/lib.rs` tests: the native console-callback mapping, both `shouldInterceptRequest` branches recording, the honest posture, the main-document row carrying the LIVE posture even when it is captured BEFORE any chrome refresh (the ordering trap), and the ANR guard (a capture push completing while the session lock is HELD by another thread).
- `crates/werust-ios/rust/src/lib.rs` tests: both shims injected at document start, a shim-posted console/fetch envelope reaching the store, a hostile body fabricating nothing, an unregistered channel capturing nothing, the native points' posture, the main-document row carrying the LIVE posture even when it is captured BEFORE any chrome refresh (the ordering trap), and the raw C-ABI exports (including null-session tolerance).
- `crates/werust-core/tests/debug_capture_edge_wiring_shape.rs`: a source-shape guard over all three edges (each hooks its own platform callback, records at every reachable point, never gates capture edge-side, never lets a branch depend on a capture call, never marks verification from a capture point, Android never routes capture through the session lock, desktop pushes from `finished` only, and desktop + iOS reconcile the main document through the ONE shared core predicate rather than their own URL compares).
- `crates/werust-core/tests/platform_capability_parity.rs`: the new `debug-capture-console-and-network` capability row.

What no automated test in this repo can cover: that a REAL page's `console.log` reaches the store through a live WebKitGTK webview / a device WebView / a simulator WKWebView, and that real subresource loads appear. Those are the manual steps below.

## Manual verification steps

Not yet executed (no display / device / simulator session was run for this task). Each step is written so it can be executed and its result recorded here later. The store has no VIEW yet, so each step reads the store through the debug JSON accessor rather than a tab.

### Desktop (WebKitGTK)

1. `cargo run -p werust` on a machine with a display; let `https://example.com/` load.
2. Press F12 (a debug build) to open the WebKit Web Inspector and run in its console: `console.warn("werust capture check"); fetch("https://example.com/?probe");`
3. Expect the warning to still appear in the inspector's own console (capture CHAINS, it does not swallow).
4. In a debugger/eval on `BrowserShell::debug_json()` (or once the desktop debug view lands, its Console/Network tabs): expect a `warn` entry with the message and a source, and network rows for the document, its subresources, and the `?probe` fetch — with `https` rows `unverified-origin`.
5. Navigate to a known `ipfs://<cid>` page: expect its rows `content-verified`, and the main-document row's `trust` equal to what the toolbar trust indicator shows.
6. Navigate to an ENS `.eth` page (the bar shows the NAME, e.g. `ronan.eth`, while the load is `ipfs://<cid>/…`): the main-document row must show `name-via-trusted-rpc`, the SAME thing the indicator shows, NOT the plainer `content-verified`. This is the case a display-identity compare silently missed.
7. Force a subresource failure (load a page referencing an `ipfs://<cid>` that does not resolve, or kill the gateway mid-load): expect EXACTLY ONE row for that request, `unverified-origin`, never a second row claiming `content-verified`.
8. Confirm the window stays responsive throughout (capture adds no blocking work to the GTK loop).

### Android

1. `crates/werust-android/build-and-run.sh` (or the equivalent Gradle install) onto a device/emulator.
2. Load a page that logs, e.g. via `chrome://inspect` run `console.error("werust capture check")`.
3. Read `WerustCore.debugJson()` (a temporary log line, or the mobile debug view once it lands): expect an `error` entry carrying the source id and line number from the native callback.
4. Expect network rows for EVERY request the page made, including the `https://` ones werust does not intercept (status `null`, honestly unknown, since the response never crosses werust).
5. Load an ENS `.eth` / `ipfs://` page: expect the intercepted rows with real status/MIME and `content-verified`, and the main-document row matching the trust indicator. Read `WerustCore.debugJson()` AS SOON AS the document row appears, BEFORE `onPageFinished`: the row must ALREADY match, because it reads the load's LIVE posture — the chrome cache only refreshes on the commit/finish signals, so a cached read would still show the stale pre-verify `unverified-origin` in that window.
6. THE ANR CHECK: start a slow `ipfs://` load (a large site / a slow gateway) and, while it is retrieving, interact with the UI and trigger console output. The UI must stay responsive and no ANR dialog may appear.

### iOS (Simulator)

1. `crates/werust-ios/build-and-run.sh`.
2. In the loaded page (via Safari's Web Inspector over the simulator) run: `console.info("werust capture check"); fetch("https://example.com/?probe");`
3. Read `werust_ios_debug_json` (a temporary log line, or the mobile debug view once it lands): expect an `info` entry, and a network row for the `?probe` fetch.
4. Expect the info message ALSO still visible in Safari's own console (the shim chains).
5. Load an `ipfs://` page: expect the scheme-handler rows `content-verified` with real status/MIME, the main-document row matching the trust indicator, and NO duplicate `ipfs://` row from the page-side shim.
6. Load an ENS `.eth` page (the bar shows the pinned name while the scheme tasks carry `ipfs://<cid>/…`): the main-document row must show `name-via-trusted-rpc`, matching the indicator. Read the debug JSON AS SOON AS the scheme-task row lands, BEFORE `didFinish`: the row must ALREADY match, because it reads the load's LIVE posture, not the chrome cache (which only refreshes on the commit/finish signals). This is the case Swift's old `chrome().url` compare could never satisfy, and the case a cached-posture read would stamp too LOW.
7. Confirm the honest gap: an `<img>`/`<script>` subresource of an `https://` page does NOT appear (Decision 3). That absence is expected, not a bug.
