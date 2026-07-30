//! **Gate 0 of the Windows work** (`docs/adr/0011-webview2-for-windows.md`,
//! step 0 of its breakdown): the Windows analogue of the committed Android
//! probe `SpaClientNavOriginTest.kt`.
//!
//! # The question
//!
//! werust does not need "a document renders"; it needs a page served from an
//! intercepted `ipfs://` URL to do a same-origin `fetch('/blog/__data.json')`
//! and a `history.pushState` without throwing — what every SvelteKit
//! client-side navigation does. On Android that FAILED: an intercepted document
//! gets an OPAQUE origin, Blink rejects the fetch before the network stack, and
//! `pushState` throws `SecurityError`. That cost a field bug and the internal
//! `https://<cid>.ipfs.werust.invalid` origin map
//! (`crates/werust-android/rust/src/origin_map.rs`).
//!
//! WebView2 is the same Blink engine, but unlike Android's interception hook it
//! has real scheme REGISTRATION (`ICoreWebView2CustomSchemeRegistration` with
//! `HasAuthorityComponent` + `TreatAsSecure`), which Microsoft documents as
//! giving an http-like tuple origin. Whether that holds for `fetch` is an OPEN
//! WebView2 bug ([#4328], open since 2024-01-28), and the neighbouring
//! behaviour regressed in stable runtime 144 in January 2026 ([#5495]). The
//! runtime is EVERGREEN and cannot be pinned. So this is MEASURED, not read.
//!
//! # The shape
//!
//! Canned bytes, no werust core, no IPFS, no network. Two cases:
//!
//! * **Case A** — `ipfs://` registered with `HasAuthorityComponent` +
//!   `TreatAsSecure`.
//! * **Case B** — the internal `https://<cid>.ipfs.werust.invalid` origin.
//!
//! For each: the document's `origin` string, whether a same-origin `fetch`
//! RESOLVES *and* fires `WebResourceRequested` (both, not either), and whether
//! `pushState` throws. [`facts::mechanism_from`] turns those into the verdict.
//!
//! # Why it stays re-runnable
//!
//! The recorded verdict lives in
//! `docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json` and the probe
//! ASSERTS against it, so a later evergreen-runtime change to this corner turns
//! the `windows-origin-probe` workflow red with the exact field that moved,
//! instead of silently invalidating a decision the Windows shell was built on.
//!
//! [#4328]: https://github.com/MicrosoftEdge/WebView2Feedback/issues/4328
//! [#5495]: https://github.com/MicrosoftEdge/WebView2Feedback/issues/5495

pub mod cli;
pub mod facts;
pub mod page;

#[cfg(windows)]
pub mod win;
