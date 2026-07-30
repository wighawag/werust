# Windows: the WebView2 `Renderer` backend (engine only) — what landed, and what is proven by what

Task: `windows-webview2-renderer-backend`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), sub-task 2 of its Windows breakdown, funded by [Amendment 1](../../adr/0011-webview2-for-windows.md#amendment-1-2026-07-30--the-defer-is-overturned-windows-and-macos-are-funded-now) and unblocked by [Amendment 2](../../adr/0011-webview2-for-windows.md#amendment-2-2026-07-30--gate-0-answered-windows-serves-real-ipfs-origins). Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md). Sibling task that puts a window and chrome on top of this: `windows-win32-window-and-chrome`.

## What landed

One crate and one extended CI leg.

- **`crates/windows-renderer`** — the `Renderer` implementation over Edge **WebView2**, bound with `webview2-com` 0.39.1 (ADR-0011 finding 4: what `wry` itself depends on; never the abandoned `webview2` crate), with **no widening of the trait** (the source-shape guard pins the seam's method list, and ADR-0011's headline answer was "the trait does not change"). Navigation, session history, the load lifecycle, same-document SPA URL tracking, the script-message bridge, custom-scheme interception and the ADR-0010 new-window-in-place rule all go through the seam onto real WebView2 APIs.
- **`.github/workflows/windows-renderer.yml`** — the leg that landed FIRST (task `windows-renderer-ci-leg`, precisely so this task could be MEASURED rather than predicted) now also builds, tests and **RUNS** this crate on a `windows-latest` runner.

It **CONSUMES `crates/webview-shared`** rather than copying it: `LoadLifecycle`/`SharedLifecycle`, the `navigate` URL rule and the ADR-0008 off-thread `ipfs://` boundary are the same code the WebKitGTK and WKWebView backends run. ADR-0011 finding 5 predicted this reuse ("toolkit-free and reusable AS IS"); this crate is its third consumer, and the shape guard asserts the backend re-defines none of it.

Both trust hooks are real, never the silent no-ops `docs/adr/0005` exists to forbid: `install_ipfs` routes `ipfs://` through the same `werust_core::ipfs::resolve_ipfs_request` + verifying `fetcher` path every other edge uses, and `install_provider` injects the same `werust_core::provider` shim over the same bridge. The backend therefore declares `TrustHooks::all()` and passes `renderer::qualify`.

No chrome: no URL bar, no trust indicator, no menus, no debug view. The only window here is `host_in_bare_window`, a bare unfocused host that exists so the engine can be RUN. No packaging, no signing, no parity column.

## The origin question is already SETTLED — this task did not re-open it

`ipfs://` is **REGISTERED** here (`ICoreWebView2CustomSchemeRegistration` with `HasAuthorityComponent` + `TreatAsSecure`), not merely intercepted, so a page gets the real tuple origin `ipfs://<cid>`. That is not a reading of Microsoft's docs and it was not re-derived by this task: it was MEASURED on a `windows-latest` runner by `crates/windows-origin-probe` — verdict, evidence and re-run instructions in [`docs/spikes/windows-ipfs-origin-probe-on-ci/README.md`](../windows-ipfs-origin-probe-on-ci/README.md), recorded as ADR-0011 Amendment 2 — with a negative control (the identical run with `HasAuthorityComponent = false`) that reproduced the Android opaque-origin failure verbatim. So `origin_map.rs` stays an Android module and this shell serves `ipfs://` like desktop Linux, macOS and iOS.

The two flags are pinned as constants in `crates/windows-renderer/src/pure.rs` and asserted on the Ubuntu gate, so a later "simplification" that flips either one reds the gate rather than silently re-opening a field bug this repo has already paid for once.

The runtime is EVERGREEN and this corner regressed in stable 144 in January 2026 (WebView2Feedback #5495). If you suspect the ground moved, **re-run `.github/workflows/windows-origin-probe.yml`** — that probe asserts against a recorded `expected.json` and goes red naming the field that moved. Do not re-derive the answer by hand.

## The one structural constraint, and how it is answered

WebView2 fixes the SET of custom scheme NAMES at **environment** creation and makes it immutable for the browser-process lifetime, while the seam's `register_scheme_handler` is called AFTER construction. ADR-0011 finding 5 prescribes the answer and it is **not** a trait change:

* the container `HWND` is created **EAGERLY**, in `Webview2Renderer::new`, so `view_handle` is valid from construction;
* the environment + controller + engine are created **LAZILY**, on the first `navigate`, by which time the shell has registered its schemes.

This is the identical constraint, and the identical answer, the macOS backend needs for `WKWebViewConfiguration` (which is copied when the `WKWebView` is constructed). A scheme registered after realisation cannot be intercepted and is reported loudly on stderr rather than silently swallowed — the seam returns unit and must not widen, so the contract is stated instead of encoded.

## What CI proved (measured, not claimed)

Run **[30585224388](https://github.com/wighawag/werust/actions/runs/30585224388)**, workflow `windows-renderer`, job `windows-crates`, on a GitHub `windows-latest` runner (Windows 10.0.26100), **WebView2 Runtime 150.0.4078.65** — the SAME runtime build the origin probe measured its verdict on. Verbatim output of the trust-hooks step: [`trust-hooks-smoke-2026-07-30.txt`](trust-hooks-smoke-2026-07-30.txt). It ran against THIS tree; the first measuring run, [30584851232](https://github.com/wighawag/werust/actions/runs/30584851232), produced byte-identical output from the same code before this file was stamped from it. Both were GREEN; here is what each step actually established.

(Both were dispatched with `gh workflow run windows-renderer.yml --ref <branch>` against a branch carrying this code, which is the entire reason `windows-renderer-ci-leg` landed the workflow on `main` FIRST. Nothing below is a prediction.)

1. **The `#[cfg(windows)]` backend compiles against a real Windows SDK.** PASSED. `cargo build -p windows-renderer …` on `x86_64-pc-windows-msvc`: the COM wiring is COMPILED and LINKED (against `WebView2LoaderStatic.lib`), not merely parsed as the Ubuntu gate does.
2. **The crate's tests, and the shared code it consumes, pass on Windows.** PASSED. `cargo test -p windows-renderer …`: 11 pure-rule unit tests + the 12 source-shape assertions, plus the 5 `webview-shared` tests (the very lifecycle and off-thread-boundary tests the WebKitGTK and WKWebView backends rely on), 276 `werust-core`, 36 `fetcher`, 20 `renderer`, 23 `windows-origin-probe` and the 7 leg-shape assertions — all green on the third platform.
3. **Trust hook 2 (`ipfs://`) works end to end on a live WebView2.** PASSED. The smoke stored a page under its own CIDv1 and served it through the PRODUCTION verifying resolver across the SHARED off-thread boundary, behind a real `GetDeferral` (pinned in-memory retriever: offline, deterministic, no gateway). The load reported `LoadState::Finished` and `TrustPosture::ContentVerified`.
4. **The REGISTERED scheme gives the document its real tuple origin — observed from inside the BACKEND.** PASSED. The page reported its own origin as `ipfs://bafkreih2auwkjsxeesvxgr2f2r4gvruvniinu2x7ro5bodi57n3fo57uoy` and `isSecureContext: true`. That is ADR-0011 Amendment 2's verdict reproduced independently of the probe, by the shell code rather than by the experiment, on the same runtime build.
5. **Trust hook 1 (EIP-1193) works end to end on a live WebView2.** PASSED. The same page reported `window.ethereum` as an object and `request({ method: 'eth_chainId' })` resolving to `0x1`, which can only happen if the page -> native -> page round-trip completed over the script bridge — and therefore that the WebView2 bridge ADAPTER really does give the SHARED, unmodified `provider_shim` the channel shape it posts to.
6. **The fail-closed guarantee holds.** PASSED. The smoke's negative control served bytes that do NOT hash to the CID that named them: the load ended `LoadState::Failed` and still reported `UnverifiedOrigin`. A smoke where everything passes has measured nothing; this one can fail, and the control did.

The Ubuntu `verify` gate additionally covers, on every ordinary run: the pure decision rules (`crates/windows-renderer/src/pure.rs`, including the runtime-missing message a Windows runner can never exercise because its image HAS the runtime), and the source-shape guard `crates/windows-renderer/tests/windows_backend_shape.rs` over the `#[cfg(windows)]` source it cannot compile.

## What still awaits real Windows hardware (stated plainly)

**This work was WRITTEN blind, from Linux**, and then measured on CI. ADR-0011 Amendment 1 recorded that constraint up front and asked each new platform to land with an explicit statement of what was proven versus what remains analysis. Here is what run 30584851232 did NOT settle.

- **Nothing here has run on Windows HARDWARE, only on a `windows-latest` CI runner.** That is a real Windows with a real evergreen WebView2 Runtime, and it is what settles the trust hooks and the origin mechanism; it is not a desktop with a display, a GPU or a user in front of it.
- **The runtime-missing path is UNMEASURED at runtime, and cannot be measured on this runner.** The `windows-latest` image HAS the WebView2 Runtime (150.0.4078.65, read in the same run), so the honest-failure path is exercised only by its unit test on the Ubuntu gate: the message NAMES the runtime, POINTS at the download, keeps the platform's own detail, and arrives as an ordinary `RendererError::Backend` rather than a panic. Whether a bare Windows 10 box without the runtime *feels* right is unverified.
- **Everything about rendering quality, input, focus, HiDPI and window embedding is untouched by this task** and unverified. `send_pointer`/`send_key`/`send_scroll` rely on the WebView2 child HWND receiving real Win32 input exactly as the WebKitGTK backend relies on GTK's; neither is exercised here, and the run drove the engine through a bare unfocused host with no human present. Re-parenting the container HWND into a shell window is the sibling task's job and has never been done.
- **Not wired, deliberately, and inherited by `windows-win32-window-and-chrome`:** `OpenDevToolsWindow`, the DevTools-protocol console capture, the `AddWebResourceRequestedFilter("*")` network capture, and the `windows` column of `docs/platform-capability-matrix.toml`. ADR-0011's mapping lists all four as available; this task's scope excludes them, exactly as `macos-renderer` excluded them and `werust-macos` picked them up. See [`DECISIONS.md`](DECISIONS.md) §6.
- **The `werust://settings` internal page, the `_redirects` 3xx sink, `go_back`/`go_forward` and the ADR-0010 new-window route are wired but not DRIVEN.** They go through the same shared rules every other edge uses, and the shape guard asserts the wiring, but nothing in this run clicked a `_blank` link or pressed Back.
- **A same-document (SPA) `pushState` is wired to `add_SourceChanged` / `IsNewDocument == FALSE` but was not exercised.** The origin probe measured that `pushState` SUCCEEDS on a registered `ipfs://` origin here; that a successful `pushState` surfaces `LoadEvent::UrlChanged` through this backend is asserted structurally, not observed. Driving it is naturally the window task's ronan.eth-shaped test.
- **Scheme-registration ordering is a real constraint, not a hypothesis.** The SET of scheme names is fixed at environment creation and immutable for the browser-process lifetime, so schemes must be registered BEFORE the first navigation. The backend answers this by creating the environment LAZILY (eager container `HWND` so `view_handle` works from construction). A registration that arrives too late is reported on stderr, because the seam returns unit and must not be widened; see [`DECISIONS.md`](DECISIONS.md).
- **The runtime is EVERGREEN and cannot be pinned.** This exact corner regressed in stable 144 in January 2026 (WebView2Feedback #5495). A green run today is a measurement of runtime 150.0.4078.65, not a promise about tomorrow's.

## Re-running it

```
gh workflow run windows-renderer.yml --ref <branch>
```

By hand on a Windows box:

```
cargo run -p windows-renderer --example trust_hooks_smoke
```

From Linux, a fast type-check of the `#[cfg(windows)]` half with no CI round trip (a type-check, NOT a build and NOT a test):

```
LLVM_BIN=/usr/lib/llvm-19/bin ./docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh
```
