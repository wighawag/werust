# Decisions — `windows-win32-window-and-chrome`

Task: `windows-win32-window-and-chrome`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), sub-task 3 of its Windows breakdown (funded by Amendment 1). What landed and what is proven by what: [`README.md`](README.md).

These are the choices that were NOT already made for me by the ADR, the spike's section-5 mapping, the existing product rules or the engine task. Each says what was chosen, why, what was rejected, and what else it touches — so a reviewer can ratify or reverse it rather than discover it.

## 1. TOOLKIT: plain Win32 + the common controls, no cross-platform toolkit (the decision the task asked for by name)

**Chosen:** the window is a plain Win32 top-level `HWND` with system controls — `BUTTON`, `EDIT`, `STATIC`, `msctls_progress32`, `tooltips_class32`, `HMENU`, `SysTabControl32`, `SysListView32` — drawn by `user32`/`comctl32`, with GDI only for the brushes and text colours the chrome owns. The only new dependency in `crates/werust-windows` is the `windows` crate (0.62, already in the tree and already the engine's), behind seven features. No `webview2-com` here at all: the window reaches the engine through the `Renderer` seam plus one small devtools handle.

**Why:** three reasons, in order of weight.

1. **This is the trust-carrying path.** werust's whole thesis is that the origin is not trusted by default; a browser chrome that reports the trust posture is exactly where a fat dependency tree is least welcome. `windows` + `comctl32` is the platform, not a dependency in the supply-chain sense.
2. **The research prescribed it** (`docs/spikes/windows-platform-research/README.md`), and werust has deliberately adopted no cross-platform toolkit: GTK is the Linux edge's, AppKit is macOS's, and each edge paints the SAME core derivation with its own widgets. A toolkit adopted for Windows alone would be a fourth UI stack, not a unification.
3. **It matches the sibling exactly.** The AppKit window is hand-laid-out AppKit; this is hand-laid-out Win32, with the same strips in the same order and the same arithmetic. Two windows that differ only in the widget calls are two windows one person can hold in their head.

**Rejected:**

- **`wry` + `tao` (or `winit`)**, the obvious "everyone uses it" answer. `wry` IS a webview wrapper — werust already has its own `Renderer` seam over WebView2 and would use none of it — and `tao`/`winit` give a window and an event loop but NO widgets at all, so the toolbar, the menu, the tooltip and the two list views would still be hand-written Win32 on top of a large event-loop dependency. It buys nothing this window needs and adds a stack werust would then have to keep in step on three platforms.
- **`egui` / `iced` / another immediate-mode or Elm-style Rust UI.** These draw their own pixels, which means werust's chrome would stop following the OS theme (ADR-0009 is a product rule here, not a preference), would not use real Win32 accessibility, and would look like nothing else on the machine. They are also the wrong shape for hosting a native child `HWND` (the WebView2 container) inside the layout.
- **GTK-on-Windows.** Ruled out by the research: it needs a shipped GTK runtime, breaks the native look, and would make the Windows build depend on the pkg-config stack that already cannot build on a Windows runner (`crates/werust`, `crates/webview-renderer` are measured RED there).

**Touches:** the packaging follow-on (`windows-release-packaging-leg`) — see decision 4, the visual-styles manifest, which is a packaging concern; and the parity column, which will describe a native-widget chrome rather than a drawn one.

## 2. The paint carrier was EXTRACTED to `crates/desktop-paint`, not copied

**The gap:** the AppKit window landed its host-independent half as `werust-macos::paint` — the `ChromePaint` carrier, the menu items, the debug rows, the capture points, and the PALETTE that gives the core's exported class names a colour. The Win32 window needs exactly that, and the obvious move is a second copy.

**Chosen:** move the module VERBATIM (with its tests) into a new toolkit-free crate `crates/desktop-paint`, consumed by BOTH windows, each re-exporting it as `paint` so nothing about the macOS crate's surface changed.

**Why:** a second copy would have been the fourth hand-maintained chrome in this repo (GTK, Kotlin, Swift, AppKit… Win32), and the palette specifically would have become its THIRD transcription of the same hex values. This project has already paid for that failure once: the trust EXPLANATION shipped desktop-only for months because each edge hand-wrote the chrome. The task's own words are "re-implement NOTHING locally"; a carrier that exists twice is a rule that can drift twice.

`werust-core` was NOT an option, and that is checked rather than assumed: `the_stylesheet_stays_in_the_edge_and_core_gains_no_styling_concept` fails the gate if the core so much as mentions `color:`. The palette is edge concern; a SHARED EDGE crate is the layer that did not exist yet.

**Rejected:** (a) copying the module (above); (b) moving it into `werust-core` (forbidden by the layering, and by a test); (c) making the GTK edge consume it too — GTK has a real stylesheet and toggles CSS classes, so it needs the core's class NAMES but not an in-code palette; rewriting a working painter for no new guarantee is not an extraction, it is a risk. Instead `crates/desktop-paint/tests/gtk_stylesheet_agreement.rs` now asserts that every colour the GTK stylesheet declares is the SAME colour the shared palette holds, which closes the drift hole without the rewrite.

**Touches:** `crates/werust-macos` (its `paint` module is now a re-export), `crates/werust-core/tests/chrome_css_class_set_edge_wiring_shape.rs` and `crates/werust-macos/tests/macos_window_shape.rs` (both follow the file to its new path — the tests travelled with the code), and BOTH CI legs' path filters and package lists (`macos-renderer.yml`, `windows-renderer.yml`), since a change to the shared half changes what both windows show.

## 3. Two ENGINE changes, both forced by hosting the engine in a real window

The sibling task recorded one engine line changed for the same reason ("the `WKWebView`'s autoresizing mask"). Windows needed two.

**3a. The container's window proc resizes the controller.** WebView2 has no autoresizing: a controller keeps the bounds it was given at realisation. Hosted in a resizable shell window, the page would keep the size it had when the engine started. **Chosen:** handle `WM_SIZE` in the container's OWN window proc (`crates/windows-renderer`), where the controller is borrowed through the window's `GWLP_USERDATA` slot and `Drop` clears the slot before closing the controller. **Rejected:** a public `resize_to_container()` the shell calls — the shell CANNOT call it, because by then the backend is a `Box<dyn Renderer>` behind the seam, and widening the seam for a Win32 detail is exactly what ADR-0011 says not to do. Keeping it in the container's proc means the page follows its container for EVERY host, with no per-shell wiring.

**3b. A `DevTools` handle, and the debug-build gate.** Devtools are `OpenDevToolsWindow` on the live `ICoreWebView2`, which is unreachable once the backend is boxed. **Chosen:** the engine exposes a small `DevTools` handle (an `Rc` filled at realisation) that the shell takes BEFORE boxing — the same move the GTK shell makes with `backend.web_view().clone()` for the WebKitGTK inspector — so the COM call stays in the engine crate and the window crate needs no `webview2-com`. In the same change the engine now sets `AreDevToolsEnabled(cfg!(debug_assertions))`: the `web-inspector` capability's RECORDED rule is that the platform's own devtools are gated on a debug build (WebKitGTK's `enable-developer-extras`, iOS's `isInspectable`, Android's `setWebContentsDebuggingEnabled` all are), and WebView2 defaults the flag to TRUE, so leaving it unset would have made Windows the one platform that ignores the rule. This FOLLOWS an existing decision; it does not make a new one.

**Touches:** `crates/windows-renderer`'s public surface (two additions, no removals) and its release behaviour (a release build is no longer silently inspectable — which is the point). The Windows `web-inspector` matrix cell belongs to the parity follow-on.

## 4. No comctl32 v6 manifest: classic visual styles, deferred to packaging

**Chosen:** ship without an application manifest, so the process links comctl32 5.82 and the controls draw in the classic style.

**Why:** a v6 manifest is an embedded Win32 RESOURCE, which needs a build script and a resource compiler (`embed-resource` or equivalent) — a build-time dependency and a packaging concern, in a task whose scope is explicitly "unpackaged, unsigned". Everything the window DOES works identically on 5.82: report-mode list views, tab controls, progress bars and tooltips all predate it.

**The honest consequence:** the chrome looks dated on Windows 11 until the manifest lands. That is a REAL user-visible gap, so it is named here, in the README's "what awaits real hardware", and in the follow-on task `windows-release-packaging-leg` rather than left to be discovered.

**UPDATE (2026-07-31, `windows-release-packaging-leg`): the manifest has landed, and one sentence of this decision was wrong.** `crates/werust-windows/app.manifest` now carries the comctl32 v6 dependency (and per-monitor-v2 DPI awareness), embedded by `crates/werust-windows/build.rs` through the MSVC linker rather than a resource compiler; the reasoning is in [that task's record](../windows-release-packaging-leg/README.md). Two corrections to what is written above and in §6: the v6 dependency does NOT make system-drawn push BUTTONs follow dark mode — that has no public API at all and needs an undocumented uxtheme path (`work/notes/findings/win32-common-controls-dark-mode-needs-more-than-a-v6-manifest-2026-07-31.md`), so the light-buttons-in-dark-mode gap survives the manifest and now has its own task; and the switch to v6 silently disables `PBM_SETBARCOLOR`, so the URL bar's progress strip had to opt that one control out of theming to keep the shared palette's colour. `TOOL_INFO_V2_SIZE` in `chrome.rs` needs no change: it is the size BOTH versions accept, which is exactly why it was chosen.

## 5. The trust EXPLANATION is a real tooltip control, and the smoke reads it back

**Chosen:** one `tooltips_class32` per window carries the trust indicator's explanation and the URL bar's progress sentence; the smoke asserts the explanation by sending `TTM_GETTEXT` and comparing with `trust_indicator_detail`.

**Why:** the tooltip is what macOS uses (`setToolTip:`) and what the GTK edge uses, so the surface is the same on all three desktops. And reading it BACK is the difference between "the string reached a struct" (which the Ubuntu gate already proves) and "the string reached a widget" (which only a Windows runner can). The explanation is the specific thing that shipped desktop-only for months; it deserves the stronger assertion.

**Rejected:** a second visible label for the explanation (it would crowd a toolbar that already carries a 210px phrase), and trusting the paint snapshot (that is the assertion the gate already makes; a smoke that only re-checks it measures nothing new).

## 6. ADR-0009 on a toolkit that propagates nothing

**Chosen:** the chrome READS the OS setting through `windows_renderer::os_color_scheme()` — the ONE registry read, which already lived in the engine crate beside the platform bindings — maps it with the SHARED `renderer::OsColorScheme` rule, paints its own surfaces, sets `DWMWA_USE_IMMERSIVE_DARK_MODE` on the title bar, and re-reads on `WM_SETTINGCHANGE`.

**Why:** AppKit propagates the effective appearance into every control, so the macOS window follows the OS by NOT acting; Win32 hands an owner-drawn `STATIC` no appearance at all. Following the OS therefore REQUIRES a read here. Doing it through the engine's existing reader (rather than a second `RegGetValueW` in the window crate) keeps one source: the engine follows the OS via `PREFERRED_COLOR_SCHEME_AUTO` and the chrome follows the same signal. `NoPreference` paints LIGHT, because the shared rule says werust never guesses dark.

**Known limit, stated:** push BUTTONs are system-drawn and do not honour dark mode, so a dark-mode window has dark chrome surfaces and light buttons. Recorded rather than hidden. (This originally read "without a v6 manifest", i.e. that decision 4's manifest would fix it. It does not — see the UPDATE under decision 4; the gap outlived the manifest and is now `work/tasks/backlog/windows-chrome-dark-mode-for-common-controls.md`, which is where the `follow-os-color-scheme` parity cell points.)

## 7. The Win32 constants the `windows` crate does not generate are spelled once

`LVS_REPORT`, `NM_CUSTOMDRAW`, `TCN_SELCHANGE`, `TTS_ALWAYSTIP`, `SS_LEFTNOWORDWRAP`, `PBM_SETPOS` and friends are `#define`s in the SDK headers rather than typed enums, so the bindings omit them. **Chosen:** declare them at the top of the module that uses them, each naming the header it comes from, rather than scattering bare hex through the calls. `LVITEMW` / `LVCOLUMNW` / `TCITEMW` are likewise spelled as `#[repr(C)]` structs in `debugview.rs` because only a few of their fields are used and the ABI must match exactly.

## 8. The durable profile is `%LOCALAPPDATA%\werust\WebView2`

Required by this task's own acceptance criterion (planted at Gate 3 of the engine task). `%LOCALAPPDATA%` because a browser profile is machine-local state and cache, not roaming documents; `werust\WebView2` so a future per-user directory (or a future backend's profile) is a sibling rather than a collision. An unreadable `%LOCALAPPDATA%` declines rather than inventing a path no Windows tool knows, and falls back to the engine's documented default — the rule and both branches are unit-tested on the Ubuntu gate (`crates/werust-windows/src/profile.rs`), and the CI smoke checks the folder really exists outside `%TEMP%` on a real machine.

**Noticed, NOT fixed here:** `werust_core::retrieval::settings_dir()` has no Windows branch at all (`$XDG_CONFIG_HOME` / `$HOME/.config`), so the retrieval-backend SETTING does not persist on Windows. That is the settings concept's own bug, not this window's; recorded at `work/notes/observations/settings-dir-has-no-windows-branch-2026-07-30.md`. This rule names the same vendor directory (`werust`) a core Windows branch would name, so the two converge rather than collide.

## 9. `werust-windows` is a separate binary, like `werust-macos`

Same reason, recorded once more because it is the first thing a reader asks: `crates/werust` depends on gtk4/webkit6 UNCONDITIONALLY and cannot build on Windows, and two binaries cannot share the name `werust` in one workspace. The headless verbs (`resolve`, `version`) stay in the toolkit-free `werust` binary and are not re-implemented here; this binary opens a window, which is the one thing only it can do.
