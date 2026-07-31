# macOS: the WKWebView `Renderer` backend (engine only) — what landed, and what is proven by what

Task: `macos-wkwebview-renderer-backend`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the "how `macos-desktop-build` should be split" block, sub-task 2, funded by its Amendment 1. Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md). Sibling task that puts a window and chrome on top of this: `macos-appkit-window-and-chrome`.

## What landed

Three crates and one CI leg.

- **`crates/webview-shared`** — the toolkit-free half every system-webview backend shares, **MOVED** out of `crates/webview-renderer` (which depends on gtk4/webkit6 unconditionally and therefore cannot host it): the `LoadLifecycle` state machine, the `navigate` URL rule, and the ADR-0008 off-thread `ipfs://` boundary (`offthread.rs`). Moved, never copied — the source-shape guard asserts the old file is gone, that exactly one definition of `retrieve_off_thread`/`complete_ipfs_request` exists, and that both desktop backends consume it. ADR-0011 finding 5 predicted this reuse; a future WebView2 backend is its third consumer.
- **`crates/macos-renderer`** — the `Renderer` implementation over `WKWebView`, with **no widening of the trait** (the guard pins the seam's method list). Navigation, session history, the load lifecycle, same-document SPA URL tracking, the script-message bridge and custom-scheme interception all go through the seam onto real WebKit APIs: `WKNavigationDelegate`, `WKUIDelegate` (ADR-0010's new-window-in-place), KVO on `WKWebView.URL`, `WKScriptMessageHandler` + document-start `WKUserScript`, and `WKURLSchemeHandler`.
- **`crates/macos-origin-probe`** — the WebKit analogue of `crates/windows-origin-probe`: canned bytes, no core, no IPFS, no network, a negative control, and a recorded verdict every re-run is asserted against. It has RUN: the verdict below is measured, and [`probe-report-2026-07-30.json`](probe-report-2026-07-30.json) is the verbatim output it was stamped from.
- **`.github/workflows/macos-renderer.yml`** — a job on the existing `macos-14` runner that builds the backend, runs its tests, drives both trust hooks on a live WKWebView, and runs the origin probe.

Both trust hooks are real, never the silent no-ops `docs/adr/0005` exists to forbid: `install_ipfs` routes `ipfs://` through the same `werust_core::ipfs::resolve_ipfs_request` + verifying `fetcher` path desktop and both mobile edges use, and `install_provider` injects the same `werust_core::provider` shim over the same bridge. The backend therefore declares `TrustHooks::all()` and passes `renderer::qualify`.

No chrome: no URL bar, no trust indicator, no menus, no debug view. The only window here is `host_in_bare_window`, a borderless off-screen host that exists so the engine can be RUN. No signing, no packaging.

## The MEASURED verdict

**A `WKURLSchemeHandler`-served document gets a REAL `ipfs://<cid>` tuple origin.** So a WebKit shell (macOS AND iOS) serves `ipfs://` the way desktop Linux and Windows do, and `origin_map.rs` stays an Android module.

Measured 2026-07-30 on a GitHub `macos-14` runner, **macOS Version 14.8.7 (Build 23J520)**, Xcode 15.4, **AppleWebKit/605.1.15**. Verbatim run: [`probe-report-2026-07-30.json`](probe-report-2026-07-30.json), from CI run [30563185521](https://github.com/wighawag/werust/actions/runs/30563185521). Pinned for re-runs: [`expected.json`](expected.json).

| | case A: handler-served `ipfs://` | negative control: the same bytes with no handler-served origin |
|---|---|---|
| document origin | `ipfs://bafybei…pfzq` | `null` (opaque) |
| secure context | **yes** (predicted no; see below) | no |
| same-origin `fetch('/blog/__data.json?x-sveltekit-invalidated=01')` | **`ok:200`** | `reject:TypeError` |
| the `WKURLSchemeHandler` fired for that fetch | **yes** | **no** |
| `history.pushState({}, '', '/blog/')` | **`ok:/blog/`** | `throw:SecurityError` |
| `<script type="module">`-shaped `import()` | `ok:module` | `reject:TypeError` |
| CSS `@font-face url()` reached the handler | yes | no |
| `navigator.serviceWorker.register('/sw.js')` | `reject:TypeError` | `unavailable` |

The informational rows decide nothing, but two are worth recording. `navigator.serviceWorker.register('/sw.js')` was refused (`TypeError`) on the real `ipfs://` origin, one more data point for the per-platform divergence already captured in `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md` (WebView2 answered `InvalidStateError` on the same case); and the CSS `@font-face url()` subresource DID reach the handler, so WebKit has no WebView2-#4362-shaped hole here either.

`+[WKWebView handlesURLScheme:@"https"]` measured **true**, which is why there is no macOS "case B": WebKit will not hand a scheme it handles natively to a `WKURLSchemeHandler`, so the Android/Windows internal-`https` fallback is not constructible here. On WebKit the handler-served scheme is not the preferred mechanism, it is the only one, and a case-A failure would be a genuine blocker rather than a mechanism choice.

Case A passing means something only because the control failed on the same runner, in the same process, with the same bytes and the same handler still installed: the control reproduces the Android failure shape exactly (opaque origin, the fetch rejected inside the engine before the handler, `pushState` throwing `SecurityError`, the handler asked for nothing at all). The control is asserted on every re-run; if it ever starts PASSING the run fails as a non-discriminating probe.

**The one field that moved, and why the verdict did not.** The prediction this file was first committed with said case A's `secure_context` would be FALSE (WebKit's secure-context list does not cover custom schemes and there is no `TreatAsSecure` equivalent for a `WKURLSchemeHandler`). It measured TRUE. That is better than predicted, it is the field the prediction itself named as least confident, and it decides nothing: the mechanism rests only on origin + fetch + handler-fired + `pushState`. `expected.json` is re-recorded WITH that reason rather than silently overwritten, which is the contract this probe holds itself to. Every other pinned field matched.

## What CI proved (measured, not claimed)

Run [30563185521](https://github.com/wighawag/werust/actions/runs/30563185521), `macos-renderer` on `macos-14`, against this branch's code. Step by step, because "the job is green" would be false: the job exited RED, on the probe's assertion step and nothing else.

1. **The `#[cfg(target_os = "macos")]` backend compiles against a real SDK.** PASSED. `cargo build -p macos-renderer -p macos-origin-probe` on macOS 14.8.7 / Xcode 15.4: the `objc2` wiring is COMPILED, not merely parsed.
2. **The macOS crates' tests, and the shared moved code, pass on macOS.** PASSED. `cargo test -p macos-renderer -p macos-origin-probe -p webview-shared`: 3 pure-rule tests + the 12 source-shape assertions, the probe's 20 decision-rule/canned-site/CLI tests, and the `webview-shared` tests all green on the other desktop platform. That last count was **5 in this run** and is **9 today**, and the difference is worth stating: the 5 were the three off-thread-boundary tests and the two `navigate` URL-rule tests. The `LoadLifecycle` state-machine tests this line once claimed were among them had stayed behind in the GTK-bound `webview-renderer` when the state machine itself moved, so the shared crate's central guarantee was exercised on Linux only. Task `macos-spike-doc-accuracy-and-harness-guard` MOVED those four beside the code they cover, so a `webview-shared` run on this runner now covers the lifecycle too.
3. **Trust hook 2 (`ipfs://`) works end to end on a live `WKWebView`.** PASSED. `examples/trust_hooks_smoke.rs` stored a page under its own CIDv1, served it through the production verifying resolver across the shared off-thread boundary (pinned in-memory retriever: offline, deterministic), and the load reported `TrustPosture::ContentVerified`. The page reported its own origin as `ipfs://bafkreigledotdonpj4hfupvfks64l3355rea2mznztbbbujjdeqxxrcvwu`, which is the origin result again, independently, from inside the BACKEND rather than the probe.
4. **Trust hook 1 (EIP-1193) works end to end on a live `WKWebView`.** PASSED. The same page reported `window.ethereum` as an object and `request({ method: 'eth_chainId' })` resolving to `0x1`, which can only happen if the page to native to page round-trip completed over the script bridge.
5. **The fail-closed guarantee holds.** PASSED. The smoke's negative control served bytes that do NOT hash to the CID that named them: the load ended `LoadState::Failed` and still reported `UnverifiedOrigin`. A smoke where everything passes has measured nothing.
6. **The `WKURLSchemeHandler` origin behaviour.** MEASURED, and the step exited RED. The probe ran, produced the report above and derived the `registered-ipfs-scheme` verdict, then exited non-zero because the run disagreed with the then-recorded `expected.json` on case A's `secure_context` (predicted `false`, measured `true`). That is the probe working exactly as designed: it names the field that moved instead of quietly accepting it. With `expected.json` re-stamped from the run, that comparison is clean, which `crates/macos-origin-probe/tests/recorded_verdict.rs` now replays on the Ubuntu gate.

The Ubuntu `verify` gate additionally covers, on every ordinary run: the pure decision rules (`crates/macos-renderer/src/pure.rs`), the probe's verdict rule, canned site and CLI, the source-shape guard `crates/macos-renderer/tests/macos_backend_shape.rs`, and the recording guard `crates/macos-origin-probe/tests/recorded_verdict.rs` (which pins `expected.json` to the committed verbatim run, so a hand-written verdict cannot pass for a measured one again).

## What still awaits a Mac (stated plainly)

**This work was WRITTEN blind, from Linux**, and then measured on CI. ADR-0011 Amendment 1 recorded that constraint up front and asked each new platform to land with an explicit statement of what was proven versus what remains analysis. Here is what the run above did NOT settle.

- **Nothing here has run on Mac HARDWARE, only on a `macos-14` CI runner.** That is a real macOS with a real WebKit and it is what settles the origin question; it is not a desktop with a display, a GPU or a user.
- **The local type-check is not a build, and stays that way.** The macOS sources are checkable against `aarch64-apple-darwin` from Linux (`objc2` bindings are pure Rust declarations), which catches typos without a CI round trip but links nothing and sends no message. The harness is committed as [`typecheck-macos-from-linux.sh`](typecheck-macos-from-linux.sh) so the sibling window task inherits the loop; run it before pushing macOS code, and treat the `macos-renderer` job as the actual verdict. It was RUN clean on 2026-07-31 (engine, window and probe, `aarch64-apple-darwin`) after task `macos-spike-doc-accuracy-and-harness-guard` repaired it for the `desktop-paint` extraction and put a temp-root guard on the `rm -rf` it does to its own scratch workspace: [`../macos-spike-doc-accuracy-and-harness-guard/DECISIONS.md`](../macos-spike-doc-accuracy-and-harness-guard/DECISIONS.md).
- **Everything about rendering quality, input, focus, HiDPI and the responder chain is untouched by this task** and unverified. The `send_pointer`/`send_key`/`send_scroll` seam methods rely on AppKit's responder chain exactly as the WebKitGTK backend relies on GTK's; neither is exercised here, and the CI run drove the engine through an off-screen host with no human in front of it.
- **The `werust://settings` internal page and the `_redirects` 3xx sink are wired but not driven.** They go through the same routes desktop uses; nothing here loads one.
- **Scheme registration ordering is a real constraint, not a hypothesis.** `WKWebViewConfiguration` is copied when the `WKWebView` is constructed, so schemes must be registered BEFORE the first navigation. The backend answers this by creating the engine LAZILY (eager container `NSView` so `view_handle` works from construction) — ADR-0011 finding 5's prescribed answer for the identical WebView2 constraint. A registration that arrives too late is reported on stderr, because the seam returns unit and must not be widened; see `DECISIONS.md`.

## The recorded verdict, and re-running it

Pinned for re-runs: [`expected.json`](expected.json), stamped from [`probe-report-2026-07-30.json`](probe-report-2026-07-30.json). The two are held together by `crates/macos-origin-probe/tests/recorded_verdict.rs` on the Ubuntu gate: the recorded verdict must diff CLEAN against the committed run, and its provenance must name the OS build, the WebKit build and the CI run. Editing the verdict by hand reds that test, which is the point.

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

That caveat is now settled at the mechanism level, by measurement rather than by reading. `WKURLSchemeHandler` is ONE WebKit class, and run [30563185521](https://github.com/wighawag/werust/actions/runs/30563185521) measured what a handler-served document actually receives on it: a real `ipfs://<cid>` tuple origin, a same-origin `fetch` that resolves AND reaches the handler, and a `pushState` that does not throw. That is a property of the WebKit port, which macOS and iOS share, not of AppKit, so the load-bearing half of the caveat holds for iOS too. The negative control failing in the same run is what makes that a measurement rather than a tautology.

It is still not a *full* retirement, and the DIAGNOSIS addendum says exactly that rather than claiming more: the run did not build the iOS app, load `ronan.eth` or click a blog link, so the residual risk it leaves open is something iOS-specific in `WKWebViewShellController`'s own wiring, not WebKit's origin model.
