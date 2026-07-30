//! A [`Renderer`](renderer::Renderer) backend over the macOS system webview
//! (`WKWebView`), in its own crate.
//!
//! This is the ENGINE half of the macOS desktop shell prescribed by
//! `docs/adr/0011-webview2-for-windows.md` (its "how `macos-desktop-build` should
//! be split", sub-task 2; funded by Amendment 1). The WINDOW half -- the AppKit
//! window, URL bar, trust indicator, menus and debug view -- is the sibling task
//! `macos-appkit-window-and-chrome`, and nothing here paints chrome.
//!
//! # Why a separate crate
//!
//! `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so
//! nothing in it compiles on macOS. The parts of it that never touched GTK -- the
//! load-lifecycle state machine, the `navigate` URL rule and the ADR-0008
//! off-thread `ipfs://` boundary -- were therefore **MOVED** into
//! [`webview_shared`] (not copied), and this crate builds on that shared code.
//! Two desktop backends, ONE definition of what a load state, a rejected URL and
//! a verified load mean.
//!
//! # What qualifies it
//!
//! A backend qualifies for werust on the TRUST HOOKS, not on rendering
//! (`CONTEXT.md`, `docs/adr/0001`): [`MacosRenderer::install_ipfs`] wires
//! `ipfs://` custom-scheme interception onto the hash-verified core path, and
//! [`MacosRenderer::install_provider`] wires EIP-1193 provider injection onto the
//! script-message bridge. Both are REAL here (never the silent no-ops
//! `docs/adr/0005` exists to forbid), which is why
//! [`trust_hooks`](renderer::Renderer::trust_hooks) opts into both and
//! [`qualify`](renderer::qualify) accepts it.
//!
//! # What compiles where
//!
//! The `WKWebView` half is `#[cfg(target_os = "macos")]`, the same target-gating
//! `crates/windows-origin-probe` uses for WebView2 and `crates/werust-android`
//! for JNI. So:
//!
//! * The Ubuntu `verify` gate compiles and unit-tests [`pure`] -- the decisions
//!   the wiring makes -- and runs `tests/macos_backend_shape.rs`, the repo's
//!   source-shape guard, over the macOS source it cannot compile.
//! * `.github/workflows/macos-renderer.yml` builds the real backend on a
//!   `macos-14` runner, runs its tests, drives `examples/trust_hooks_smoke.rs`
//!   (a real `WKWebView` loading a real hash-verified `ipfs://` page and reading
//!   `window.ethereum` back over the bridge), and runs the sibling
//!   `crates/macos-origin-probe`.
//!
//! What CI proves versus what still awaits a Mac is stated explicitly in
//! `docs/spikes/macos-wkwebview-renderer-backend/README.md`.

pub mod pure;

#[cfg(target_os = "macos")]
mod backend;

#[cfg(target_os = "macos")]
pub use backend::{MacosRenderer, OffThreadResolve};
