# macOS: the WKWebView `Renderer` backend (engine only) — what landed, and what is proven by what

Task: `macos-wkwebview-renderer-backend`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the "how `macos-desktop-build` should be split" block, sub-task 2, funded by its Amendment 1. Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md). Sibling task that puts a window and chrome on top of this: `macos-appkit-window-and-chrome`.

## What landed

Three crates and one CI leg.

- **`crates/webview-shared`** — the toolkit-free half every system-webview backend shares, **MOVED** out of `crates/webview-renderer` (which depends on gtk4/webkit6 unconditionally and therefore cannot host it): the `LoadLifecycle` state machine, the `navigate` URL rule, and the ADR-0008 off-thread `ipfs://` boundary (`offthread.rs`). Moved, never copied — the source-shape guard asserts the old file is gone, that exactly one definition of `retrieve_off_thread`/`complete_ipfs_request` exists, and that both desktop backends consume it. ADR-0011 finding 5 predicted this reuse; a future WebView2 backend is its third consumer.
- **`crates/macos-renderer`** — the `Renderer` implementation over `WKWebView`, with **no widening of the trait** (the guard pins the seam's method list). Navigation, session history, the load lifecycle, same-document SPA URL tracking, the script-message bridge and custom-scheme interception all go through the seam onto real WebKit APIs: `WKNavigationDelegate`, `WKUIDelegate` (ADR-0010's new-window-in-place), KVO on `WKWebView.URL`, `WKScriptMessageHandler` + document-start `WKUserScript`, and `WKURLSchemeHandler`.
- **`crates/macos-origin-probe`** — the WebKit analogue of `crates/windows-origin-probe`: canned bytes, no core, no IPFS, no network, a negative control, and a recorded verdict the run is asserted against.
- **`.github/workflows/macos-renderer.yml`** — a job on the existing `macos-14` runner that builds the backend, runs its tests, drives both trust hooks on a live WKWebView, and runs the origin probe.

Both trust hooks are real, never the silent no-ops `docs/adr/0005` exists to forbid: `install_ipfs` routes `ipfs://` through the same `werust_core::ipfs::resolve_ipfs_request` + verifying `fetcher` path desktop and both mobile edges use, and `install_provider` injects the same `werust_core::provider` shim over the same bridge. The backend therefore declares `TrustHooks::all()` and passes `renderer::qualify`.

No chrome: no URL bar, no trust indicator, no menus, no debug view. The only window here is `host_in_bare_window`, a borderless off-screen host that exists so the engine can be RUN. No signing, no packaging.

## What CI proves

These are things the `macos-renderer` job establishes by COMPILING and RUNNING on a real macOS runner, not by reading documentation.

1. **The `#[cfg(target_os = "macos")]` backend compiles against a real SDK.** `cargo build -p macos-renderer` on `macos-14`.
2. **The shared, moved code still passes on macOS.** `cargo test -p webview-shared` runs the very lifecycle and off-thread-boundary tests the WebKitGTK backend relies on, on the other desktop platform.
3. **Trust hook 2 (`ipfs://`) works end to end on a live `WKWebView`.** `examples/trust_hooks_smoke.rs` stores a page under its own CIDv1, serves it through the production verifying resolver across the shared off-thread boundary (with a pinned in-memory retriever, so the run is offline and deterministic), and asserts the load reports `TrustPosture::ContentVerified`.
4. **Trust hook 1 (EIP-1193) works end to end on a live `WKWebView`.** The same page reports back that `window.ethereum` is an object and that `request({ method: 'eth_chainId' })` RESOLVES — which can only happen if the page → native → page round-trip completed over the script bridge.
5. **The fail-closed guarantee holds.** The smoke's negative control serves bytes that do NOT hash to the CID that named them; the load must FAIL and must still report `UnverifiedOrigin`. A smoke where everything passes has measured nothing.
6. **The `WKURLSchemeHandler` origin behaviour, measured.** `crates/macos-origin-probe` reports the document origin, `isSecureContext`, whether a SvelteKit-shaped same-origin `fetch` resolves *and* fires the handler, and whether `pushState` throws — with a negative control (the identical bytes with a nil base URL, i.e. an opaque origin, and the handler still installed) that must reproduce the Android failure shape.
7. **Why there is no macOS "case B".** The probe MEASURES `+[WKWebView handlesURLScheme:@"https"]` rather than asserting it from Apple's docs: WebKit handles `https` itself and will not hand it to a `WKURLSchemeHandler`, so the Android/Windows internal-`https` fallback (`origin_map.rs`) is not constructible here. On WebKit the handler-served scheme is not the preferred mechanism, it is the only one.

The Ubuntu `verify` gate additionally covers, on every ordinary run: the pure decision rules (`crates/macos-renderer/src/pure.rs`), the probe's verdict rule, canned site and CLI, and the source-shape guard `crates/macos-renderer/tests/macos_backend_shape.rs`.

## What still awaits a Mac (stated plainly)

**This work was built blind, from Linux.** ADR-0011 Amendment 1 recorded that constraint up front and asked each new platform to land with an explicit statement of what was proven by CI versus what remains analysis. Here it is.

- **The `macos-renderer` job has not yet been run from the machine that wrote this.** Every "CI proves" item above is a job that EXISTS and is wired to the trigger; the first green run on `macos-14` is what converts each of them from "expected" to "measured". Until then they are claims about a job, not results from one.
- **`expected.json` is therefore a PREDICTION, not yet a recording.** It is written as a falsifiable one on purpose: the probe asserts against it, so the first run either confirms the prediction (and the file becomes a recorded verdict, to be re-stamped with the real OS/WebKit build) or goes RED naming the exact field that moved. Its `recorded` field says so in words. The field the prediction is least confident about is `secure_context` on case A: WebKit's secure-context list does not include custom schemes, and there is no `TreatAsSecure` equivalent to a `WKURLSchemeHandler`, so it is predicted FALSE — and it does not decide the verdict either way (only origin + fetch + handler-fired + pushState do).
- **Compilation of the Objective-C wiring was type-checked, not linked, locally.** The macOS sources were checked against `aarch64-apple-darwin` from Linux (which works for `objc2` bindings, since they are pure Rust declarations), so the seam wiring is type-correct. That is strictly weaker than a real build: it does not link against the frameworks and it cannot run a single message send. The harness is committed as [`typecheck-macos-from-linux.sh`](typecheck-macos-from-linux.sh) so the sibling window task gets the same fast loop instead of discovering typos one CI round trip at a time; run it before pushing macOS code.
- **Everything about rendering quality, input, focus, HiDPI and the responder chain is untouched by this task** and unverified. The `send_pointer`/`send_key`/`send_scroll` seam methods rely on AppKit's responder chain exactly as the WebKitGTK backend relies on GTK's; neither is exercised here.
- **The `werust://settings` internal page and the `_redirects` 3xx sink are wired but not driven.** They go through the same routes desktop uses; nothing here loads one.
- **Scheme registration ordering is a real constraint, not a hypothesis.** `WKWebViewConfiguration` is copied when the `WKWebView` is constructed, so schemes must be registered BEFORE the first navigation. The backend answers this by creating the engine LAZILY (eager container `NSView` so `view_handle` works from construction) — ADR-0011 finding 5's prescribed answer for the identical WebView2 constraint. A registration that arrives too late is reported on stderr, because the seam returns unit and must not be widened; see `DECISIONS.md`.

## The recorded verdict, and re-running it

Pinned for re-runs: [`expected.json`](expected.json).

On demand, from the Actions tab: run the `macos-renderer` workflow. It also runs on `main` and on pull requests when the backend, the probe or the recorded verdict changes.

By hand on a Mac:

```
cargo run -p macos-origin-probe -- --expected docs/spikes/macos-wkwebview-renderer-backend/expected.json --out macos-origin-probe-report.json
cargo run -p macos-renderer --example trust_hooks_smoke
```

Run the probe with NO `--expected` to get a pure recording run (it reports and does not judge), which is what you want the first time and after a deliberate re-decision.

WebKit ships with the OS and cannot be pinned, so the probe does not merely report: it ASSERTS and exits non-zero naming the field that moved. A red `macos-renderer` job means the ground under werust's `ipfs://` serving mechanism has shifted **on both WebKit shells at once** — macOS and iOS use the same `WKURLSchemeHandler` class — and the verdict must be re-decided and re-recorded with the reason, not silently overwritten.

## Relationship to the iOS caveat this retires

`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md` ("iOS parity") records that iOS does not share Android's opaque-origin cause *by mechanism analysis*, with the runtime confirmation left as recorded steps awaiting a Mac. ADR-0011 cites that caveat as the honesty cost of building blind.

The probe here addresses it at the mechanism level: `WKURLSchemeHandler` is one WebKit class, and what the macOS run measures about the origin a handler-served document receives is a statement about the WebKit port, not about AppKit. It is not a *full* retirement of the caveat — it does not build the iOS app, load `ronan.eth`, or click a blog link — and the DIAGNOSIS addendum says exactly that rather than claiming more.
