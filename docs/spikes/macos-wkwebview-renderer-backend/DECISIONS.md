# Judgement calls made building the macOS WKWebView backend

The choices in `macos-wkwebview-renderer-backend` that another task, a user or a reviewer could be surprised were decided here. Each says what was chosen, why, what else was considered, and what it touches. Context and the honest verification split: [`README.md`](README.md).

## 1. `offthread.rs`'s shared home is a NEW crate, `webview-shared`, not `renderer`

**Chosen:** a new workspace crate `crates/webview-shared` holding `LoadLifecycle`, `SharedLifecycle`, `validate_url` and `offthread`.

**Why:** the obvious home would have been `crates/renderer` (the seam crate everything already depends on), but `offthread.rs` imports `werust_core::ipfs` and `werust_core` imports `renderer` — putting it there is a dependency cycle. That is a forced fact, not a preference. Given a new crate was required anyway, the load lifecycle went with it: `offthread::complete_ipfs_request` takes a `&SharedLifecycle`, so splitting them would have left the lifecycle stranded in the gtk4/webkit6 crate the macOS backend cannot depend on. The alternative — parameterising the completion over a `trait VerifiedLoadSink` so the lifecycle could stay put — would have widened an internal API to avoid a move, and made the two backends' "what a verified load means" reachable through two different types.

**What it touches:** `crates/webview-renderer` (now depends on and re-exports from the shared crate; its public surface is unchanged), and any future WebView2 backend, which ADR-0011 finding 5 already expects to be the third consumer. The mobile edges were deliberately NOT migrated: their lifecycle is edge-driven across a C-ABI/JNI boundary with a session history the platform does not own, a genuinely different shape, and dragging them in would have been scope creep with real risk.

**Naming:** `webview-shared` was checked against `CONTEXT.md`'s glossary. It does not re-mean "seam" (it holds no hot-swappable interface), it does not re-mean "painter" (it holds no display rule), and it does not overlap `werust-core` (which is the browsing core every OS edge sits over, not the internals of one KIND of backend). It is named for exactly what it is: the part of a SYSTEM-WEBVIEW backend that has no toolkit in it.

## 2. The `WKWebView` is created LAZILY; the container `NSView` is eager

**Chosen:** `MacosRenderer::new` builds only an `NSView` and a `WKWebViewConfiguration`; the `WKWebView` is realised on the first `navigate` (or explicitly via `realize`).

**Why:** `WKWebViewConfiguration` is COPIED by `-[WKWebView initWithFrame:configuration:]`, and `setURLSchemeHandler:forURLScheme:` lives on the configuration. So the set of intercepted schemes is fixed when the engine is constructed, while the seam's `register_scheme_handler` is called after construction. This is the SAME constraint `docs/adr/0011-webview2-for-windows.md` finding 5 records for WebView2's `ICoreWebView2CustomSchemeRegistration`, and it prescribes the same answer verbatim: "a lazy environment (eager container `HWND` so `view_handle` works, environment + controller created on first `navigate`), not a trait change". Adopting the ADR's own prescription keeps the two new desktop backends the same shape.

**What it touches:** the sibling `macos-appkit-window-and-chrome`, which embeds `view_handle()` — that is why the container is EAGER, so the handle is valid from construction. And any shell code that registers a scheme: the contract is "register every scheme before the first navigate", which is what the shells already do.

## 3. A late scheme registration is reported on stderr, not refused and not silently dropped

**Chosen:** if `register_scheme_handler` (or `install_verifying_scheme`) is called after the engine has been realised, the backend prints a loud line naming the scheme and does not register it.

**Why:** the alternatives are worse. Silently ignoring it is exactly the "silent no-op seam method" `docs/adr/0005` exists to forbid. Returning an error is impossible without widening `Renderer::register_scheme_handler` (it returns unit), and the task's first acceptance criterion is NO widening of the trait. Rebuilding the webview to pick the handler up would silently destroy the session history and the current page — a much bigger surprise than a log line. Panicking would turn a recoverable ordering mistake into a crash in a browser.

**What it touches:** it introduces a new user-visible-ish behaviour (a stderr line), which is why it is recorded here rather than buried. If the shell ever needs to add a scheme at runtime, the honest fix is a documented `recreate`-style operation on the backend, not a seam change.

## 4. There is no "case B" in the macOS origin probe, and the reason is MEASURED

**Chosen:** the probe runs case A (handler-served `ipfs://`) and a negative control, and additionally measures `+[WKWebView handlesURLScheme:@"https"]`.

**Why:** the Windows probe's case B was the internal `https://<cid>.ipfs.werust.invalid` origin — the fallback if case A failed. On WebKit that fallback cannot be built at all: WebKit refuses to give a natively-handled scheme to a `WKURLSchemeHandler`. Rather than assert that from Apple's documentation (the exact habit that cost this repo a field bug on Android), the probe measures it and puts the boolean in the report. The consequence is recorded in the verdict rule: a case-A failure on WebKit is a genuine BLOCKER, not a mechanism choice, and `verdict_from` says so instead of picking an unmeasured mechanism.

**What it touches:** the `Mechanism` enum deliberately keeps BOTH variants and their exact wire spellings from `crates/windows-origin-probe` (`registered-ipfs-scheme` / `internal-https-origin`), so the same cross-platform question is not named twice — a second name for one concept is the debt every later artifact would inherit. `InternalHttpsOrigin` can never be returned on macOS; it exists so the failing outcome has a name.

## 5. The negative control is "the same bytes with no handler-served origin", not a flipped flag

**Chosen:** the control loads the IDENTICAL canned page with `-[WKWebView loadHTMLString:baseURL:]` and a NIL base URL, with the registered scheme handler still installed on the same webview.

**Why:** the Windows control flipped exactly one registration flag (`HasAuthorityComponent`), which is the ideal one-variable control. WebKit exposes no such flag on `WKURLSchemeHandler`, so the smallest available variable is whether the document came from the handler at all. Keeping the handler INSTALLED matters: it makes "the handler never fired" a measured difference rather than an absence. The control is asserted on every re-run: if it ever starts PASSING, the whole run fails as a non-discriminating probe.

## 6. The canned probe page is a deliberate SIBLING of the Windows one, not a shared module

**Chosen:** `crates/macos-origin-probe/src/page.rs` restates the canned site rather than depending on `crates/windows-origin-probe`.

**Why:** the two pages differ where they must (WebKit's `window.webkit.messageHandlers` vs WebView2's `window.chrome.webview`), the two probes have different case sets, and both crates are `publish = false` measurement rigs, not libraries. A macOS crate depending on a crate named `windows-origin-probe` would be a worse kind of confusion than a restated page. What IS deliberately shared, so the three platforms' evidence is directly comparable, is the CID (the same `ronan.eth` fixture root the Android probe and `origin_map.rs` use), the paths, and the names of the measured facts.

## 7. Off-thread completions are applied from `poll_event`, not from a main-queue dispatch

**Chosen:** the verifying scheme route spawns a worker thread, sends back only the `Send` `RetrievalOutcome`, and applies the completion in `MacosRenderer::pump_scheme_completions`, which `poll_event` calls on every drain.

**Why:** ADR-0008 requires the blocking CAR fetch + per-block verify to stay off the UI thread and the completion to be marshalled back onto it. The GTK backend does that with `gio::spawn_blocking` + `MainContext::spawn_local`. The macOS analogue would be `dispatch_async(dispatch_get_main_queue())`, but the value that must survive the hop includes a retained `WKURLSchemeTask`, which is not `Send` — so it would have to be smuggled across a `Send` boundary. Draining on the pump keeps the task on the main thread from start to finish, needs no additional FFI, and costs nothing: every shell already drains `poll_event`. `pump_scheme_completions` is public so a driver with its own loop (the CI smoke) can pump explicitly.

**What it touches:** the sibling window task's main loop. Its obligation is exactly the obligation the GTK shell already has — drain the seam — so nothing new is required of it.

## 8. The Linux type-check harness is COMMITTED, as a script rather than a workspace member

**Chosen:** [`typecheck-macos-from-linux.sh`](typecheck-macos-from-linux.sh) — a script that builds a scratch workspace OUTSIDE the repo, symlinks the real macOS sources, and swaps `werust-core`/`fetcher` for tiny API-compatible stand-ins so `ring` (which cannot cross-compile to an Apple target from Linux) stays out of the graph.

**Why:** this task wrote roughly 700 lines of `objc2` wiring blind. Without a local type-check, every typo costs a CI round trip on a runner that is not even triggered on most pushes. `objc2` is pure Rust, so `cargo check --target aarch64-apple-darwin` genuinely type-checks it with no SDK; the only obstacle is one C-compiling transitive dependency, and the stand-ins remove it. Committing the harness means the sibling `macos-appkit-window-and-chrome` inherits the loop instead of rediscovering it.

**Why not a workspace member:** the stand-ins are deliberately FAKE. A crate in the workspace named `werust-core` that is not `werust-core` would be a trap; keeping the whole thing in a temp directory the script recreates each run makes it impossible to accidentally build against.

**What it touches:** nothing in the product. It is a development loop, and the README says plainly that it is not a build and does not replace the `macos-14` job.
