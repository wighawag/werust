---
title: "werust: a first-party in-app debug menu (console + network), alongside the native remote inspector"
slug: in-app-debug-menu-console-and-network
taskedAfter: []
---

> A general browser MENU (like other browsers' ⋮ menu) that will grow to hold the usual browser
> items, whose FIRST content is the werust VERSION and a DEBUG entry opening a tabbed DEBUG VIEW
> (Console log + Network requests) on EVERY platform. This is a FIRST-PARTY, no-tether debug surface
> built INTO the browser - it OVERRIDES the earlier "mobile debugging is remote-inspection-only"
> stance (from `enable-web-inspector-devtools-all-platforms`): a phone user with no tethered desktop
> can open the menu -> Debug and see console + network. The native remote inspector (Safari over USB /
> chrome://inspect over USB / desktop F12) STAYS as the deep/full devtools; this is the standalone
> console+network subset. Design origin: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`
> (v0.2.6 human request). Phase 1 captures network ALWAYS; a debug-menu config toggle (capture on/off +
> reload) is a Phase-2 follow-on.

## Problem Statement

werust has a native remote inspector (debug-gated, per `enable-web-inspector-devtools-all-platforms`): desktop F12 (WebKitGTK Web Inspector), iOS via Safari Web Inspector over USB, Android via chrome://inspect over USB. That is a DEEP devtools experience but it REQUIRES a tethered desktop on mobile - a phone user standing on their own cannot see what the page is doing. The human wants a FIRST-PARTY, in-app debug surface, reached through a general browser MENU, that shows at least the CONSOLE LOG and the NETWORK REQUESTS with NO tether, on every platform.

Two realities make this a real feature, not a tweak (checked against the code):

- **No console capture exists today.** No `console-message`/`onConsoleMessage`/console bridge is wired anywhere. Each platform must wire its console hook (desktop WebKitGTK console-message signal; Android `WebChromeClient.onConsoleMessage`; iOS a `WKUserContentController` user-script wrapping `console.*`, since WKWebView has no native console callback).
- **Network requests are not all visible to werust today.** Desktop sees only the REGISTERED schemes (`ipfs://`, `werust://`) via `register_uri_scheme`; `https://`/`http://` go DIRECT through WebKit. Android's `shouldInterceptRequest` sees EVERY request (returns null to pass non-intercepted through). iOS `WKURLSchemeHandler` sees only custom schemes. So capturing "all" network needs a per-platform CAPTURE POINT, and iOS is genuinely constrained (its coverage will be partial and improve over time).

There is also NO menu on any platform yet (the shells are a toolbar of back/forward/reload/stop + URL bar), so the general menu is new UI everywhere.

## Solution

A deep-module design: ONE shared capture store in the toolkit-free core, fed by per-platform capture points, rendered by a per-platform tabbed debug view reached from a new general menu. This mirrors how the URL bar / trust indicator / loading step already work (one shared fact over the chrome/FFI surface, per-platform native rendering).

1. **A bounded capture store in `werust-core`.** `ConsoleEntry { level, message, source, line, ts }` + `NetworkEntry { method, url, status, mime, size, from_cache, scheme, trust: TrustPosture, ts, duration }`, held in two bounded ring buffers (capped, oldest-evicted, so a long session cannot grow unboundedly - the retrieval-budget / ens_pages discipline). Exposed over the SAME chrome / FFI-JSON surface the edges already read (an additive `debug` section or a dedicated accessor). Each `NetworkEntry` carries werust's HONEST per-request trust posture (an `ipfs://` request is content-verified, an `https://` subresource is unverified-origin) - coherent with ADR-0006, NOT a new trust label. A `network-capture-enabled` flag (default true) makes the Phase-2 toggle a small addition, not a rework.

2. **Per-platform capture points feed the store.** Console: desktop console-message signal, Android `onConsoleMessage`, iOS an injected console user-script. Network: desktop the WebView resource-load signals (captures https too, not just the scheme handler's ipfs://), Android `shouldInterceptRequest` (already sees all), iOS the reachable points (custom-scheme + main-frame nav + a best-effort fetch/XHR user-script, with its coverage limits recorded honestly). Capture is READ-ONLY observation (it does not alter the load path, the ipfs:// verification, or the trust posture - it REPORTS the posture per entry) and runs where the platform event already runs (off the UI thread - the Android ANR fix is respected).

3. **A general browser menu (the container).** A ⋮/menu affordance on every platform (desktop GTK MenuButton/Popover, Android PopupMenu, iOS UIMenu/SwiftUI), structured to GROW into the usual browser items later. FOR NOW it shows the werust VERSION (from `CARGO_PKG_VERSION`, exposed to mobile via one FFI accessor so all three agree) and a DEBUG entry that opens the debug view. The menu is user-facing and always available (not debug-build-gated).

4. **A tabbed debug view (per platform).** Opened from the menu's Debug entry: a Console tab (level-distinguished list of console entries) and a Network tab (request list: method/url/status/mime/size + the honest per-request trust posture, rendered with the SAME vocabulary the trust indicator uses). A Clear action + live update on the existing refresh cadence (no busy loop; the Android ANR fix respected). Read-only (a typeable REPL stays the native inspector's job). Desktop is a panel/window; mobile is a full screen with a tab bar - so the desktop and mobile views are separate tasks.

The native remote inspector stays as the deep devtools; this in-app menu is the standalone console+network subset. Both coexist.

## User Stories

1. As a user (esp. on a phone with no tethered desktop), I open a general menu and tap Debug to see the page's console log and network requests IN-APP, without any remote inspection setup.
2. As a user, the general menu shows the werust version and is structured to hold the usual browser menu items later (it is not a debug-only menu).
3. As a user, the Network tab labels each request with werust's HONEST trust posture (an ipfs:// request content-verified, an https:// subresource unverified-origin), so the debug view never implies a request was trusted that was not.
4. As a user, turning on the debug view never slows the browser (capture is off the UI thread; the view updates on the existing refresh cadence, not a busy loop) - the Android ANR fix is not regressed.
5. As a developer, console + network are captured into ONE shared core store over the chrome/FFI surface, and each platform renders the SAME store - so the debug view is consistent across platforms and grows in one place.
6. As a developer, network capture is always-on now but the store is shaped so a later config toggle (capture on/off + reload) is a small addition.

## Out of Scope (this spec)

- A typeable console REPL / full DOM inspector / sources / breakpoints -> that stays the NATIVE remote inspector's job (F12 / Safari-over-USB / chrome://inspect), which is untouched.
- The Phase-2 network-capture CONFIG TOGGLE (a debug-menu setting to turn capture on/off + reload, off by default) -> a follow-on task; Phase 1 is always-on with the store shaped for it.
- FULL iOS network capture (a proxy capturing every browser-internal subresource) -> iOS Phase-1 coverage is best-effort (custom-scheme + main-frame + a fetch/XHR user-script), limits recorded; it improves later.
- The general menu's FUTURE items (bookmarks, settings, history, ...) -> this spec lands the menu CONTAINER + version + the Debug entry only; the menu is built to grow.
- Redaction/privacy policy for captured console/network data (an RPC URL, an eth address can appear) -> flagged as a decision for the toggle/hardening follow-on, not resolved here.

## Decisions to record when tasking (open judgement)

- Debug-view surface on desktop: an in-window togglable panel vs a separate window/dialog (record the choice).
- The FFI shape for the store: an additive `debug` section on the existing chrome JSON vs a dedicated `debug_json()` accessor (keep the chrome JSON lean; additive either way).
- iOS network coverage: is custom-scheme + main-frame nav + a fetch/XHR user-script an acceptable Phase-1 Network tab, with limits recorded (yes, per Out of Scope)?
- Trust vocabulary in the Network tab: reuse the trust-indicator posture words exactly (ADR-0006); do not invent a new label.

## Task breakdown (derived tasks)

1. `debug-capture-store-console-and-network-in-core` (foundation: the bounded store + entry types + FFI surface + capture-enabled flag; core-only, unit-tested). blockedBy: [].
2. `debug-console-network-capture-per-platform` (wire console + network capture on all 3 to feed the store; always-on). blockedBy: [debug-capture-store-console-and-network-in-core].
3. `general-browser-menu-with-version-and-debug-entry` (the general ⋮ menu + version + Debug entry, all platforms). blockedBy: [].
4. `debug-view-console-network-tabs-desktop` (desktop tabbed debug panel). blockedBy: [debug-capture-store-console-and-network-in-core, general-browser-menu-with-version-and-debug-entry].
5. `debug-view-console-network-tabs-mobile` (Android + iOS full-screen tabbed debug view). blockedBy: [debug-capture-store-console-and-network-in-core, general-browser-menu-with-version-and-debug-entry].
6. (Phase 2, later) `debug-network-capture-toggle-config` (the debug-menu capture on/off + reload toggle, off by default). blockedBy: the above.

Practical order: store (1) -> menu (3) -> capture (2) -> desktop view (4) -> mobile view (5) -> (toggle later). The store lands first, the menu gives the Debug entry, capture feeds the store, the views render it.
