# DIAGNOSIS: blog/portfolio buttons do NOTHING on Android (field-test finding D, mobile half)

Task: `mobile-ronan-eth-buttons-no-navigation`. Field finding: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` finding D's MOBILE half. Diagnosis method: `~/.agents/skills/diagnosing-bugs/SKILL.md`. Date: 2026-07-28. Device: Android emulator `sdk_gphone64_x86_64`, API 36 (Android 16), real System WebView.

## Symptom (as reported)

On `ronan.eth` (a SvelteKit `@sveltejs/adapter-static` prerendered site), clicking the **blog** or **portfolio** buttons did NOTHING on Android: no navigation, no error, no visible signal. The same buttons work on desktop (desktop's separate blog-data bug was the query-string-leaks-into-the-DAG-path failure, fixed in `diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture`, and it produced a SvelteKit "500" WITH navigation, a later-in-chain symptom).

## The feedback loop (Phase 1)

Two red-capable loops were built, both committed:

1. **The on-device WebView probe** (`crates/werust-android/app/src/androidTest/java/com/github/wighawag/werust/SpaClientNavOriginTest.kt`, run with `cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest`): a raw `WebView` on the real System WebView serves a minimal page that does exactly what a SvelteKit client nav does (a relative `fetch('/blog/__data.json?x-sveltekit-invalidated=01')`, then `history.pushState('/blog/')`), FIRST from an `ipfs://` document answered through `shouldInterceptRequest` (the pre-fix mechanism) and THEN from the internal `https://<cid>.ipfs.werust.invalid` origin (the fix). The harness records the exact three signal streams the task told the diagnosis to capture: every intercepted request (the Network tab's source), every console message (the Console tab's source), and every `doUpdateVisitedHistory` (the SPA-nav signal). Network-isolated (canned bytes), deterministic, seconds per run. The first test goes RED on the bug (it asserts the broken behaviour of the pre-fix mechanism, so it would go green-for-the-wrong-reason if the platform ever changed) and the second pins the fixed behaviour.
2. **The Rust-seam guards** (`crates/werust-android/rust/src/backend.rs` tests `a_spa_client_side_nav_on_the_internal_origin_completes_end_to_end` + siblings, and `origin_map.rs` unit tests): the core-side URL mapping and SPA-nav signal routing, in the repo's `verify` gate.

## Reproduce + minimise (Phase 2)

The minimal repro removes the Rust core, the ENS front door, and IPFS retrieval entirely: a canned-bytes `WebView` harness isolates the ONE variable that distinguishes Android from desktop, the page's ORIGIN. Every remaining element is load-bearing: drop the `fetch` and the `pushState` would still throw; drop the `pushState` and the `fetch` would still be rejected.

## Hypotheses (Phase 3, ranked)

1. **The Android WebView gives an `ipfs://` document an OPAQUE origin, so the client nav dies inside Blink before any signal reaches werust.** Prediction: on the opaque origin, `fetch(ipfs://…)` is rejected before the network stack (NO `shouldInterceptRequest`, NO Network entry), `pushState` throws `SecurityError` (NO `doUpdateVisitedHistory`), and the ONLY signal is a console error. This is the "NOTHING in the debug view except a console error" signature the task named.
2. The ANR-fix executor serialisation swallows the client nav. Prediction: blocking the `SyncSession` mutex (e.g. during a long retrieval) freezes UI-thread callbacks. Disproved by the evidence below: the signals never EXIST, even with the core idle and responsive (the site rendered, scroll worked, chrome responded).
3. A `shouldInterceptRequest` interaction eats the router's data request. Prediction: the interception fires but the response never reaches the page. Disproved: the interception NEVER fires for the `__data.json` fetch on the broken path (evidence below).

## The evidence (Phase 4, on-device, verbatim)

From the probe's logcat (tag `SpaClientNavProbe`), real System WebView, API 36:

BEFORE (the pre-fix mechanism, an `ipfs://<cid>/` document served via `shouldInterceptRequest`):

```
origin: ipfs://
fetch: reject:TypeError
pushState: throw:SecurityError
intercepted requests: [ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/]
console: [ERROR: Fetch API cannot load ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/blog/__data.json?x-sveltekit-invalidated=01. URL scheme "ipfs" is not supported.]
history updates: []
```

AFTER (the fix, the SAME page on the internal `https://<cid>.ipfs.werust.invalid` origin):

```
origin: https://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq.ipfs.werust.invalid
fetch: ok:200
pushState: ok:/blog/
intercepted requests: [https://<cid>.ipfs.werust.invalid/, …/favicon.ico, …/blog/__data.json?x-sveltekit-invalidated=01, …/favicon.ico]
console: []
history updates: [https://<cid>.ipfs.werust.invalid/blog/]
```

Note what the BEFORE evidence shows about the task's three questions: a console error DOES fire (Blink's, naming the unsupported scheme and the exact `__data.json` URL); a `__data.json` Network entry does NOT appear (the fetch is rejected INSIDE Blink, before the network stack, so `shouldInterceptRequest` (the Network tab's only feed) never sees it); and NO `doUpdateVisitedHistory` fires (the `pushState` throws). On the pre-fix build the in-app debug view would therefore show exactly ONE clue: the Console-tab error. That is the "client nav is swallowed before any signal reaches werust" case, with the console error as the smoking gun.

## Root cause (Phase 5)

**An `ipfs://` document served through `WebViewClient.shouldInterceptRequest` gets an OPAQUE origin in the Android System WebView** (the page's origin serialises as `ipfs://` with NO host; Chromium only builds tuple origins for standard schemes, and Android's interception hook is not scheme registration). On that origin, two Blink-level refusals kill every SvelteKit client-side navigation before any signal reaches werust:

- `fetch()` to an `ipfs://` URL is rejected inside Blink ("URL scheme \"ipfs\" is not supported") BEFORE the network stack, so the client router's `__data.json` data fetch never happens; and
- `history.pushState` to another `ipfs://` path throws `SecurityError` (not same-origin with the opaque origin), so even the URL update dies.

The button "does nothing" because the router's first step (the data fetch) throws, its error handling aborts the navigation, and there is no navigation event, no request, and no history update for werust to observe. It is distinct from the desktop symptom (a 500 WITH navigation) because it is EARLIER in the chain, exactly as the task predicted.

**Why desktop navigates while Android did not:** desktop's WebKitGTK backend registers `ipfs` as a FIRST-CLASS scheme (`webkit_web_context_register_uri_scheme`), so the document gets a real `ipfs://<cid>` tuple origin and both `fetch` and `pushState` work. Android has no scheme-registration API; `shouldInterceptRequest` is interception-only, and an intercepted custom-scheme document stays opaque.

**Why the fix is the internal `https://` origin:** the fallback recorded for exactly this contingency in `work/notes/observations/mobile-ipfs-interception-mechanism-2026-07-23.md`, promoted to THE mechanism. The WebView loads `https://<cid>.ipfs.werust.invalid[/path]` (a normal fetchable, `pushState`-able secure context, one origin PER SITE so two content-addressed sites share no storage), and the Rust edge (`crates/werust-android/rust/src/origin_map.rs`) maps every URL between that internal origin and the core's real `ipfs://` URLs: pending loads OUT (`take_pending_load`), every reported/intercepted URL BACK (`resolve_scheme`, `on_url_changed`, `on_page_*`, the debug Network capture). The core's history, URL bar, trust machinery, `_redirects` main-frame inference, and debug view all keep speaking `ipfs://`; the internal origin never leaks into the core. The CID is normalised to its canonical lowercase base32 CIDv1 form (Chromium lowercases hostnames, so a mixed-case CIDv0 could not round-trip), the SAME form the ENS contenthash decoder already produces. `.invalid` is RFC 2606, so the internal origin can never collide with, or resolve to, a real site. The ONE Kotlin call site that loads a URL the core did not surface (the `_blank`/`window.open` transport) maps through the session-free JNI `toWebViewUrl`.

## The fix works end to end (on-device evidence)

On the API-36 emulator, the REAL app (the debug APK of this branch) loaded the REAL `ronan.eth` site (the exact published CID `bafybeifuz4lr7oyjg3v7s6tq4dq5gtx5t7irf7lesw4szahuyqfkzfiw7m`, reproduced by `ipfs add`-ing `/home/wighawag/dev/github/wighawag/ronan-eth/web/build` to a local offline kubo) through the full werust path (pending load → internal origin → interception → CAR retrieval → hash verification). Results:

- The home page renders, trust indicator **✓ verified** (`home-page-renders-verified-after-fix.png`).
- Clicking **Read My Blog**: the SPA nav proceeds AND completes; the URL bar follows to `ipfs://<cid>/blog/` (the `doUpdateVisitedHistory` → `on_url_changed` → mapped-back path), and the blog page RENDERS ITS POSTS ("Natural Composability in Autonomous Worlds", "Infinite Games", …), so the `__data.json` fetch + hydration succeeds end to end, no SvelteKit error boundary (`blog-page-renders-posts-after-fix.png`).
- Clicking **Check My Portfolio**: navigates to `/portfolio/` and renders the cards.
- The **Back** button returns home (`ipfs://<cid>/`), so the SPA nav integrated with core history.
- The debug view's **Network tab** shows the intercepted requests as REAL `ipfs://` URLs with `200 … ✓ content-verified` (`debug-view-network-tab-speaks-ipfs-urls.png`); the internal origin is invisible on the user's diagnosis surface, exactly as designed.

Environment caveat for the run (not a code issue): this LAN's router DNS-blocks the default RPC endpoint (`ethereum-rpc.publicnode.com`, replaced by a `blocking.asus.hns.tm` block page), so the ENS leg could not run on this network; the fail-closed surfacing handled it honestly (the prominent amber retryable banner carried the full reason, `ENS resolution failed: rpc transport error: io: invalid peer certificate: certificate not valid for name "ethereum-rpc.publicnode.com"…`, plus the footer reason; no unverified render). The site was therefore loaded by its direct `ipfs://<cid>/` URL with the retriever pointed at the local offline kubo via a SCRATCH build (the `DEFAULT_TRUSTLESS_GATEWAY` constant temporarily set to `http://10.0.2.2:8080`, reverted immediately after the APK was built; never committed). The settings-UI route to a custom gateway could not be used because the setting cannot take effect on Android at all, a PRE-EXISTING gap recorded at `work/notes/observations/retrieval-backend-setting-cannot-take-effect-on-mobile-2026-07-28.md`. Neither workaround touches the mechanism under test (origin mapping + SPA-nav completion).

## Regression guards

- `SpaClientNavOriginTest` (committed androidTest, on-device, network-isolated): the real-WebView seam. `an_ipfs_document_served_via_interception_has_an_opaque_origin_where_client_nav_dies` pins the root-cause behaviour of the pre-fix mechanism; `the_internal_https_origin_lets_a_spa_client_nav_proceed_and_complete` pins the fix; `the_core_maps_an_ipfs_url_to_the_internal_origin_over_jni` pins the Kotlin→JNI wiring of the session-free map.
- Rust seam (the `verify` gate): `origin_map.rs` unit tests (round-trip, CIDv0 normalisation, lowercase-host, lookalike-host and fail-soft cases) and `backend.rs` tests (`the_pending_load_is_served_on_the_internal_https_origin`, `webview_signals_on_the_internal_origin_report_back_as_ipfs`, `a_spa_client_side_nav_on_the_internal_origin_completes_end_to_end`, `an_internal_origin_request_routes_to_the_ipfs_scheme_handler`).
- Manual device steps: this file (the end-to-end section above). Every step was driven over `adb` (`input tap`/`uiautomator dump`/`screencap`) and is reproducible on any emulator with a local kubo.

## iOS parity

iOS could NOT be run on this host (Linux; the iOS shell needs Xcode/a simulator), so this is a mechanism analysis with the runtime confirmation left as recorded steps, not a device run.

iOS does NOT share the cause, by construction. The iOS shell registers `ipfs` with `setURLSchemeHandler(_:forURLScheme:)` (`WKWebViewShellController`'s `IpfsSchemeHandler`), and WebKit gives a `WKURLSchemeHandler`-served document a REAL tuple origin (`scheme://host`), not an opaque one: that is the entire serving model of Capacitor/Ionic/Cordova apps, where full SPAs (client-side routers, relative `fetch`/XHR, `pushState`, `localStorage`) run from `ionic://localhost` / `capacitor://localhost` custom-scheme origins as a matter of course. The known WKURLSchemeHandler limitations are elsewhere (cross-scheme CORS fetches, missing POST bodies), not same-origin fetch/pushState. So on iOS the client router's relative `fetch('/blog/__data.json')` is same-origin, reaches the scheme handler, and `pushState` within `ipfs://<cid>` is same-origin and fires the `WKWebView.url` KVO the iOS edge already observes for SPA nav (`track-webview-url-on-spa-clientside-navigation`). The root cause is CHROMIUM-SPECIFIC (opaque origin for a non-standard scheme answered via interception); the two WebKit ports both give handler-served schemes real origins.

Recorded runtime confirmation for a Mac (residual risk, small): build the iOS shell, load `ronan.eth`, click blog/portfolio; expected: navigates and renders posts. If it ever does NOT, the fix analogue is the same internal-origin map behind the iOS edge, and this task's `origin_map` concept ports directly.

### Addendum 2026-07-30 — the mechanism is now under a RUNTIME probe (`macos-wkwebview-renderer-backend`)

The paragraph above is still, as written, a mechanism analysis. What changed is that the mechanism is no longer only reasoned about: the macOS WKWebView backend task built `crates/macos-origin-probe`, the WebKit analogue of `crates/windows-origin-probe`, which loads a canned SvelteKit-shaped page from `ipfs://<cid>/` through a registered `WKURLSchemeHandler` on a real `macos-14` runner and MEASURES the four facts this caveat rests on — the document `origin`, whether a same-origin `fetch('/blog/__data.json')` resolves *and* fires the handler, and whether `pushState` throws — against a negative control (the identical bytes with an opaque origin) that must reproduce the Android failure. It runs in `.github/workflows/macos-renderer.yml` and asserts against a recorded verdict, so a later WebKit change to this corner goes red naming the field that moved.

What that does and does not settle, stated precisely:

* `WKURLSchemeHandler` is ONE WebKit class. What the probe measures about the origin a handler-served document receives is a property of the WebKit port, which macOS and iOS share, so it addresses the load-bearing half of this caveat for both.
* It does NOT build the iOS app, load `ronan.eth`, or click a blog link. The device-level steps recorded above stay open, and the residual risk they cover (something iOS-specific in `WKWebViewShellController`'s wiring rather than in WebKit's origin model) is untouched.
* At the time of writing the probe has been BUILT but not yet RUN: it was authored from Linux and its `expected.json` is an explicitly falsifiable PREDICTION until the first `macos-14` run. See `docs/spikes/macos-wkwebview-renderer-backend/README.md` ("What CI proves" / "What still awaits a Mac") for the honest split, and re-read this addendum against that file's recorded verdict once the job has run.

## The ANR guard is not regressed

The fix touches NO threading: it is a pure URL mapping in the Rust edge plus the pending-load mapping, with the `_blank` transport mapping through a session-free pure function. The ANR architecture is exactly as `android-anr-main-thread-diagnose-and-unblock` left it: session-driving actions still run on the single-threaded `coreExecutor` (never the UI thread), `shouldInterceptRequest` still runs on a WebView worker thread serialised by the `SyncSession` mutex (its host test `the_sync_session_serializes_the_ui_thread_and_the_webview_worker_thread` still passes), and console capture still pushes off the session lock (no UI-thread wait behind an in-flight retrieval). The on-device end-to-end ran with the UI responsive throughout (scrolling, chrome, debug view all live during loads), and the full workspace suite (including the ANR/sync-session tests) is green.

## Was the in-app debug view sufficient to diagnose it? (first field test of the instrument)

YES, with one named gap. The debug view's two tabs are exactly the two surfaces this bug needed, and their pre-fix signature is decisive: the Console tab carries the smoking gun (Blink's `Fetch API cannot load ipfs://…/blog/__data.json… URL scheme "ipfs" is not supported.`, which Android captures natively via `onConsoleMessage`, no shim needed) and the Network tab's corresponding SILENCE (no `__data.json` entry) is the confirmation that the request never existed. The two together distinguish "swallowed before any signal" from "started and failed", which was the diagnostic question. On the fixed build the Network tab is additionally where the fix is verified: it speaks the real `ipfs://` URLs with honest verified status (`debug-view-network-tab-speaks-ipfs-urls.png`). The view also proved itself as the general-purpose instrument for OTHER failures during this run (the settings page, the honest 502 entries when retrieval failed, the ENS/RPC failure surfaced through the chrome).

The named gap (recorded as `work/notes/observations/debug-view-network-tab-buried-by-per-nav-favicon-noise-2026-07-28.md`): on `ronan.eth` each SPA navigation produces ~5 favicon requests (the site's service-worker update check plus Chromium re-fetching `/pwa/favicon.ico`), so the newest-first Network list buries the interesting `__data.json` entry within seconds and has NO filter/search; a human diagnosing a data-fetch problem on a real site would have to scroll pages of favicon noise to find the one entry that matters.

## What would have prevented this bug

The Android interception mechanism was chosen (`mobile-ipfs-scheme-interception-ios-and-android`) with the opaque-origin risk already recorded as a KNOWN caveat with a named fallback, but the runtime experiment that would have settled it was deferred because the gate could not run a device. The lesson is now paid for: platform-origin behaviour is only decidable on-device, and this repo now HAS the on-device harness (`connectedDebugAndroidTest` + the committed probe) to ask such questions directly instead of field-finding them later.
