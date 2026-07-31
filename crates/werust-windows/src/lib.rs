//! The Windows PRODUCT: a Win32 window that PAINTS werust's chrome over the
//! WebView2 backend.
//!
//! This is the WINDOW half of the Windows desktop shell prescribed by
//! `docs/adr/0011-webview2-for-windows.md` (sub-task 3 of its Windows split,
//! funded by Amendment 1). The ENGINE half is [`windows_renderer`] -- a
//! `Renderer` implementation over Edge WebView2, with the trust hooks and the
//! load lifecycle -- and nothing in it paints. This crate is the other side of
//! that line: it opens a top-level `HWND`, re-parents the backend's container
//! window into it, and reflects the shared
//! [`BrowserShell`](werust_core::BrowserShell)'s state in Win32 controls.
//!
//! # It PAINTS; it does not DERIVE
//!
//! Every display RULE already lives in the toolkit-free `werust-core`: the status
//! line, the trust indicator and its EXPLANATION, the error banner's text and
//! severity, the invalid-entry badge, the load-progress fraction and its
//! sentence, the exported CSS-class sets and the `CssClassFamily` aggregate, and
//! the debug view's row text and level/trust classes. This crate re-implements
//! NONE of them, and it does not even carry its own copy of the CARRIER that
//! reads them: it consumes [`desktop_paint`], the shared half the AppKit window
//! landed and this task extracted.
//!
//! That is not tidiness. The same derivation was hand-written in Kotlin and in
//! Swift and had already drifted -- the trust EXPLANATION shipped desktop-only for
//! months -- and a third native window would have minted a fourth copy, which is
//! the specific failure ADR-0011's Consequences section warns about.
//!
//! The seam between "derive" and "paint" is therefore [`paint`]: it hands the
//! Win32 layer plain values (strings, booleans, fractions, colours), compiles on
//! the Ubuntu `verify` gate against the REAL core, and is unit-tested there. The
//! Win32 layer ([`window`], [`chrome`], [`debugview`]) assigns those values to an
//! `EDIT`, `BUTTON`s, `STATIC`s, a `msctls_progress32`, an `HMENU`, a
//! `SysTabControl32` and two `SysListView32`s, and forwards user actions to the
//! shell. It contains no rule.
//!
//! # What compiles where
//!
//! The Win32 half is `#[cfg(windows)]`, the same target-gating
//! `crates/windows-renderer` uses for WebView2 and `crates/werust-macos` for
//! AppKit. So:
//!
//! * The Ubuntu `verify` gate compiles and unit-tests [`profile`] (the durable
//!   user-data-folder rule), compiles and tests the whole shared [`paint`] half,
//!   and runs `tests/windows_window_shape.rs`, the source-shape guard, over the
//!   Win32 source it cannot compile.
//! * `.github/workflows/windows-renderer.yml` builds the real window on a
//!   `windows-latest` runner, runs its tests, and drives
//!   `examples/window_smoke.rs` -- a REAL top-level window with the real toolbar,
//!   error surface, menu and debug view, constructed and pumped off-screen -- so
//!   the Win32 wiring is EXECUTED, not merely parsed.
//!
//! What that job proves versus what still awaits a Windows box with a human in
//! front of it is stated step by step in
//! `docs/spikes/windows-win32-window-and-chrome/README.md`, including the manual
//! verification steps for everything a CI runner cannot judge.
//!
//! # It scales itself, because the manifest promised Windows it would
//!
//! `app.manifest` declares `PerMonitorV2`: Windows must NOT bitmap-scale this
//! process, because it scales itself. [`dpi`] is where it does — one seam holding
//! the chrome's 96-DPI design metrics and the `MulDiv` arithmetic that turns them
//! into the pixels of whichever monitor the window is on, fed by ONE
//! `GetDpiForWindow` read and rebuilt on `WM_DPICHANGED`. The seam is pure and
//! host-independent, so the Ubuntu gate unit-tests the arithmetic no CI runner
//! can otherwise reach: a runner has no DPI at all. What only a human on a scaled
//! display can confirm is listed in
//! `docs/spikes/windows-chrome-must-scale-with-the-display-dpi/README.md`.
//!
//! # The binary is a GUI app
//!
//! `src/main.rs` links with `#![cfg_attr(windows, windows_subsystem =
//! "windows")]`: a browser must not drag a console window onto the desktop
//! beside itself, which is what the first run on real hardware found it doing.
//! The `cfg` gate is what keeps everything above true -- the crate still compiles
//! on every host, so its host-independent half stays inside the Ubuntu gate.
//!
//! The cost is that there is no console to print to, and this shell has an honest
//! startup failure to report (a machine with no WebView2 Runtime is TOLD so). So
//! [`startup`] gives that failure a surface on both launch paths -- the launching
//! terminal's console when there is one, a message box when there is not -- and
//! `main.rs` picks one per launch. Why, and what was rejected:
//! `docs/spikes/windows-gui-subsystem-no-console-window/DECISIONS.md`.
//!
//! # The product decisions it FOLLOWS (never re-decides)
//!
//! * `docs/adr/0009` -- FOLLOW the OS colour scheme, never force dark. The ENGINE
//!   does this natively (`COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO`). The CHROME
//!   cannot: Win32 hands an owner-drawn surface no appearance at all. So it READS
//!   the OS setting through the engine crate's ONE registry read, mapped by the
//!   shared `renderer::OsColorScheme` rule (`NoPreference` paints LIGHT -- werust
//!   never guesses dark), and re-reads it on `WM_SETTINGCHANGE`. One source, two
//!   surfaces.
//! * `docs/adr/0010` -- a `target="_blank"` / `window.open` navigates IN PLACE
//!   until tabs exist. That is the ENGINE's `add_NewWindowRequested` hook over the
//!   shared `renderer::new_window_action`; this window neither opens a second
//!   window for it nor re-decides the rule.
//! * `loading-progress-in-the-url-bar-not-a-banner` -- in-flight progress lives IN
//!   the URL bar and must not displace the page. The progress bar is laid out
//!   INSIDE the URL bar's rectangle in the fixed-height toolbar, so a navigation
//!   never resizes the page. Only a FAILURE may take a banner.
//! * `docs/adr/0006`/`0007` -- the trust posture is a product surface and a
//!   mutable name is never labelled "verified". Both come from the core's
//!   derivation verbatim.
//! * The `web-inspector` capability -- devtools are the PLATFORM's own
//!   (`OpenDevToolsWindow`), never a werust re-implementation, and gated on a
//!   debug build exactly as the WebKitGTK, iOS and Android rows are.
//!
//! # Scope
//!
//! Unsigned and unpackaged: no installer, no code signing, no zip on a Release
//! (task `windows-release-packaging-leg`), and no `windows` column in the
//! platform-capability matrix (task `windows-parity-column-and-stub-tasks`, which
//! runs after this so the cells describe what really shipped).

/// The window's host-independent half, SHARED with the AppKit window
/// (`crates/desktop-paint`). Named `paint` here for the same reason
/// `werust-macos` names it that: it is where deriving stops and painting starts.
pub use desktop_paint as paint;

// The chrome's ONE DPI seam. NOT `cfg`-gated, for the same reason `profile` is
// not: `app.manifest` declares `PerMonitorV2`, so the WINDOW owes Windows its own
// scaling, and the arithmetic that owes it is pure — so it is compiled and
// unit-tested on the Ubuntu gate instead of being discovered on a 200% display
// (task `windows-chrome-must-scale-with-the-display-dpi`).
pub mod dpi;
pub mod profile;

#[cfg(windows)]
pub mod chrome;
#[cfg(windows)]
pub mod debugview;
// Where werust's own words go now that the binary is a GUI app with no console
// of its own (task `windows-gui-subsystem-no-console-window`).
#[cfg(windows)]
pub mod startup;
#[cfg(windows)]
pub mod win32;
#[cfg(windows)]
pub mod window;
