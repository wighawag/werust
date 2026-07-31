# Judgement calls made building the WebView2 `Renderer` backend

Task: `windows-webview2-renderer-backend`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), sub-task 2 of the Windows breakdown (funded by Amendment 1, unblocked by Amendment 2). What landed and what is proven by what: [`README.md`](README.md).

These are the choices that were NOT already made for me by the ADR, the spike's section-5 mapping or the measured probe verdict. Each says what was chosen, why, what was rejected, and what else it touches — so a reviewer can ratify or reverse it rather than discover it.

## 1. The crate is `windows-renderer`; the type is `Webview2Renderer`

**Chosen:** crate name `windows-renderer` (matching the sibling `macos-renderer`, and matching the `windows-renderer` CI leg that already exists on `main`), type name `Webview2Renderer` (the name ADR-0011's own breakdown uses: "`Webview2Renderer`: the `Renderer` impl over `webview2-com`").

**Why:** two existing names already point at this thing and they disagree by one word. Following BOTH costs nothing and re-means nothing: the crate is the platform edge (like `macos-renderer`, `werust-android`), the type is the engine binding (and WebView2, not "Windows", is what it binds — a hypothetical future Windows backend over the native renderer would sit beside it, not replace it).

**Rejected:** `webview2-renderer` as the crate name, which would have made the CI leg's name (`windows-renderer.yml`) and its path filter read oddly; and `WindowsRenderer` as the type, which would have contradicted the ADR text for no gain.

**Touches:** the CI leg's package selectors and `push` path filter, and `crates/werust-core/tests/windows_renderer_leg_shape.rs`'s `GREEN_ON_WINDOWS` — all updated in the same change, because that guard deliberately makes extending the leg a decision rather than a reflex.

## 2. The page-side bridge shape is supplied by an ADAPTER, not by forking the shared shim

**The gap:** werust's provider and debug shims are SHARED, toolkit-free core code (`werust_core::provider::provider_shim`, `werust_core::debug`), and they post to `window.webkit.messageHandlers.<name>.postMessage(...)` — WebKit's page-side API, which WebKitGTK, macOS and iOS all have natively. WebView2 has no named handlers at all: it has ONE channel, `window.chrome.webview.postMessage`.

**Chosen:** the backend injects a small document-start ADAPTER (`pure::bridge_adapter_script`) that DEFINES `window.webkit.messageHandlers.<name>` in terms of `window.chrome.webview`, wrapping each post in an envelope `{"handler": "<name>", "body": "<string>"}`; the host reads the envelope back (`pure::parse_bridge_envelope`) and routes to the registered bridge. The shared shims are injected UNCHANGED on top.

**Why this is not a new concept:** it is the SECOND instance of one this repo already established. The Android edge has exactly the same problem (its `WebView` has no `messageHandlers` either) and solves it exactly this way, in Kotlin: `BrowserActivity.kt`'s `buildProviderScript` writes a preamble defining `window.webkit.messageHandlers.<channel>` over its `@JavascriptInterface`, then appends the core shim. Following the established answer keeps the core shims at ONE implementation.

**Rejected:** (a) teaching `werust_core::provider::provider_shim` to detect `window.chrome.webview` — that puts a per-platform branch inside the one piece of page-side code every edge shares, which is precisely what the shared-derivation rule exists to prevent; (b) inventing a new seam concept for "bridge transport", which would duplicate `register_script_message_handler` at the wrong layer.

**Touches:** any future shared page-side shim (the debug console/network capture shims the sibling window task will want) — they will work on Windows with no change, because they post to the same shape. A message a page sends directly through `window.chrome.webview.postMessage` that is NOT one of these envelopes is DROPPED rather than delivered to some bridge; that is asserted, because mis-delivering an unaddressed message would hand page-controlled input to a native handler.

## 3. A failed scheme resolution fails the load HERE, with the honest reason

**The gap:** on WebKitGTK a refused `ipfs://` resolution calls `request.finish_error(...)` and the engine's `load-failed` carries werust's own message ("block hash mismatch for …"). WebView2 has no such channel: a `WebResourceRequested` handler either sets a response or does not, and `NavigationCompleted` can then only report a generic `WebErrorStatus`.

**Chosen:** on a refusal the backend sets NO response at all (fail closed — not one unverified byte reaches the engine, and built-in error pages are disabled so nothing is substituted), records the reason, and — if the refused resource IS the current main document — moves the shared lifecycle to `Failed` with that reason immediately. `NavigationCompleted` then SKIPS an already-failed load rather than reporting it a second time, and otherwise prefers the recorded reason over the platform status.

**Why:** it preserves the behaviour every other edge already gives (a legible failure reason reaching `LoadEvent::Failed`, which the chrome's error banner renders) without inventing a seam field, and it does not depend on WebView2's exact `IsSuccess` semantics for a resource-level failure. The alternative — answering with a synthetic `502` and a body — would have put werust in the position of SERVING something for a page whose bytes did not verify, which the fail-closed rule forbids.

**Rejected:** completing the deferral with no response and letting `NavigationCompleted` alone decide (the reason degrades to "unexpected error", losing the verify detail); and serving a synthetic error document (fails closed in name only).

**Touches:** the sibling window task's error banner, which will show this text. It is the same text WebKitGTK shows.

## 4. The default user-data (profile) folder is under the OS temp directory

**Chosen:** `%TEMP%\werust-webview2`, with `Webview2Renderer::with_user_data_folder` as the way a shell chooses its own.

**Why:** WebView2 requires a WRITABLE user-data folder and defaults to one beside the executable, which is often read-only (and is wrong for a CI runner). An ENGINE-only crate has no business minting werust's durable profile location — that is a product decision belonging to the shell — so it picks something obviously provisional and hands the real choice to the caller.

**Touches:** the sibling `windows-win32-window-and-chrome`, which SHOULD pass a durable per-user path (`%LOCALAPPDATA%\werust`) rather than inherit the temp default. Flagged here so that is a decision it makes rather than a default it inherits silently.

## 5. Every scheme this backend registers gets `HasAuthorityComponent` + `TreatAsSecure`

**Chosen:** both flags TRUE for every registered scheme, not just `ipfs://` — so `werust://settings` gets them too.

**Why:** the flags are what the probe MEASURED as giving a real tuple origin (ADR-0011 Amendment 2), and having two registration policies would mean two origin behaviours to reason about for no benefit. `werust://settings` is an internal page werust itself serves; a tuple origin (`werust://settings`) is strictly better behaved than an opaque one, and a secure context for content werust generated is not a claim about anyone else's server.

**Rejected:** per-scheme flags, which would have introduced a policy surface with no caller.

**Touches:** nothing outside this crate today; the constants are pinned in `pure.rs` and asserted, so a later "simplification" that flips either flag reds the Ubuntu gate rather than silently re-opening the opaque-origin field bug this repo has already paid for once.

## 6. No devtools, no capture wiring, no parity column

**Chosen:** `OpenDevToolsWindow`, the DevTools-protocol console capture and the `AddWebResourceRequestedFilter("*")` network capture that ADR-0011's mapping lists are NOT wired here, and neither is the `windows` column of `docs/platform-capability-matrix.toml`.

**Why:** the task's scope boundary is explicit (no chrome, no parity column), and the macOS split set the precedent — `macos-renderer` wired neither, and `werust-macos` (the window task) did. Wiring a devtools opener with no menu to open it from, or a capture feed with no debug view to show it in, would be building half of the sibling task in the wrong crate.

**Touches:** `windows-win32-window-and-chrome`, which inherits all four rows. They are listed as NOT DONE in the README's "what still awaits" section rather than left to be discovered.

## 7. The OS colour-scheme reader lives in the ENGINE crate, though the engine does not need it

*Recorded after the fact by task `windows-backend-error-mapping-and-leg-header-accuracy` (its own judgement calls: [`../windows-backend-error-mapping-and-leg-header-accuracy/DECISIONS.md`](../windows-backend-error-mapping-and-leg-header-accuracy/DECISIONS.md)). The choice was made by the sibling `windows-win32-window-and-chrome` and ratified at its Gate 3; it never reached this file, which left an unexplained registry dependency in an engine crate.*

**Chosen:** `crates/windows-renderer` exports `os_color_scheme()` (and the free function behind it) over an `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme` read, with the mapping rule pure in `pure.rs` (`os_color_scheme_from_apps_use_light_theme`) and unit-tested on the Ubuntu gate. The WebView2 engine itself needs NONE of it: it follows the OS through `COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO`, which is documented to track the setting with no reading at all. The only caller is the CHROME, in `crates/werust-windows`.

**Why here rather than in the chrome crate:** it is a PLATFORM-BINDING detail, and this crate is where the Windows platform bindings live, the same place as `RegGetValueW`, the `windows`/`webview2-com` crates and every other Win32 call already are. Putting it in the window crate would mean a second module reaching for the registry, and the window crate re-reads this value on `WM_SETTINGCHANGE` at a point where it no longer holds the backend (the renderer is behind the `Renderer` seam by then), so the reader has to be callable as a free function anyway. ONE reader and ONE mapping serve the engine and the chrome alike; `crates/werust-windows/tests/windows_window_shape.rs` asserts there is no SECOND OS-colour-scheme reader, so the choice is held rather than merely intended.

**Rejected:** (a) a copy of the read in `werust-windows`, i.e. two readers of one setting, free to drift, in the crate that is meant to hold no rule; (b) routing it through the `Renderer` seam as a new trait method, which widens the cross-platform seam for a Windows chrome detail, when `docs/adr/0009` already says the chrome FOLLOWS the OS and every other edge reads its own platform signal (macOS `os_color_scheme_from_appearance`, GTK `os_color_scheme_from_portal`); (c) leaving the chrome to guess, forbidden by the same ADR, which is why an unreadable value maps to `NoPreference` and never to dark.

**Touches:** `crates/werust-windows` (its only consumer, and the reason the symbol is `pub`), and any future non-chrome consumer, which gets the reader for free. It also means `windows-renderer`, an engine crate, carries a dependency on `Win32::System::Registry`; that is deliberate and is what this entry exists to explain.
