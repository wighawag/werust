# Decisions: the per-platform console + network capture points

Task: `debug-console-network-capture-per-platform`.
Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`.
Inherits: `docs/spikes/debug-capture-store-console-and-network-in-core/DECISIONS.md` (the store this feeds).

Code: the shared half in `crates/werust-core/src/debug.rs` (the injected shims, the envelope parse, the event -> entry mapping); desktop in `crates/webview-renderer/src/backend.rs` (`install_debug_capture`) wired from `crates/werust/src/main.rs`; Android in `BrowserActivity.kt` + `WerustCore.kt` + `crates/werust-android/rust/src/lib.rs`; iOS in `WKWebViewShellController.swift` + `WerustCore.swift` + `crates/werust-ios/rust/src/lib.rs` (`install_debug_capture`) + `crates/werust-ios/Sources/werust_mobile.h`.

This records the judgement calls made while wiring the six capture points, so the debug-VIEW tasks (`debug-view-console-network-tabs-{desktop,mobile}`) and the Phase-2 `debug-network-capture-toggle-config` inherit them explicitly instead of re-deriving them.

## Decision 1: the console MECHANISM differs per platform, on purpose

Desktop and iOS inject a page-side `console.*` shim; **Android uses its real native `WebChromeClient.onConsoleMessage` callback.**

- **Why the split.** It is forced on two of the three, and a win on the third. WebKitGTK 6 exposes **no** console signal (the task body's "wire the WebView's console-message signal" premise is false for the pinned `webkit6` binding: `WebView` has no such signal), and WKWebView has no console callback at all — so desktop and iOS have only injected JS. Android *does* have the native callback, and it is strictly better than a shim: it reports level/message/source/line directly, it sees engine-emitted console output a page-side wrapper never could, and a page cannot un-wrap it.
- **What it touches.** The debug-VIEW tasks: an Android Console tab may show entries a shim-only platform would miss, and iOS/desktop line numbers are best-effort (derived from a synthetic stack) where Android's are exact. Do not treat a per-platform difference in console *fidelity* as a bug.
- **The alternative considered.** Injecting the shim on all three for uniformity. Rejected: it would *downgrade* Android to the weaker mechanism purely for symmetry, and leave the page able to un-wrap the only console capture on that platform.
- **Kept coherent** by mapping every platform's level spelling through ONE core function, `ConsoleLevel::from_platform` (Android's `WARNING`/`TIP` and the shim's `warn`/`info` land in the same vocabulary), so the tab reads identically whatever captured it.

## Decision 2: ONE shared shim string in `werust-core`, on its OWN capture channel

`console_shim()` / `network_shim()` live in `werust_core::debug` and post to `CAPTURE_BRIDGE` (`"werustDebug"`), not to the EIP-1193 `PROVIDER_BRIDGE`.

- **Why one string.** Desktop and iOS inject the byte-for-byte same source, so the two shim platforms cannot drift into two copies of subtly different JS. The parse (`parse_capture_message` / `route_capture_message`) is shared for the same reason.
- **Why its own channel** (a COHERENCE call, not just plumbing). `werustProvider` is a *trust* surface with a request/response contract; capture is one-way, read-only observation and nothing is ever pushed back down it. Folding capture into the provider channel would re-mean an existing concept and put page-controlled debug traffic on the channel the wallet bridge answers.
- **What it touches.** Any later capture point (a future platform, a future capture kind) must post `{"kind": ...}` on this channel and extend `parse_capture_message`, not mint a third channel.
- **Hostile input is assumed.** The channel is reachable from page JS directly, so the parse is total and fail-quiet: an unreadable, hostile, or unknown-`kind` body is DROPPED, never an error, never a panic, never a fabricated entry.

## Decision 3: iOS network coverage is partial — here is exactly what it does and does not see

**Sees:** the `WKURLSchemeHandler` custom-scheme tasks (`ipfs://`, `werust://`) with their real status/MIME and real verified posture; the `WKNavigationDelegate` main-frame navigation (including `https://` pages); and page-issued `fetch` / `XMLHttpRequest` via the injected best-effort shim.

**Does NOT see:** browser-internal subresource loads — `<img>` / `<script>` / `<link>` / CSS `url()` / fonts / media / navigation preloads, and any request made before the document-start script runs in a frame the shim did not reach. WKWebView exposes no per-resource load callback, so there is no API to observe them without a proxy.

- **Why accepted.** The spec's Out of Scope names full iOS capture (a proxy capturing every browser-internal load) as explicitly deferred, and requires the Phase-1 limits be recorded honestly. Partial is acceptable; silence is not.
- **What it touches.** The mobile debug-VIEW task: an iOS Network tab is legitimately shorter than an Android one for the same page. The view should not present iOS's list as exhaustive.
- **How it improves later.** A `WKWebView`-level proxy (or a future WebKit API) replaces the shim half without touching the store or the mapping, because both live in the core.

## Decision 4: one request produces ONE row — the point that KNOWS the outcome wins

Where two capture points could see the same request, the one that knows its real outcome records it and the others skip it:

- The page-side `fetch`/`XHR` shim SKIPS `ipfs:` / `werust:` URLs (in JS).
- The iOS `didFinish` main-frame capture skips the same schemes (`isCoreServedScheme`).
- Desktop does NOT inject the `fetch`/`XHR` shim at all: its resource-load signals already see every resource, including the internal loads the shim cannot, so the shim would only double-record a subset.

- **Why.** A second row from a point that does not know the outcome would claim the weaker `unverified-origin` posture for a request the handler honestly recorded as `content-verified` — two contradicting rows for one request, in the surface whose whole job is being honest about trust.
- **What it touches.** Any future capture point must ask "does a point that knows more already record this?" before adding a row.

## Decision 5: the MAIN-DOCUMENT row takes the LOAD's own posture; sub-resources keep their own

Every capture point flags the main-document request, and that row's posture is overwritten from the load's posture (the same fact the chrome trust indicator paints). Sub-resource rows keep the per-request posture `request_trust_posture` derives.

- **Why.** This is the obligation the store's DECISIONS.md Decision 4 explicitly handed to this task (ADR-0006's two-axis rule): `request_trust_posture` returns a plain per-request posture and does not apply the loudest-warning rule, so on an ENS-named page the Network tab would show `content-verified` for the page row while the indicator shows `name-via-trusted-rpc` — two surfaces contradicting each other on one screen.
- **How each platform identifies it.** Desktop: `WebView::main_resource()`, falling back to the lifecycle's current URL. Android: `WebResourceRequest.isForMainFrame` (the platform's own answer). iOS: the URL matching the core's current chrome URL, plus the `didFinish` navigation which is main-frame by definition.
- **What it touches.** The debug-VIEW tasks may rely on "the main-document row and the trust indicator always agree".

## Decision 6: Android capture pushes OFF the session lock (the ANR guard)

`SyncSession` now holds a CLONE of the `DebugCapture` handle beside its mutex; `push_console_entry` / `push_network_entry` / `clear_debug_capture` / `debug_json` all go through that clone, never `self.with(...)`.

- **Why.** `onConsoleMessage` runs on the Android **UI thread**, and `resolve_ipfs` can hold the session lock for SECONDS on a worker thread during a CAR retrieval (`docs/adr/0008`). Capturing through the session boundary would block the UI thread behind a content retrieval — exactly the ANR shape the spec's user story 4 exists to prevent, and exactly what the off-main-thread work fixed. `DebugCapture` is an `Arc<Mutex<_>>` precisely so a capture point needs no `&mut` shell.
- **The ONE exception, deliberately scoped.** The MAIN-FRAME row's posture read (`self.with(|s| s.chrome().trust_posture)`) does take the lock — it must, since that fact lives in the session. It runs only on the WebView WORKER thread that already locks for `resolve_ipfs`, never on the UI thread, and only for the main document (not for any sub-resource, i.e. not on the hot path).
- **Guarded** at runtime by `a_capture_push_never_waits_on_the_session_lock_so_the_ui_thread_cannot_anr` (it holds the session lock and captures from another thread) and structurally by `debug_capture_edge_wiring_shape.rs`, so a later refactor cannot quietly route capture back through the lock.
- **This changed a landed method's threading contract**: the store task's `push_*` wrappers used to lock. Nothing outside this task called them yet, so the change is contained.

## Decision 7: an unknown fact stays unknown, and no capture point ever upgrades trust

A status/size/line of `0` from a platform means "the platform reported none", and the core constructors keep it honestly ABSENT (JSON `null`), never a fabricated `0`. In particular Android's passed-through branch records status `0`/mime `""`, because the response never crosses werust.

Likewise, `verified` at every call site means "these bytes actually came back through the hash-verified content-addressed path", never "the URL looks content-addressed": a failed `ipfs://` resolution, a `werust://` internal page, and every page-side shim row are all honestly `unverified-origin`. The rule itself is never re-derived at an edge — every entry goes through the core's `network_entry` / `request_trust_posture`.

## Decision 8: entries are always built through `new()` + `with_*`

Every capture point builds its entry via the core's `console_entry` / `network_entry` helpers, which use the constructors. `ConsoleEntry` / `NetworkEntry` have public fields, but the `MAX_TEXT_CHARS` truncation that makes the store bounded lives ONLY in `new()` / `with_*` — a point assigning `entry.message = huge` directly would silently break boundedness in exactly the pathological case it was designed for. Pinned by `a_capture_point_entry_is_bounded_because_it_goes_through_the_constructors`.

## Decision 9: the parity-matrix row lands NOW, and says `implemented` for CAPTURE only

A `debug-capture-console-and-network` row is added to `docs/platform-capability-matrix.toml`, `implemented` on all three.

- **Why now** (the store task deferred it, Decision 5 there, on the grounds that nothing was user-reachable yet). Capture is where the per-platform ASYMMETRY the guard exists to catch becomes real — notably iOS's honestly partial network coverage — so the row belongs here rather than with the views.
- **What `implemented` means here**: the capture POINTS are wired on that platform, not that a user can SEE the entries (the tabbed views are the follow-on `debug-view-console-network-tabs-{desktop,mobile}` tasks). The row's comment says so explicitly, and the coverage differences are named in it, so `implemented` is not read as "identical reach".
