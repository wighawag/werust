//! The macOS PRODUCT: an AppKit window that PAINTS werust's chrome over the
//! `WKWebView` backend.
//!
//! This is the WINDOW half of the macOS desktop shell prescribed by
//! `docs/adr/0011-webview2-for-windows.md` (its "how `macos-desktop-build` should
//! be split", sub-task 3). The ENGINE half is [`macos_renderer`] — a `Renderer`
//! implementation over `WKWebView`, with the trust hooks and the load lifecycle —
//! and nothing in it paints. This crate is the other side of that line: it opens
//! an `NSWindow`, embeds the backend's view, and reflects the shared
//! [`BrowserShell`](werust_core::BrowserShell)'s state in AppKit widgets.
//!
//! # It PAINTS; it does not DERIVE
//!
//! Every display RULE already lives in the toolkit-free `werust-core`: the status
//! line, the trust indicator and its explanation, the error banner's text and
//! severity, the invalid-entry badge, the load-progress fraction and hint (task
//! `desktop-chrome-presentation-into-core`), plus the debug view's row text and
//! level/trust classes (moved there by this task). This crate re-implements NONE
//! of them. That is not tidiness: the same derivation was hand-written in Kotlin
//! and in Swift and had already drifted — the trust EXPLANATION shipped
//! desktop-only for months — and a third native window would have minted a fourth
//! copy.
//!
//! The seam between "derive" and "paint" is [`paint`]: one host-independent
//! module that calls the core's rules and hands the AppKit layer plain values
//! (strings, booleans, fractions, colours). It compiles and is unit-tested on the
//! Ubuntu `verify` gate AGAINST THE REAL CORE, so the display half of this window
//! is covered by an ordinary `cargo test` even though no Mac is present. [`window`]
//! is then the thin AppKit layer: it assigns those values to `NSTextField`s,
//! `NSButton`s, an `NSProgressIndicator` and an `NSMenu`, and forwards user
//! actions to the shell.
//!
//! # What compiles where
//!
//! The AppKit half is `#[cfg(target_os = "macos")]`, the same target-gating
//! `crates/macos-renderer` uses for WebKit and `crates/windows-origin-probe` for
//! WebView2. So:
//!
//! * The Ubuntu `verify` gate compiles and unit-tests [`paint`] and runs
//!   `tests/macos_window_shape.rs`, the source-shape guard, over the AppKit
//!   source it cannot compile.
//! * `.github/workflows/macos-renderer.yml` builds the real window on a `macos-14`
//!   runner, runs its tests, and drives `examples/window_smoke.rs` — a REAL
//!   `NSWindow` with the real toolbar, error surface, menu and debug view,
//!   constructed and pumped off-screen — so the Objective-C wiring is executed,
//!   not merely parsed.
//!
//! What that job proves versus what still awaits a Mac with a human in front of
//! it is stated step by step in
//! `docs/spikes/macos-appkit-window-and-chrome/README.md`, including the manual
//! verification steps for everything a CI runner cannot judge.
//!
//! # The product decisions it FOLLOWS (never re-decides)
//!
//! * `docs/adr/0009` — FOLLOW the OS colour scheme, never force dark. On macOS
//!   that costs nothing and is done by NOT acting: AppKit propagates the effective
//!   `NSAppearance` into both the chrome and the `WKWebView`'s web process, so
//!   this window sets no appearance at all (the guard asserts it never does).
//! * `docs/adr/0010` — a `target="_blank"` / `window.open` navigates IN PLACE
//!   until tabs exist. That is the ENGINE's `WKUIDelegate` hook over the shared
//!   `renderer::new_window_action`; this window neither opens a second window nor
//!   re-decides the rule.
//! * `loading-progress-in-the-url-bar-not-a-banner` — in-flight progress lives IN
//!   the URL bar and must not displace the page. The progress indicator is laid
//!   out INSIDE the URL bar's rectangle in the fixed-height toolbar, so a
//!   navigation never resizes the page view. Only a FAILURE may take a banner.
//! * `docs/adr/0006`/`0007` — the trust posture is a product surface and a mutable
//!   name is never labelled "verified". Both come from the core's derivation
//!   verbatim.
//!
//! # Scope
//!
//! Unsigned and unpackaged: no code signing, no notarization, no `.app` bundle
//! (task `macos-release-packaging-leg`), and no `macos` column in the
//! platform-capability matrix (task `macos-parity-column-and-stub-tasks`, which
//! runs after this so the cells describe what really shipped).

pub mod paint;

#[cfg(target_os = "macos")]
pub mod window;
