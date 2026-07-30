//! **The macOS origin measurement** for the WKWebView `Renderer` backend (task
//! `macos-wkwebview-renderer-backend`): the direct analogue of
//! `crates/windows-origin-probe`, and of the committed Android probe
//! `SpaClientNavOriginTest.kt`.
//!
//! # The question
//!
//! werust does not need "a document renders"; it needs a page served from an
//! intercepted `ipfs://` URL to do a same-origin `fetch('/blog/__data.json')`
//! and a `history.pushState` without throwing -- what every SvelteKit
//! client-side navigation does. On Android that FAILED: an intercepted document
//! gets an OPAQUE origin, Blink rejects the fetch before the network stack, and
//! `pushState` throws `SecurityError`. That cost a field bug and the internal
//! `https://<cid>.ipfs.werust.invalid` origin map
//! (`crates/werust-android/rust/src/origin_map.rs`).
//!
//! WebKit was EXPECTED to behave differently: a `WKURLSchemeHandler`-served
//! document was expected to get a real `scheme://host` tuple origin, which is the
//! whole serving model of Capacitor/Ionic apps. That expectation is why
//! `docs/adr/0011-webview2-for-windows.md` says macOS is the better-placed
//! platform. In this repo it WAS only a **MECHANISM ANALYSIS**: the iOS shell
//! ships on that mechanism and its runtime confirmation awaited a Mac
//! (`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`, "iOS
//! parity").
//!
//! **It is now MEASURED.** This probe ran on a `macos-14` runner on 2026-07-30
//! (macOS 14.8.7 Build 23J520, AppleWebKit/605.1.15) and recorded a real
//! `ipfs://<cid>` tuple origin, a same-origin `fetch` that resolved AND fired the
//! handler, and a `pushState` that did not throw, with the negative control
//! failing in the same run. That holds for BOTH WebKit ports, because the
//! mechanism under test (`WKURLSchemeHandler`) is the same class on macOS and
//! iOS. Verdict, evidence and the honest CI-versus-hardware split:
//! `docs/spikes/macos-wkwebview-renderer-backend/README.md`; verbatim run:
//! `docs/spikes/macos-wkwebview-renderer-backend/probe-report-2026-07-30.json`.
//!
//! This repo has already paid once for settling a platform-origin question from
//! documents instead of a device. Not repeating that is the whole point.
//!
//! # The shape
//!
//! Canned bytes, no werust core, no IPFS, no network. Two cases on one runner:
//!
//! * **Case A** -- the page served from `ipfs://<cid>/` by a registered
//!   `WKURLSchemeHandler`.
//! * **The NEGATIVE CONTROL** -- the IDENTICAL bytes and the identical page,
//!   loaded with `-[WKWebView loadHTMLString:baseURL:]` and a NIL base URL, which
//!   WebKit gives an OPAQUE origin. The registered handler stays installed on the
//!   same webview, so "the handler never fired" is a measured difference rather
//!   than an absence.
//!
//! For each: the document's `origin` string, `isSecureContext`, whether a
//! same-origin `fetch` RESOLVES *and* fires the scheme handler (both, not
//! either), and whether `pushState` throws. [`facts::verdict_from`] turns those
//! into the verdict.
//!
//! # Why there is no "case B"
//!
//! The Windows probe carried a case B -- the internal
//! `https://<cid>.ipfs.werust.invalid` origin `origin_map.rs` implements -- as the
//! fallback mechanism if case A failed. **That fallback is not available on
//! WKWebView**: WebKit refuses to hand a scheme it handles natively (`https`,
//! `http`, `file`, `about`, `data`, `blob`, ...) to a `WKURLSchemeHandler`. The
//! probe does not take that from the documentation either: it MEASURES it, with
//! `+[WKWebView handlesURLScheme:]`, and reports it as
//! [`CaseFacts`](facts::CaseFacts)-adjacent context on the [`Report`](facts::Report).
//! So on macOS case A is not merely the preferred mechanism, it is the only one,
//! and a case-A failure would be a genuine blocker rather than a fallback.
//!
//! # Why it stays re-runnable
//!
//! The recorded verdict lives in
//! `docs/spikes/macos-wkwebview-renderer-backend/expected.json` and the probe
//! ASSERTS against it, so a later WebKit change to this corner turns the
//! `macos-renderer` workflow red with the exact field that moved, instead of
//! silently invalidating a decision two WebKit shells were built on.

pub mod cli;
pub mod facts;
pub mod page;

#[cfg(target_os = "macos")]
pub mod mac;
