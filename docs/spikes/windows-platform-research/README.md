# Windows platform research: the evidence base

Task: `windows-platform-research` (research only, no implementation). Decision + recommendation: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md). Judgement calls made while researching: [`DECISIONS.md`](DECISIONS.md).

This file is the durable artifact behind the ADR: every claim the ADR makes, with the source that supports it, the date it was read, and an explicit VERIFIED / UNVERIFIED marker. It also carries the two things a future implementer needs and should not have to re-derive: the seam-method-to-WebView2-API map, and the design of the on-Windows probe that would settle the one question no document can settle.

Research host: Linux (Debian). No Windows machine was available, so nothing here was executed on Windows. Every runtime claim about WebView2 is therefore documentary (Microsoft primary docs, plus the public bug tracker and real-world Rust consumers), and the one load-bearing runtime question is written up below as a runnable probe rather than asserted.

## 1. The load-bearing question, and the honest answer

The task's core question: would custom-scheme `ipfs://` interception in WebView2 give the document a REAL tuple origin, avoiding the opaque-origin problem that forced the Android internal-`https` origin map (`crates/werust-android/rust/src/origin_map.rs`, root cause in [`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`](../mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md))?

**On paper: yes, and by explicit API design.** WebView2 has a real scheme-REGISTRATION API (not merely interception, which is what Android has), and registering with `HasAuthorityComponent = TRUE` is documented to give the scheme http-like tuple origins:

> The URIs of registered custom schemes will be treated similar to http URIs for their origins. They will have tuple origins for URIs with host and opaque origins for URIs without host as specified in 7.5 Origin - HTML Living Standard. Example: `custom-scheme-with-host://hostname/path/to/resource` has origin of `custom-scheme-with-host://hostname`.

and

> When this property is set to `true`, the URIs with this scheme will be interpreted as having a scheme and host origin similar to an http URI. [...] If this property is set to `false`, URIs with this scheme will have an opaque origin similar to a data URI. This property is `false` by default.

Source: [ICoreWebView2CustomSchemeRegistration](https://learn.microsoft.com/en-us/microsoft-edge/webview2/reference/win32/icorewebview2customschemeregistration) (read 2026-07-30). Introduced in WebView2 Win32 SDK **1.0.1587.40**. `TreatAsSecure` (only effective with `HasAuthorityComponent`) additionally makes the origin a secure context. VERIFIED as documentation; UNVERIFIED as observed behaviour.

**In practice: not proven, and the specific sub-behaviour werust depends on has an open, unfixed bug.** werust's `ipfs://` requirement is not "a document renders"; it is "a SvelteKit client-side navigation completes", which needs a same-origin `fetch()` of `/blog/__data.json` plus `history.pushState`. The WebView2 tracker has an open report that `fetch()` and `XMLHttpRequest` to a registered custom scheme fail with the `WebResourceRequested` handler never firing:

- [WebView2Feedback #4328](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4328) "Custom schemes don't work with the js fetch() api", opened 2024-01-28, **still open** as of 2026-07-30, last confirmed 2025-12-12: "`XmlHttpRequest` also does not work. The devtools console says [...] blocked by CORS policy [...] but the WebResourceRequested handler is never called, so there is no opportunity to add that header to the response. The same thing happens whether the origin of the webpage is my custom scheme, an http server, or a file."
- [WebView2Feedback #4362](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4362) "WebResourceRequested not called from CSS with custom URI scheme", opened 2024-02-09, **still open**, commented 2026-01-21: CSS `url()` subresources (web fonts) 401 without reaching the handler. The reporter's mitigation is exactly the setting werust needs anyway (`HasAuthorityComponent = true`), so this one is plausibly not a blocker, but it shows the same corner is thinly tested.
- [WebView2Feedback #5495](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5495) "[Regression] Navigation from pages rendered with custom scheme fails", 2026-01-21: on stable runtime 144.0.3719.82 an ordinary LINK CLICK from a custom-scheme document stopped navigating; Microsoft confirmed and fixed it in canary 146.0.3817.0 within two days. This is the important one for risk, not for feasibility: the runtime is EVERGREEN and auto-updates, so a regression in this corner ships to users without werust doing anything.
- [WebView2Feedback #4694](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4694) and the API contract itself: custom scheme registrations are fixed at environment creation, immutable for the browser-process lifetime, and every environment sharing the process must register an IDENTICAL set or creation fails.

Some of #4328's symptoms are consistent with reporters NOT setting `HasAuthorityComponent = true` (an opaque custom-scheme origin serialises `Origin: null`, and the documented `AllowedOrigins` rule is "From any opaque origin (Origin header is null), no cross-origin requests are allowed"), which is why this research does not conclude "WebView2 custom schemes are broken". It concludes: the outcome is genuinely undetermined from documents, and it is cheap to determine with a probe (section 4).

## 2. What the biggest real-world Rust consumer does (the decisive precedent)

The task asked whether Tauri / Lapce / Zed solve this. Findings (read 2026-07-30):

- **Zed** and **Lapce** are not relevant: both render their own UI (GPUI, floem) and embed no system webview, so neither exercises custom-scheme origins.
- **Tauri** goes through **wry** (`wry` 0.56.0, published 2026-07-30, 9.3M recent downloads), and wry deliberately does **NOT** use WebView2's custom-scheme registration. It ships an internal-`http(s)`-origin mapping in [`src/custom_protocol_workaround.rs`](https://github.com/tauri-apps/wry/blob/dev/src/custom_protocol_workaround.rs), whose own header says:

  > - WebView2 supports non-standard protocols only on Windows 10+, so we have to use a workaround. See <https://github.com/MicrosoftEdge/WebView2Feedback/issues/73>
  > - On Android, there's no API for registering custom protocols, so this workaround is also used.

  The mechanism is `{protocol}://localhost/abc` <-> `{http_or_https}://{protocol}.localhost/abc`, applied on navigation and reverted before the request reaches the user's protocol handler, with `AddWebResourceRequestedFilter` on the mapped `http(s)` prefix (`src/webview2/mod.rs`, `attach_custom_protocol_handler`). A GitHub search of `tauri-apps/wry` for `CustomSchemeRegistration` returns **zero** issues or PRs: they have never moved to the API.

**This is the same shape werust already built for Android.** wry maps `{scheme}://host` to `http(s)://{scheme}.localhost`; werust maps `ipfs://<cid>` to `https://<cid>.ipfs.werust.invalid` (with the CID as the HOST so two content-addressed sites are not same-origin, and `.invalid` per RFC 2606 so it can never resolve). werust's version is strictly better shaped for a browser. So the mechanism the largest Windows-webview Rust deployment uses in production for exactly this problem is a mechanism werust ALREADY OWNS, unit-tested, in `origin_map.rs`.

## 3. Bindings, runtime, toolchain

**Which Rust binding.** VERIFIED from crates.io / docs.rs / the repo, 2026-07-30:

| Crate | Latest | Published | Recent downloads (90d) | Verdict |
|---|---|---|---|---|
| `webview2-com` + `webview2-com-sys` ([wravery/webview2-rs](https://github.com/wravery/webview2-rs), MIT) | 0.39.1 | 2026-03-11 | ~6.07M | **Use this.** Generated from the WebView2 winmd with `windows-rs`, callback boilerplate generated by macros, and it is what wry/Tauri depends on, so it is exercised by millions of installs. |
| `webview2` + `webview2-sys` (sopium) | 0.1.4 | **2021-10-20** | ~3.6k | Do not use. Effectively unmaintained for ~5 years; predates the custom-scheme API. |
| `wry` (as the whole backend, not just bindings) | 0.56.0 | 2026-07-30 | ~9.3M | Considered and rejected as werust's seam; see ADR-0011 "Considered options". |

`webview2-com` covers the custom-scheme path in safe-ish Rust today: `webview2_com::CoreWebView2CustomSchemeRegistration` is a Rust-implemented COM object with `new(scheme_name)`, `set_has_authority_component`, `set_treat_as_secure`, `set_allowed_origins`, and `CoreWebView2EnvironmentOptions::set_scheme_registrations` feeds them to environment creation. Note the friction the crate documents: `windows-bindgen` mis-generates `ICoreWebView2EnvironmentOptions4::SetCustomSchemeRegistrations` (it reads the array of interface pointers as an out-param), so the crate declares its own `IFixedEnvironmentOptions4`. That is a fixed, upstream-absorbed cost, not a werust cost, but it is a signal about how thin the ice is off the main path.

**Runtime availability.** VERIFIED from [Distribute your app and the WebView2 Runtime](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) (read 2026-07-30): the Evergreen Runtime "will be included as part of the Windows 11 operating system"; "The vast majority of Windows 10 devices have the WebView2 Runtime installed already" with a small number lacking it, and Microsoft's own recommendation is that the app CHECK for the runtime (registry key or API) and deploy the ~2MB bootstrapper if missing. Windows 7 / 8.1 are out of scope regardless: Edge and the WebView2 Runtime dropped them after version 109 (January 2023, [Edge blog 2022-12-09](https://blogs.windows.com/msedgedev/2022/12/09/microsoft-edge-and-webview2-ending-support-for-windows-7-and-windows-8-8-1/)). So: **Windows 10+ is fine in practice, but "no installer ever needed" is not a promise werust can make**; a first run on a bare Windows 10 box must degrade honestly (name the missing runtime, link the download) rather than crash. That is a user-visible behaviour a Windows task must specify, not discover.

**Linking and cross-compilation.** VERIFIED from the [webview2-rs README](https://github.com/wravery/webview2-rs) (read 2026-07-30): for `*-pc-windows-msvc` the crate links `WebView2LoaderStatic.lib`, so nothing extra ships; for `*-pc-windows-gnu` (the natural cross-from-Linux target) it links the import lib instead and **`WebView2Loader.dll` must sit next to the executable or on `$PATH`**. Regenerating bindings needs `mono` on a non-Windows host, but consuming the published crate does not.

## 4. The probe that would settle it (design, not run)

The repo has already paid once for deciding a platform-origin question from documents instead of a device (`mobile-ipfs-scheme-interception-ios-and-android` recorded the opaque-origin risk as a caveat, deferred the runtime check because the gate could not run a device, and the bug was found in the field: DIAGNOSIS.md, "What would have prevented this bug"). The Android answer was a committed on-device probe, `crates/werust-android/app/src/androidTest/.../SpaClientNavOriginTest.kt`. The Windows analogue should exist BEFORE any Windows backend is written, and it is small.

**Shape** (mirroring the Android probe, network-isolated, canned bytes, seconds per run, no werust core, no IPFS, no ENS):

1. A tiny `windows` + `webview2-com` binary, `#[cfg(windows)]`, ignored on other hosts, run on a `windows-latest` CI job (and by hand on a Windows box).
2. Create the environment TWICE, as two cases:
   - **Case A, real custom scheme:** register `ipfs` with `HasAuthorityComponent = true`, `TreatAsSecure = true`, `AllowedOrigins = ["ipfs://*"]`; navigate to `ipfs://<cid>/`; answer `WebResourceRequested` from a canned in-memory map.
   - **Case B, internal origin:** no registration; navigate to `https://<cid>.ipfs.werust.invalid/`; answer the same canned bytes through a `WebResourceRequested` filter on that host (the mechanism `origin_map.rs` already implies).
3. The served document does exactly what a SvelteKit client nav does, and reports each result to the host over `window.chrome.webview.postMessage`:
   - `document.location.origin` (is it a tuple origin or `null`?),
   - a same-origin `fetch('/blog/__data.json?x-sveltekit-invalidated=01')` (does it resolve, and does `WebResourceRequested` FIRE for it?),
   - `history.pushState({}, '', '/blog/')` (does it throw `SecurityError`?),
   - a CSS `url()` web-font subresource and a `<script type="module">` (the #4362 / #4328-comment shapes),
   - `navigator.serviceWorker.register('/sw.js')` (informational: see the observation note below).
4. Assert, per case: the four host-visible facts (origin string, fetch outcome, whether the handler fired for the fetch, pushState outcome). Record the verbatim output in this directory, exactly as the Android DIAGNOSIS records its BEFORE/AFTER logcat.

**Decision rule the probe feeds:** if Case A passes every check on the then-current stable runtime, the Windows backend can serve real `ipfs://` origins (and desktop Linux, iOS and Windows then agree on the URL the page sees, with Android the only mapped platform). If Case A fails any of them, Windows uses Case B, `origin_map.rs` is promoted from an Android module to a shared one, and the Windows edge maps URLs exactly as the Android edge does. Either way the probe is the artifact that keeps the choice honest, and it must be re-runnable because the runtime is evergreen (#5495 is the proof that this corner regresses in stable).

## 5. Can the `Renderer` trait express a WebView2 backend?

Yes, with **no widening**, and with more of the seam natively served than either mobile platform manages. Mapping (WebView2 API names verified against Microsoft reference docs, 2026-07-30):

| `Renderer` method / seam fact | WebView2 mechanism | Notes |
|---|---|---|
| `navigate` / `reload` / `stop` | `ICoreWebView2::Navigate`, `Reload`, `Stop` | direct |
| `go_back` / `go_forward` / `can_go_*` | `GoBack`, `GoForward`, `get_CanGoBack`, `get_CanGoForward`, `add_HistoryChanged` | real session history, like WebKitGTK |
| `poll_event` / `LoadState` | `add_NavigationStarting`, `add_ContentLoading`, `add_NavigationCompleted` (+ `add_DOMContentLoaded`) | maps onto Started / Committed / Finished / Failed as WebKitGTK's `load-changed` does |
| `LoadEvent::UrlChanged` (SPA nav) | `add_SourceChanged`, whose args carry `IsNewDocument` | a same-document change is exactly `IsNewDocument == FALSE`, so the SPA-nav fact is NATIVE here (desktop infers it from `notify::uri`, Android from `doUpdateVisitedHistory`, iOS from KVO on `url`) |
| `view_handle` | the container `HWND` the controller is created into (`ICoreWebView2Controller`, `put_Bounds`) | `ViewHandle(*mut c_void)` already carries an opaque platform pointer; an `HWND` fits with no trait change |
| `send_pointer` / `send_key` / `send_scroll` / `set_focus` | the child HWND receives real input; `MoveFocus` for focus | same posture as the GTK widget (the embedded view serves interaction) |
| `register_script_message_handler` | `add_WebMessageReceived` + page-side `window.chrome.webview.postMessage` | the provider shim's channel name is per-platform already |
| `inject_script` | `AddScriptToExecuteOnDocumentCreated` | document-start, as required |
| `evaluate_javascript` | `ExecuteScript` (or `PostWebMessageAsJson`) | fire-and-forget matches the seam |
| `register_scheme_handler` | `ICoreWebView2EnvironmentOptions4::SetCustomSchemeRegistrations` (scheme NAMES, at env creation) + `AddWebResourceRequestedFilter` + `add_WebResourceRequested` (the handler, any time) | **the one real constraint**: see below |
| `SchemeResponse` status (site 404 / `_redirects`) | `ICoreWebView2Environment::CreateWebResourceResponse(stream, statusCode, reasonPhrase, headers)` | carries a real status, like WebKitGTK's `set_status` and Android's status-taking `WebResourceResponse` |
| off-UI-thread retrieval (ADR-0008) | `ICoreWebView2WebResourceRequestedEventArgs::GetDeferral` -> `ICoreWebView2Deferral::Complete` | the existing `crates/webview-renderer/src/offthread.rs` split (`retrieve_off_thread` produces a `Send` outcome; `complete_ipfs_request` applies it on the marshalling thread) is toolkit-free and reusable AS IS; only the glue changes (deferral + post back to the message-loop thread instead of `gio::spawn_blocking` + `spawn_local`) |
| new window in place (ADR-0010) | `add_NewWindowRequested`, read `get_Uri`, `put_Handled(TRUE)`, then navigate the current view | feeds the shared `renderer::new_window_action` rule unchanged |
| follow the OS color scheme (ADR-0009) | `ICoreWebView2Profile::put_PreferredColorScheme(COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO)` | AUTO is documented to follow the OS, so this row is a one-liner instead of desktop's XDG-portal read |
| web inspector | `OpenDevToolsWindow` (gate on `debug_assertions`, per the repo's recorded gating decision) | full Chrome devtools |
| debug console capture | no console event, BUT `CallDevToolsProtocolMethod` gives `Runtime.consoleAPICalled` / `Log.entryAdded` | potentially better than desktop's injected shim; the shared shim also works, so this is an option, not a requirement |
| debug network capture | `AddWebResourceRequestedFilter("*", ALL)` + `add_WebResourceResponseReceived` | sees every resource including `https://`, matching desktop's `resource-load-started` reach (wider than iOS) |

**The one real constraint.** WebView2 fixes the SET OF SCHEME NAMES at environment creation and makes it immutable for the browser-process lifetime; only the `WebResourceRequested` HANDLER can be attached later. The seam's `register_scheme_handler(&mut self, scheme, handler)` is called AFTER construction today (`WebViewRenderer::new()` builds the `WebView`, then `install_ipfs` / the `werust` scheme registration run on the live `WebContext`). So a WebView2 backend must either (a) create the environment LAZILY (construct the container HWND eagerly so `view_handle` works, defer environment + controller creation to the first `navigate`, by which time the shell has registered its handlers), or (b) declare werust's known scheme set (`werust_core::ipfs::IPFS_SCHEME`, `werust_core::retrieval::WERUST_SCHEME`) at construction and reject a later unknown scheme. Option (a) honours the trait contract as written and is recommended; option (b) introduces a new refusal (a user-visible error) and would be a decision to record. Either way **the trait does not change**, which is the answer the task asked for.

## 6. Effort model

Grounded in the sizes of the shells this repo has already built (measured 2026-07-30):

| Existing comparable | Size |
|---|---|
| `crates/webview-renderer` (GTK/WebKitGTK backend + seam glue + off-thread boundary) | 3,404 lines Rust |
| `crates/werust/src/main.rs` (the GTK desktop shell: chrome, menu, debug view, CLI, tests) | 2,995 lines Rust |
| `crates/werust-android/rust` (Rust edge) + Kotlin edge | 4,073 + 1,980 lines |
| `crates/werust-ios/rust` (Rust edge) + Swift edge | 3,271 + 1,771 lines |

Estimate for a Windows desktop build at capability parity (solo + LLM, the pace this repo has actually run at):

| Piece | Person-days |
|---|---|
| 0. The origin probe (section 4) + recorded verdict | 1 to 3 |
| 1. Shared desktop-chrome presentation extraction (platform-neutral, also serves macOS; see ADR-0011) | 2 to 4 |
| 2. `Webview2Renderer`: the `Renderer` impl over `webview2-com` (nav, history, events, script bridge, scheme handler + deferral, posture, color scheme, new-window, capture) | 8 to 12 |
| 3. The Win32 window + chrome painting (URL bar, nav buttons, trust indicator, banners, badge, menu, two-tab debug view) | 8 to 14, lower if 1 lands first |
| 4. `windows` column in `docs/platform-capability-matrix.toml` + the stub tasks the guard then forces (21 capability rows today) | 1 to 2 |
| 5. CI: a `windows-latest` build/test job, runtime presence check, packaged zip attached to the Release | 2 to 4 |
| **Total** | **22 to 39 person-days (roughly 4.5 to 8 person-weeks)** |

The dominant cost is item 3, and it is the same cost macOS carries. That is why ADR-0011's recommendation turns on item 1 rather than on WebView2.

## 7. CI and build strategy

- **Add a `windows-latest` job; do not cross-compile.** The Linux desktop leg already learned this lesson the expensive way: `.goreleaser.yaml` and `docs/adr/0002` record that the `builder: rust` zigbuild path could not link system WebKitGTK, so the desktop leg is a NATIVE `cargo build`. Windows is the same shape (a system webview loader + a message-loop shell): build it where it runs. GitHub-hosted `windows-latest` is free for public repos.
- Target `x86_64-pc-windows-msvc` so `WebView2LoaderStatic.lib` links statically and the zip is one exe. `aarch64-pc-windows-msvc` is a later, cheap addition (the loader ships an arm64 lib) once there is a reason.
- Cross-from-Linux is POSSIBLE (`x86_64-pc-windows-gnu` + shipping `WebView2Loader.dll`, or `cargo-xwin` for msvc) but buys nothing here: the artifact could not be smoke-tested, and the probe (section 4) needs a real Windows runtime anyway.
- **The pure-Rust `verify` gate stays on Ubuntu and cannot compile a `#[cfg(windows)]` backend.** The repo already has the honest pattern for this: source-shape tests in the gate (`crates/werust-core/tests/*_shape.rs`) plus a real platform job. A Windows task should add both, plus the `windows` parity-matrix column, or the backend lives entirely outside the gate.
- Runtime presence in CI is a small unknown: the `windows-latest` image manifest lists Microsoft Edge but does not explicitly list the WebView2 Runtime. Microsoft states it is part of Windows 11 (the current image is 10.0.26100), and the bootstrapper is a 2MB install step if absent. Confirm in step 0, do not assume.

## 8. Windows versus the macOS task

| | Windows (WebView2) | macOS (WKWebView, task `macos-desktop-build`) |
|---|---|---|
| Engine | Chromium/Blink (same family as Android's System WebView) | WebKit (same engine as the iOS shell) |
| Custom-scheme origin | registration API exists, tuple origin documented, real-world reliability UNPROVEN (section 1) | proven by construction: WebKit gives `WKURLSchemeHandler`-served documents real tuple origins, and the iOS shell already ships on it (DIAGNOSIS.md, "iOS parity") |
| Rust reach | full: the whole webview is drivable from Rust via `webview2-com`, no second language | open: Rust ObjC bindings exist (`objc2`, `objc2-web-kit`) but were NOT evaluated here, and the existing iOS precedent is Swift over the C-ABI, so the shell language is a live choice for that task |
| Existing werust code to lean on | the `Renderer` seam, `offthread.rs`, `origin_map.rs` (if the probe says so) | the `IosBackend` + `CoreSession` pair, which is already a working WKWebView `Renderer` backend |
| CI | new `windows-latest` job | the `macos-14` runner already exists |
| Risk | one genuinely open question (origin behaviour) | no open engine question; the risk is bundle/AppKit plumbing |

Conclusion carried into the ADR: **macOS is the cheaper and lower-risk of the two new platforms**, and the piece that makes EITHER cheap is the same shared chrome-presentation extraction.

## 9. Signals captured outside this task's scope

- `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`: a service worker can register on Android's internal-`https` `ipfs://` origin but not on a real custom-scheme origin (desktop, iOS, and a would-be WebView2 `ipfs://`), so the SAME site gets different behaviour per platform. Not a Windows question, and not in any parity row today.
