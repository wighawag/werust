//! The TOOLKIT-FREE half every system-webview [`Renderer`](renderer::Renderer)
//! backend shares: the load-lifecycle state machine, the URL rule `navigate`
//! applies, and the off-UI-thread `ipfs://` resolution boundary.
//!
//! # Why this crate exists
//!
//! werust's first backend, `webview-renderer`, binds WebKitGTK and therefore
//! depends on `gtk4` + `webkit6` UNCONDITIONALLY — nothing in it compiles on
//! macOS or Windows. But three things inside it never touched GTK at all:
//!
//! * [`LoadLifecycle`] — the [`LoadState`](renderer::LoadState) /
//!   [`LoadEvent`](renderer::LoadEvent) / [`TrustPosture`](renderer::TrustPosture)
//!   state machine a backend's load signals drive.
//! * [`validate_url`] — the rule that decides which URL strings a backend will
//!   hand to its engine at all.
//! * [`offthread`] — the ADR-0008 concurrency boundary: run the blocking
//!   CAR fetch + per-block verify on a worker, apply the completion (finish the
//!   request, mark the posture) back on the marshalling thread.
//!
//! When the macOS WKWebView backend (`crates/macos-renderer`) needed all three,
//! they were **MOVED** here rather than copied, so the two desktop backends
//! cannot drift in what a load state, a rejected URL, or a verified load MEANS.
//! `docs/adr/0011-webview2-for-windows.md` (finding 5) already predicted this:
//! the `offthread.rs` split is "toolkit-free and reusable unchanged". Its THIRD
//! consumer is now real — `crates/windows-renderer`, the WebView2 backend, uses
//! all three here unchanged, marshalling the off-thread boundary with a WebView2
//! deferral instead of `gio::spawn_blocking` or a main-queue hop.
//!
//! # What is NOT here
//!
//! Anything that needs a toolkit or an SDK: the webview object, its signals, the
//! view handle, input forwarding. Those stay in the per-platform backend crate.
//! The mobile edges (`werust-android`, `werust-ios`) also keep their OWN
//! backends: their lifecycle is EDGE-DRIVEN across a C-ABI/JNI boundary with a
//! session history the platform does not own, which is a genuinely different
//! shape from a backend that owns a live webview — so they are deliberately not
//! forced onto [`LoadLifecycle`] here.

use renderer::RendererError;

pub mod lifecycle;
pub mod offthread;

pub use lifecycle::{LoadLifecycle, SharedLifecycle};

/// Validate a URL for [`Renderer::navigate`](renderer::Renderer::navigate),
/// rejecting unusable ones.
///
/// A system webview can navigate any absolute URL its engine understands; the
/// day-one path is `http(s)://`, and the trust hook adds `ipfs://` (task
/// `ipfs-scheme-resolution-through-renderer-seam`). A URL with no scheme, or an
/// empty one, is not something to hand to the engine, so it is rejected with
/// [`RendererError::InvalidUrl`] and never starts a load — the bad text stays in
/// the URL bar for the user to fix.
///
/// Shared so every backend that owns a live webview applies the SAME rule: the
/// WebKitGTK backend and the macOS WKWebView backend both call this one.
pub fn validate_url(url: &str) -> Result<(), RendererError> {
    match url.split_once("://") {
        Some((scheme, rest)) if !scheme.is_empty() && !rest.is_empty() => Ok(()),
        _ => Err(RendererError::InvalidUrl(url.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absolute_url_with_a_scheme_and_a_target_is_accepted() {
        for url in [
            "https://example.com/",
            "http://example.com/",
            "ipfs://bafycid/index.html",
            "werust://settings",
        ] {
            assert!(validate_url(url).is_ok(), "{url} must be navigable");
        }
    }

    #[test]
    fn a_scheme_less_or_empty_url_is_rejected_without_starting_a_load() {
        for url in ["", "not-a-url", "://nohost", "https://"] {
            assert_eq!(
                validate_url(url),
                Err(RendererError::InvalidUrl(url.to_string())),
                "{url} must be refused"
            );
        }
    }
}
