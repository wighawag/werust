//! A [`Renderer`](renderer::Renderer) backend over the Windows system webview
//! (Edge **WebView2**), in its own crate.
//!
//! This is the ENGINE half of the Windows desktop shell prescribed by
//! `docs/adr/0011-webview2-for-windows.md` (its "if Windows is funded later"
//! breakdown, sub-task 2; funded by Amendment 1, unblocked by Amendment 2). The
//! WINDOW half -- the Win32 window, URL bar, trust indicator, menus and debug
//! view -- is the sibling task `windows-win32-window-and-chrome`, and nothing
//! here paints chrome.
//!
//! # Why a separate crate
//!
//! `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so
//! nothing in it compiles on Windows. The toolkit-free half it used to hold --
//! the load-lifecycle state machine, the `navigate` URL rule and the ADR-0008
//! off-thread `ipfs://` boundary -- was MOVED into [`webview_shared`] by the
//! macOS engine task, and this crate CONSUMES that shared code rather than
//! copying it. Three system-webview backends, ONE definition of what a load
//! state, a rejected URL and a verified load mean. ADR-0011 finding 5 predicted
//! exactly this reuse ("toolkit-free and reusable AS IS"); this crate is the
//! third consumer.
//!
//! # What it leans on
//!
//! The COM/bindings layer is genuinely new. The SEAM BOOKKEEPING is not, and was
//! deliberately taken from the two existing WKWebView backends
//! (`crates/macos-renderer`, `crates/werust-ios/rust`) rather than re-invented:
//! the eager-container/lazy-engine split, the in-flight scheme-request table
//! keyed by id, the `Send`-outcome-only worker boundary, the `PendingEval` queue
//! that lets a `Send` script-message handler push a response back into a
//! main-thread-only engine, and the pump-on-`poll_event` rule that means a shell
//! which already drains the seam needs no extra wiring.
//!
//! # What qualifies it
//!
//! A backend qualifies for werust on the TRUST HOOKS, not on rendering
//! (`CONTEXT.md`, `docs/adr/0001`): [`Webview2Renderer::install_ipfs`] wires
//! `ipfs://` custom-scheme interception onto the hash-verified core path, and
//! [`Webview2Renderer::install_provider`] wires EIP-1193 provider injection onto
//! the script-message bridge. Both are REAL here (never the silent no-ops
//! `docs/adr/0005` exists to forbid), which is why
//! [`trust_hooks`](renderer::Renderer::trust_hooks) opts into both and
//! [`qualify`](renderer::qualify) accepts it.
//!
//! # Real `ipfs://` origins, by measurement
//!
//! The scheme is REGISTERED (`ICoreWebView2CustomSchemeRegistration` with
//! `HasAuthorityComponent` + `TreatAsSecure`), not merely intercepted, so a page
//! gets the real tuple origin `ipfs://<cid>`. That is not a reading of the docs:
//! it was MEASURED on a `windows-latest` runner by `crates/windows-origin-probe`
//! (ADR-0011 Amendment 2, `docs/spikes/windows-ipfs-origin-probe-on-ci/`), with a
//! negative control that reproduced the Android opaque-origin failure verbatim.
//! So `origin_map.rs` stays an Android module and this shell serves `ipfs://`
//! like desktop Linux, macOS and iOS. The runtime is EVERGREEN, so if that ground
//! ever moves, re-run `.github/workflows/windows-origin-probe.yml`; do not
//! re-derive it by hand.
//!
//! # The one structural constraint
//!
//! WebView2 fixes the SET of custom scheme NAMES at ENVIRONMENT creation and
//! makes it immutable for the browser-process lifetime, while the seam's
//! `register_scheme_handler` is called AFTER construction. The prescribed answer
//! (ADR-0011 finding 5) is a LAZY environment, NOT a trait change:
//! [`Webview2Renderer::new`] creates only the container `HWND` -- eagerly, so
//! [`view_handle`](renderer::Renderer::view_handle) is valid from construction --
//! and the environment plus controller are realised on the first
//! [`navigate`](renderer::Renderer::navigate), by which time the shell has
//! registered its schemes. It is the same shape the macOS backend needs for the
//! identical `WKWebViewConfiguration` constraint.
//!
//! # What compiles where
//!
//! The WebView2 half is `#[cfg(windows)]`, the same target-gating
//! `crates/macos-renderer` uses for `objc2` and `crates/werust-android` for JNI.
//! So:
//!
//! * The Ubuntu `verify` gate compiles and unit-tests [`pure`] -- the decisions
//!   the COM wiring makes -- and runs `tests/windows_backend_shape.rs`, the
//!   repo's source-shape guard, over the Windows source it cannot compile.
//! * `.github/workflows/windows-renderer.yml` builds the real backend on a
//!   `windows-latest` runner, runs its tests, and drives
//!   `examples/trust_hooks_smoke.rs` (a real WebView2 loading a real
//!   hash-verified `ipfs://` page and reading `window.ethereum` back over the
//!   bridge, with a negative control that must fail).
//!
//! What that CI run proved, and what still awaits real Windows hardware, is
//! stated step by step in `docs/spikes/windows-webview2-renderer-backend/README.md`.

pub mod pure;

#[cfg(windows)]
mod backend;

#[cfg(windows)]
pub use backend::{os_color_scheme, DevTools, OffThreadResolve, Webview2Renderer};
