---
title: "Wire console + network capture on every platform to feed the core debug store (always-on for now)"
slug: debug-console-network-capture-per-platform
spec: in-app-debug-menu-console-and-network
blockedBy: [debug-capture-store-console-and-network-in-core]
covers: [4, 5]
---

## What to build

Feed the bounded capture store from `debug-capture-store-console-and-network-in-core` with REAL console + network events on every platform. Design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`. Always-on capture for now (the config toggle is a later task).

READ-FIRST / drift check: confirm the store + `push_console`/`push_network` exist (the blockedBy task). Confirm today: NO console hook is wired anywhere; network requests are only partly visible - desktop sees only registered schemes (`ipfs://`/`werust://`) via `register_uri_scheme`, https/http go direct; Android's `shouldInterceptRequest` sees EVERY request (returns null to pass through); iOS `WKURLSchemeHandler` sees only custom schemes.

### Console capture (all 3 platforms)
- **Desktop (WebKitGTK)**: wire the WebView's console-message signal (webkit6 `WebView` "console-message" / the user-content-manager console) -> build a `ConsoleEntry` -> `push_console` through the core (via the shell). `crates/webview-renderer/src/backend.rs`.
- **Android**: `WebChromeClient.onConsoleMessage(ConsoleMessage)` -> map level/message/source/line -> push through the FFI to the core. `crates/werust-android/.../BrowserActivity.kt` + the rust FFI.
- **iOS (WKWebView)**: WKWebView has NO native console callback, so inject a small `WKUserContentController` user-script that wraps `console.log/info/warn/error/debug` to `postMessage` over a message handler; the handler builds a `ConsoleEntry` -> push through the FFI. `crates/werust-ios/...`.

### Network capture (per platform's reach)
- **Desktop**: wire a per-resource load capture - WebKitGTK `WebView` resource-load signals (`resource-load-started` -> the `WebResource`'s `sent-request` / `finished` / `failed`, or `WebView`'s resource signals) -> a `NetworkEntry` (method/url/status/mime/size/from-cache/scheme) -> `push_network`. This captures https/http too, which the scheme handler does not see. Every `ipfs://` request the scheme handler already sees is captured with its verified trust; https subresources are captured as unverified-origin.
- **Android**: `shouldInterceptRequest` ALREADY sees every request - record a `NetworkEntry` there for BOTH the intercepted (`ipfs://`, with its status/mime + verified trust) and the passed-through (return-null) requests, before returning. `crates/werust-android/.../BrowserActivity.kt`.
- **iOS**: WKWebView does not cheaply expose all subresource requests. Capture what is reachable: the `WKURLSchemeHandler` custom-scheme requests (ipfs://, with verified trust), the main-frame navigations via `WKNavigationDelegate`, AND a best-effort JS hook - a `WKUserContentController` user-script wrapping `fetch`/`XMLHttpRequest` to `postMessage` request+response metadata (method/url/status) - the pragmatic route (dovetails with the console user-script). Record iOS's coverage limits honestly (it may miss some browser-internal subresource loads); this is acceptable and improves later.

Every `NetworkEntry` carries the HONEST per-request trust posture (ADR-0006) the store type defines. Capture is off the UI thread where the platform's event already is (do not reintroduce a UI-thread block - respect the Android ANR fix). Feed the ONE shared store; the debug VIEW (separate task) renders it.

## Acceptance criteria

- [ ] Console messages from the page (`console.log/info/warn/error/debug`) are captured into the core store on desktop, Android, and iOS (iOS via the injected console user-script), with level/message/source/line.
- [ ] Network requests are captured into the core store: desktop via the resource-load signals (incl. https/http, not just ipfs://), Android via `shouldInterceptRequest` (all requests), iOS via the reachable points (custom-scheme + main-frame + a best-effort fetch/XHR user-script), each with method/url/status/mime and the honest per-request trust posture.
- [ ] Capture is always-on for now, feeds the bounded store (oldest-evicted), and does NOT block the UI thread (Android ANR fix respected; capture runs where the platform event already runs).
- [ ] iOS network-capture coverage limits are recorded honestly (what it can/can't see); this is accepted, not a blocker.
- [ ] Verification/trust unchanged: capturing is READ-ONLY observation; it does not alter the load path, the ipfs:// verification, or the trust posture (it REPORTS the posture, per entry).
- [ ] Tracked per the parity guard. Tests cover the mapping from a platform console/network event to a core entry where testable (the FFI push + the entry mapping are unit-testable; the live platform hooks get the strongest automatable guard + recorded manual steps). Network-isolated.

> **FORWARD-POINTER (conductor, 2026-07-27, from the landed store's Gate-2 review — these are REQUIREMENTS on this task, not suggestions):**
>
> 1. **Push OFF the session lock (the ANR guard).** The Android push wrappers the store task shipped (`SyncSession::push_console_entry` / `push_network_entry` / `clear_debug_capture`) route through the WHOLE session lock via `self.with(...)`. `onConsoleMessage` runs on the Android UI THREAD, and `resolve_ipfs` can hold that same lock for multiple seconds on a worker thread during a CAR retrieval — so capturing through the session lock would block the UI thread behind a retrieval, which is exactly the ANR shape user story 4 exists to prevent. `DebugCapture` is an `Arc<Mutex<_>>` precisely so a capture point needs NO `&mut` shell: add a clone-out accessor (e.g. `SyncSession::debug_capture_handle`) returning a CLONED handle, and have every capture point push through THAT, never through the session boundary. Do not regress the ANR fix.
> 2. **Build entries through the constructors, never by field assignment.** `ConsoleEntry` / `NetworkEntry` have all-`pub` fields, but the `MAX_TEXT_CHARS` truncation that makes the store bounded lives ONLY in `new()` / `with_*()`. A capture point that assigns `entry.message = huge` directly silently breaks the boundedness guarantee in exactly the pathological case it was designed for. Always construct via `new()` + the `with_*` setters.
> 3. **Set the MAIN-DOCUMENT entry's posture from the load's own posture (ADR-0006 two-axis rule).** `request_trust_posture` returns a plain per-request posture and does NOT apply `TrustPosture::after_verify`'s loudest-warning rule, so on an ENS-named page the Network tab could show `content-verified` rows while the chrome trust indicator shows `name-via-trusted-rpc` on the same screen. The store's DECISIONS.md Decision 4 explicitly hands this obligation to THIS task: set the main-document entry's posture explicitly from the load's posture so the two surfaces cannot disagree. Sub-resource entries keep their own honest per-request posture.

## Blocked by

- `debug-capture-store-console-and-network-in-core` (the store + push_console/push_network + entry types this task feeds).

## Prompt

> Goal: feed the core debug capture store (from the blockedBy task) with REAL console + network events on all 3 platforms, always-on for now. Console: desktop WebKitGTK console-message signal, Android `WebChromeClient.onConsoleMessage`, iOS an injected `WKUserContentController` user-script wrapping console.* over a message handler (WKWebView has no native console callback). Network: desktop the WebView resource-load signals (captures https too, which the scheme handler misses), Android `shouldInterceptRequest` (already sees every request - record there for both intercepted + passed-through), iOS the reachable points (custom-scheme + main-frame nav + a best-effort fetch/XHR user-script; record iOS coverage limits honestly).
>
> Where to look: `crates/webview-renderer/src/backend.rs` (register_uri_scheme is there; add console + resource-load signals), `crates/werust-android/.../BrowserActivity.kt` (onConsoleMessage + shouldInterceptRequest) + its rust FFI, `crates/werust-ios/...` (user-scripts + scheme handler + nav delegate) + its FFI. Each NetworkEntry carries the honest per-request trust posture (ADR-0006). Capture runs where the platform event already runs (off the UI thread; respect the Android ANR fix); it is READ-ONLY (does not alter the load path / verification / posture). Feed the ONE shared store; the debug view renders it (separate task).
>
> Done = console + network captured into the core store on all 3 (iOS limits recorded), always-on, bounded, no UI-thread block, verification/trust unchanged, parity-tracked, unit-tested where testable + recorded manual steps. FIRST re-check the store/push_* exist and no console hook is wired yet.

## Requeue 2026-07-27

CONDUCTOR: THE MECHANISM IS DECIDED — DO NOT RE-DERIVE IT, AND ONE TASK PREMISE IS FALSE. The previous run wrote ZERO code: it spent its ENTIRE output budget reasoning about the WebKitGTK API and never edited a file. Start editing within your first few tool calls. The APIs below are VERIFIED PRESENT in this repo's pinned deps — do not go re-research them.

PREMISE CORRECTION (the task body is WRONG here): 'wire the WebView console-message signal' is NOT possible. WebKitGTK 6 (webkit6 0.4.0, the pinned binding) has NO console-message signal on WebView — I checked the crate source. Do not hunt for one.

CONSOLE — two mechanisms, both already proven in this repo:
* Desktop AND iOS use ONE SHARED injected shim. The seam already has everything: Renderer::inject_script + Renderer::register_script_message_handler (crates/renderer/src/lib.rs:789/795), and iOS already injects the EIP-1193 provider shim as a document-start WKUserScript with a message handler (WKWebViewShellController.swift:168-177) — copy that pattern exactly. Inject a document-start script that wraps console.log/info/warn/error/debug (chaining to the originals, never swallowing them) and posts {level,message,source,line} over a dedicated channel (e.g. werustConsole). Put the shim JS TEXT in ONE place in werust-core so desktop and iOS share the identical string; the handler maps it to ConsoleEntry and pushes.
* Android uses the REAL native callback, not a shim: override WebChromeClient.onConsoleMessage(ConsoleMessage) — BrowserActivity.kt already has a CoreWebChromeClient subclass (line 564) with no such override; add it there. It gives message/messageLevel/sourceId/lineNumber directly, which is strictly better than a shim. Record in DECISIONS.md that the mechanism differs per platform ON PURPOSE.

NETWORK — per platform's real reach:
* Desktop (VERIFIED signatures in webkit6 0.4.0): WebView::connect_resource_load_started(|_view, resource: &WebResource, request: &URIRequest|). From URIRequest take http_method() and uri(). Then on that resource connect_finished / connect_failed, and read resource.response() -> URIResponse for status_code(), mime_type(), content_length(). This captures https/http too, which the scheme handler never sees.
* Android: record in the EXISTING shouldInterceptRequest for BOTH the intercepted (ipfs://) and the passed-through (return-null) requests, before returning.
* iOS: the reachable points only — the WKURLSchemeHandler custom-scheme requests, main-frame navigations via WKNavigationDelegate, and a best-effort fetch/XHR wrapper added to the SAME shared shim as the console wrapper. RECORD iOS's coverage limits honestly in DECISIONS.md; partial is accepted, silence is not.

THE FORWARD-POINTER IN THE TASK BODY IS BINDING — re-read it. In particular capture must NOT go through the session lock (push through a CLONED DebugCapture handle; onConsoleMessage is on the Android UI thread and resolve_ipfs can hold the session lock for seconds on a worker thread — that is the ANR shape user story 4 forbids), entries must be built via new()/with_* so MAX_TEXT_CHARS truncation is not bypassed, and the MAIN-DOCUMENT network entry's posture must come from the load's own posture (ADR-0006) so the Network tab cannot contradict the trust indicator.

Capture is READ-ONLY observation: it must not alter the load path, the verification, or the posture. Always-on for now. Unit-test the event-to-entry MAPPING on each platform (that is the testable seam) plus recorded manual steps; network-isolated.

## Requeue 2026-07-27

CONDUCTOR ATTEMPT 3 — WORK IN SLICES AND COMMIT EACH ONE. Read this first, it changes HOW you work, not what to build.

WHY: your two previous attempts both spent the ENTIRE output budget on internal reasoning and wrote ZERO code, so nothing was salvageable. This task is 6 integration points; it does NOT have to be done in one sitting. Your work branch is KEPT between attempts and the next attempt CONTINUES from your commits. So a slice committed is a slice banked.

HOW TO WORK — this is binding:
* Spend AT MOST a handful of read/grep calls before your FIRST edit. Every API and file location you need is already written down below and in the previous handoff note; it has been verified against this repo's pinned deps. Do not go re-derive or re-research any of it.
* Do the slices IN THIS ORDER and COMMIT after EACH one. A commit per slice, not one commit at the end.
  SLICE 1: the shared console shim JS string in werust-core (one place, shared by desktop+iOS) + the ConsoleEntry mapping + unit tests. COMMIT.
  SLICE 2: desktop console — inject_script + register_script_message_handler in crates/webview-renderer. COMMIT.
  SLICE 3: desktop network — WebView::connect_resource_load_started, then per-resource connect_finished/connect_failed reading resource.response() for status_code/mime_type/content_length. COMMIT.
  SLICE 4: Android — onConsoleMessage on the existing CoreWebChromeClient (BrowserActivity.kt ~line 564) + a NetworkEntry recorded in the existing shouldInterceptRequest for BOTH intercepted and passed-through requests. COMMIT.
  SLICE 5: iOS — add the console wrapper AND a best-effort fetch/XHR wrapper to the shared shim, wire the message handler like the existing provider shim (WKWebViewShellController.swift ~168-177), plus the scheme handler and nav-delegate capture points; record iOS coverage limits in DECISIONS.md. COMMIT.
  SLICE 6: DECISIONS.md + the capability matrix row + recorded manual steps. COMMIT.
* If you run low on room, STOP CLEANLY after a committed slice. Finishing 3 of 6 slices with everything committed and the tree green is a GOOD outcome and the next attempt continues from it. Running out mid-thought with nothing committed is the ONLY real failure.
* Keep the tree compiling at every commit.

Everything else (the decided mechanisms per platform, and the BINDING forward-pointer in the task body about pushing off the session lock, constructing entries via new()/with_*, and the main-document posture) is unchanged from the previous handoff note — re-read it and follow it.

## Requeue 2026-07-28

CONDUCTOR FIX-UP (Gate-2 is RIGHT on all three — every one is a TRUST-HONESTY defect in the surface whose whole job is trust honesty). Your branch is green and preserved; CONTINUE from its tip. The capture wiring itself was accepted: do NOT redesign it. Fix these three, commit as you go, then finish.

FIX 1 — DESKTOP: a failed resource is recorded TWICE and the second row LIES. WebKit emits WebResource::failed and then ALSO emits finished (webkitWebResourceFailed ends by calling webkitWebResourceFinished; the GTK docs say the failed signal is emitted BEFORE finished). install_debug_capture currently connects BOTH connect_finished (record ..., true) and connect_failed (record ..., false) on the same resource, so every failed load pushes two rows AND the finished row passes finished_ok=true, which stamps a FAILED, possibly hash-MISMATCHED ipfs:// subresource as content-verified. THE FIX: never push from connect_failed. Let connect_failed only SET a failed flag (and the error) on the per-resource capture state; push exactly ONE row from connect_finished, reading that flag for the honest outcome. One request, one row, honest posture. Also DELETE the in-code comment claiming a verify failure lands on failed INSTEAD of finished — it is factually wrong.

FIX 2 — iOS: the main-document reconciliation never fires on an ENS page, the exact case it was mandated for. Both scheme handlers compute mainFrame as core.chrome().url == url.absoluteString, but ChromeState.url_text is the DISPLAY IDENTITY: on an ENS load the shell pins url_override so url_text is ronan.eth while the scheme-handler URL is ipfs://<cid>/index.html. So mainFrame is always false there, and the Network tab shows content-verified while the indicator shows name-via-trusted-rpc on the same screen — exactly the contradiction forward-pointer item 3 and Decision 5 forbid. THE FIX: stop comparing against the display identity. Do NOT invent a third comparison either.

FIX 3 (and it subsumes fix 2) — REUSE THE ONE MAIN-FRAME PREDICATE THIS REPO ALREADY HAS. The 3xx task already built exactly this: RedirectSink::is_main_frame(uri) in crates/werust-core/src/ipfs.rs, driven by the top-level URL the shell reports via note_navigation, normalised through frame_key so it SURVIVES the WebKit authority-less ipfs:///<cid> form (there is already a passing test named the_main_frame_check_survives_the_webkit_authority_less_url_form, and normalize_ens_page_key exists for the same reason). Promote that predicate to a shared, callable core accessor and have BOTH the desktop capture and the iOS capture use it instead of their own raw string compares. That kills fix 2, kills the desktop compare's two recorded fragilities (a redirected main document keeps its pre-redirect URL while the lifecycle holds the final one; the authority-less form), and leaves ONE main-frame concept in the codebase rather than three. Then CORRECT DECISIONS.md Decision 5: it currently claims desktop identifies the main document via WebView::main_resource(), which appears NOWHERE in the tree (zero hits). Describe what actually landed — the shared core predicate — not an aspiration.

Test each fix: a failed desktop resource produces exactly ONE row and it is NOT content-verified; an ENS-named page's main-document row carries the load's posture and cannot contradict the indicator; the main-frame predicate is shared, not duplicated. Keep everything network-isolated and the forward-pointer requirements (push off the session lock, construct via new()/with_*) intact.

## Requeue 2026-07-28

CONDUCTOR FIX-UP ROUND 2 — Gate-2 is right again, ONE defect left, and it is a small one. Your branch is green and preserved; CONTINUE from its tip. The three previous fixes were ACCEPTED (single honest row on desktop, the shared main-frame predicate, the corrected Decision 5) — do NOT touch them. Fix this one thing and finish.

THE DEFECT: on Android and iOS the main-document row takes a STALE posture, so it still contradicts the trust indicator on exactly the ENS/ipfs page the reconciliation exists for — only now it is too LOW instead of too high. Both mobile edges do entry.with_trust(shell.chrome().trust_posture), but ChromeState.trust_posture is a CACHED snapshot written only by refresh_chrome (crates/werust-core/src/lib.rs:1802). The production ordering is: navigate -> begin() resets the posture to unverified-origin -> refresh_chrome caches THAT -> the WebView asks for the document -> resolve_ipfs marks the backend content-verified -> capture_network reads the still-cached unverified-origin. refresh_chrome only runs later on didCommit/onPageStarted. So an ENS page's main-document row is stamped unverified-origin while the indicator shows name-via-trusted-rpc, and a plain ipfs page reads unverified-origin against a content-verified indicator. The reconciliation currently DOWNGRADES the row below the honest per-request posture it would otherwise have carried.

THE FIX: read the LIVE load posture at capture time, not the cached chrome snapshot — exactly what DESKTOP already does correctly at crates/webview-renderer/src/backend.rs:791 (self.life.borrow().posture()). Give the mobile capture points the same live read (the backend/renderer posture, or refresh before reading) instead of self.chrome().trust_posture. Sites: crates/werust-android/rust/src/lib.rs:625 and crates/werust-ios/rust/src/lib.rs:321-322. Desktop is the reference implementation here; make mobile match it rather than inventing anything.

Then make the README manual steps honest: Android step 5 and iOS step 6 cannot pass as written today, so they must describe the fixed behaviour. Add a test that pins the ordering trap — a main-document capture that happens BEFORE any refresh_chrome must still carry the live posture, not the stale cached one. Network-isolated.
