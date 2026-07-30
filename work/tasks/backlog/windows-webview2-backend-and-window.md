---
title: "Windows desktop: a WebView2 `Renderer` backend driven from a Win32 window"
slug: windows-webview2-backend-and-window
blockedBy: [desktop-chrome-presentation-into-core]
covers: []
---

## What to build

The Windows shell, funded by Amendment 1 of `docs/adr/0011-webview2-for-windows.md` (reach: a general-purpose browser that runs only on Linux desktop plus two mobile OSes cannot be evaluated by most of the people it is for). Everything technical in that ADR stands; build to it.

A native werust on Windows: a new `Renderer` backend over **Edge WebView2**, bound with **`webview2-com`** + `webview2-com-sys` (0.39.1, what `wry` itself depends on; never the abandoned `webview2` crate), driven from a plain Win32 window that PAINTS the chrome from the shared derivation `desktop-chrome-presentation-into-core` produced.

**The origin question is SETTLED; do not re-litigate it.** The probe ran on 2026-07-30 (ADR-0011 Amendment 2, `docs/spikes/windows-ipfs-origin-probe-on-ci/`) and measured the verdict **`registered-ipfs-scheme`**: on WebView2 Runtime 150.0.4078.65, an `ipfs://` scheme registered with `HasAuthorityComponent = TRUE` + `TreatAsSecure = TRUE` gives a real tuple origin, a secure context, a same-origin `fetch` that resolves AND fires `WebResourceRequested`, and a working `pushState`, with a negative control reproducing the Android failure verbatim when the flag is off. So this shell serves REAL `ipfs://` origins like desktop and iOS, and `origin_map.rs` is NOT promoted. Re-run `.github/workflows/windows-origin-probe.yml` if you suspect the evergreen runtime moved under you; do not re-derive the answer by hand. The presentation extraction decides what the window paints from; without it this becomes a fourth hand-written copy of the display rules.

**Where things actually live** (both premises the earlier macOS task got wrong):

- The `Renderer` trait is `crates/renderer/src/lib.rs` (`pub trait Renderer`, line 695), NOT `crates/webview-renderer`.
- `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so nothing in it compiles on Windows. The Windows backend needs its own crate, and `crates/webview-renderer/src/offthread.rs` (genuinely toolkit-free: it imports only `fetcher`, `renderer`, `werust_core` and the crate's own `SharedLifecycle`) must MOVE to a shared home rather than be copied. If `macos-wkwebview-backend-and-window` already moved it, reuse it as-is.

**The mapping is already written.** `docs/spikes/windows-platform-research/README.md` section 5 maps every `Renderer` method onto WebView2 with NO trait widening, and several rows are more native than the existing edges: session history (`GoBack` / `get_CanGoBack` / `add_HistoryChanged`), the SPA same-document URL change (`add_SourceChanged` with `IsNewDocument == FALSE`, which desktop has to infer from `notify::uri`), a real status code on a scheme response (`CreateWebResourceResponse`, so the `_redirects` / site-404 row works), `PreferredColorScheme = AUTO` for ADR-0009, `add_NewWindowRequested` + `put_Handled(TRUE)` feeding the existing `renderer::new_window_action` for ADR-0010, and real Chrome devtools via `OpenDevToolsWindow`.

**The one structural constraint:** the SET of custom scheme NAMES is fixed at environment creation and immutable for the browser-process lifetime, while `register_scheme_handler` is called after construction today. The prescribed fix is a LAZY environment (create the container `HWND` eagerly so `view_handle` works, create the environment + controller on first `navigate`), NOT a trait change.

**ADR-0008 (retrieval off the UI thread) is satisfiable** with `WebResourceRequested` + `GetDeferral` / `Deferral::Complete`, reusing the existing off-thread boundary rather than inventing a second pattern.

**A user-visible default is pre-specified:** a machine WITHOUT the WebView2 Runtime must fail HONESTLY, naming the missing runtime and pointing at the download, never crash. Evergreen runtime is part of Windows 11 and present on most Windows 10 machines, but "no installer needed" is not a promise anyone can make.

**The debug-view presentation may already be shared by the time you start.** `desktop-chrome-presentation-into-core` moved the CHROME rules into core but left the DEBUG-VIEW row helpers (`console_level_css_class`, `console_source_line`, `console_row_text`, `network_status_text` / `_mime_text` / `_size_text` / `_trust_label` / `_trust_css_class`) private in the GTK edge; `macos-wkwebview-backend-and-window` owns extracting them. If that landed first, CONSUME the shared versions. If it did not, extract them here the same way (behaviour-preserving, tests moving with them) rather than re-deriving them in the Windows edge.

**Scope: the backend + the window + honest failure.** The `windows` parity-matrix column and its forced stub tasks, and the CI packaging leg, are separate tasks cut after this lands, mirroring the macOS sub-task structure. Toolchain note for whoever cuts CI: `x86_64-pc-windows-msvc` statically links `WebView2LoaderStatic.lib` (single-exe), while `*-pc-windows-gnu` needs `WebView2Loader.dll` shipped alongside; and the Ubuntu `verify` gate cannot compile a `#[cfg(windows)]` backend, so source-shape tests (this repo's existing pattern) plus a native `windows-latest` job are both needed.

ADR sizing for this step: 8 to 12 days for the backend, 8 to 14 for the window and chrome (lower because the extraction landed first).

## Acceptance criteria

- [ ] A native Windows binary opens a Win32 window with a WebView2 rendering content, driven by the shared `BrowserShell` (no browsing decision in the Windows edge).
- [ ] The `Renderer` trait from `crates/renderer` is implemented with NO widening; the scheme-name-set constraint is handled by lazy environment creation, not by changing the trait.
- [ ] Both trust hooks work: an `ipfs://<cid>` URL renders hash-verified content through the mechanism THE PROBE CHOSE, and a page sees the native EIP-1193 `window.ethereum`.
- [ ] A SvelteKit-style client-side navigation works (same-origin `fetch` + `pushState`), verified against the probe's recorded verdict rather than assumed.
- [ ] The chrome paints from the SHARED derivation, not a re-derivation, and the debug view paints from the shared debug-row helpers (consuming them if the macOS task already extracted them, extracting them here if not).
- [ ] The Windows code does not live in a crate depending on gtk4/webkit6; `offthread.rs` is shared, not copied.
- [ ] A machine without the WebView2 Runtime gets an honest, named failure.
- [ ] What was proven on CI versus what remains analysis awaiting real hardware is stated explicitly (Amendment 1's recorded constraint).

## Prompt

> Goal: the Windows desktop shell: a `Renderer` backend over WebView2 via `webview2-com` (trait at `crates/renderer/src/lib.rs:695`; `crates/webview-renderer` depends on gtk4/webkit6 unconditionally so it cannot host it, and `offthread.rs` must be shared not copied), plus a Win32 window that PAINTS the chrome from the shared derivation. Build to the method-by-method mapping in `docs/spikes/windows-platform-research/README.md` section 5, which needs no trait widening; the one constraint (custom scheme NAMES are fixed at environment creation) is handled by creating the environment lazily on first `navigate`, with the container HWND eager so `view_handle` works. Serve `ipfs://` through the mechanism the origin probe CHOSE, do not re-litigate it. Trust hooks are the qualification bar, not rendering. A machine without the WebView2 Runtime must fail honestly naming the runtime. Parity column and packaging are separate follow-on tasks. State plainly what CI proved versus what still awaits real hardware.
