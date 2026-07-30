//! The Windows backend's HOST-INDEPENDENT half: the decisions the WebView2
//! wiring makes, lifted out of the COM layer so they are pure functions the
//! Ubuntu `verify` gate compiles and unit-tests with no Windows SDK, no WebView2
//! Runtime and no Windows box.
//!
//! This is the discipline `crates/macos-renderer` (`pure.rs`) and
//! `crates/windows-origin-probe` (`facts.rs`) already use: anything that is a
//! RULE rather than an SDK call belongs here, where it can be asserted on every
//! ordinary `cargo test`. What is left in `backend.rs` is genuinely only COM
//! calls and event wiring.

use renderer::{OsColorScheme, RendererError, ScriptMessage};

/// The runtime this backend cannot start without, named exactly as a user would
/// search for it.
pub const WEBVIEW2_RUNTIME_NAME: &str = "Microsoft Edge WebView2 Runtime";

/// Where to get it. Microsoft's own download page for the Evergreen Runtime (and
/// its ~2MB bootstrapper).
pub const WEBVIEW2_RUNTIME_DOWNLOAD: &str =
    "https://developer.microsoft.com/microsoft-edge/webview2/";

/// `HasAuthorityComponent` for every scheme this backend registers.
///
/// TRUE is the whole reason the Windows shell serves REAL `ipfs://` origins: it
/// is what gives `ipfs://<cid>/path` the tuple origin `ipfs://<cid>` instead of
/// an opaque one, which is what makes a same-origin `fetch` resolve and
/// `history.pushState` not throw. MEASURED, not assumed — ADR-0011 Amendment 2,
/// `docs/spikes/windows-ipfs-origin-probe-on-ci/README.md`: the probe's negative
/// control is the identical run with this flag OFF, and it reproduced the
/// Android opaque-origin failure verbatim.
pub const SCHEME_HAS_AUTHORITY_COMPONENT: bool = true;

/// `TreatAsSecure` for every scheme this backend registers.
///
/// Documented to be effective only alongside
/// [`SCHEME_HAS_AUTHORITY_COMPONENT`]; it makes a registered-scheme document a
/// SECURE CONTEXT, which the probe measured as `true` for case A. Content whose
/// bytes were hash-verified has a better claim to a secure context than a TLS
/// origin does, so werust does not withhold it.
pub const SCHEME_TREAT_AS_SECURE: bool = true;

/// The key of the bridge NAME inside a page->host script-message envelope.
pub const BRIDGE_ENVELOPE_HANDLER: &str = "handler";

/// The key of the message BODY inside a page->host script-message envelope.
pub const BRIDGE_ENVELOPE_BODY: &str = "body";

/// The honest failure for a machine with no WebView2 Runtime: name the runtime,
/// point at the download, never crash.
///
/// This is a PRE-SPECIFIED user-visible behaviour, not a choice made here:
/// `docs/adr/0011-webview2-for-windows.md` finding 6 records that the Evergreen
/// Runtime ships with Windows 11 and is on "the vast majority" of Windows 10
/// machines, but that "no installer ever needed" is not a promise werust can
/// make — so a first run on a bare box must degrade honestly rather than abort.
/// It arrives through the seam's ordinary [`RendererError::Backend`], so a shell
/// shows it the same way it shows any other backend failure and NOTHING panics.
///
/// `detail` is whatever the platform said (the `HRESULT` text from
/// `GetAvailableCoreWebView2BrowserVersionString`, or the environment-creation
/// failure). It is appended rather than swallowed, because a runtime that is
/// present-but-broken must not be reported as absent.
#[must_use]
pub fn missing_runtime_error(detail: &str) -> RendererError {
    let detail = detail.trim();
    let mut message = format!(
        "the {WEBVIEW2_RUNTIME_NAME} is not available, so werust cannot start its rendering \
         engine on this machine. Install the Evergreen Runtime from {WEBVIEW2_RUNTIME_DOWNLOAD} \
         and start werust again."
    );
    if !detail.is_empty() {
        message.push_str(&format!(" (the system reported: {detail})"));
    }
    RendererError::Backend(message)
}

/// The scheme of an absolute URI (`ipfs://<cid>/x` -> `ipfs`), or [`None`] when
/// there is none.
///
/// The one `WebResourceRequested` handler serves EVERY registered scheme, so it
/// routes on this rather than on a per-scheme closure (WebView2 raises one event
/// for all of them).
#[must_use]
pub fn scheme_of(uri: &str) -> Option<&str> {
    let (scheme, rest) = uri.split_once("://")?;
    if scheme.is_empty() || rest.is_empty() {
        return None;
    }
    Some(scheme)
}

/// The `AddWebResourceRequestedFilter` pattern that catches every request of
/// `scheme`, and nothing else.
///
/// One filter per registered scheme, deliberately NON-overlapping: WebView2
/// raises `WebResourceRequested` once per MATCHING filter, so two filters that
/// both match a request would deliver it twice — and this handler answers
/// requests, so a double delivery is a double answer.
#[must_use]
pub fn scheme_filter(scheme: &str) -> String {
    format!("{scheme}://*")
}

/// The page-side ADAPTER that gives WebView2 the `window.webkit.messageHandlers.<name>`
/// shape the SHARED core shims post to.
///
/// werust's provider shim (`werust_core::provider::provider_shim`) and debug
/// shims (`werust_core::debug`) are toolkit-free core code shared by every edge,
/// and they post to `window.webkit.messageHandlers.<name>.postMessage(...)` —
/// WebKit's page-side API, which WebKitGTK, macOS and iOS all have natively.
/// WebView2 instead has ONE channel, `window.chrome.webview.postMessage`, so the
/// channel NAME has to travel inside the message.
///
/// The answer is the one the ANDROID edge already established (`BrowserActivity.kt`
/// `buildProviderScript`): the platform injects a small preamble that DEFINES
/// `window.webkit.messageHandlers.<name>` in terms of its own transport, and the
/// shared shim is then injected UNCHANGED on top. So this is not a new concept,
/// it is the second instance of an existing one — and crucially the core shims
/// stay one implementation rather than growing a per-platform fork.
///
/// The envelope is `{"handler": "<name>", "body": "<the posted string>"}`, read
/// back by [`parse_bridge_envelope`].
#[must_use]
pub fn bridge_adapter_script(names: &[String]) -> String {
    let mut body = String::from(
        "(function () {\n  \"use strict\";\n  window.webkit = window.webkit || {};\n  \
         window.webkit.messageHandlers = window.webkit.messageHandlers || {};\n  \
         function channel(name) {\n    return {\n      postMessage: function (m) {\n        \
         window.chrome.webview.postMessage(JSON.stringify({ handler: name, body: String(m) }));\n      \
         }\n    };\n  }\n",
    );
    for name in names {
        // JSON-encoded so a channel name can never break out of the string.
        let quoted = serde_json::Value::String(name.clone()).to_string();
        body.push_str(&format!(
            "  window.webkit.messageHandlers[{quoted}] = channel({quoted});\n"
        ));
    }
    body.push_str("})();\n");
    body
}

/// Read a page->host envelope back into the seam's [`ScriptMessage`].
///
/// The inverse of [`bridge_adapter_script`]'s envelope. A message that is not
/// this shape (a page calling `window.chrome.webview.postMessage` directly, for
/// its own reasons) is [`None`] and is DROPPED rather than mis-delivered to a
/// registered bridge: an unaddressed message has no handler to belong to.
#[must_use]
pub fn parse_bridge_envelope(raw: &str) -> Option<ScriptMessage> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let handler = value.get(BRIDGE_ENVELOPE_HANDLER)?.as_str()?;
    if handler.is_empty() {
        return None;
    }
    let body = value.get(BRIDGE_ENVELOPE_BODY)?.as_str()?;
    Some(ScriptMessage {
        handler: handler.to_string(),
        body: body.to_string(),
    })
}

/// `COREWEBVIEW2_WEB_ERROR_STATUS_OPERATION_CANCELED` — what WebView2 reports
/// when a navigation was deliberately cancelled (the user pressed Stop, or a new
/// navigation superseded an in-flight one).
pub const WEB_ERROR_STATUS_OPERATION_CANCELED: i32 = 14;

/// Decide whether a completed navigation is a real, user-visible load FAILURE,
/// and with what reason.
///
/// The twin of the macOS backend's `navigation_failure`, and it exists for the
/// same reason: `Stop` and a superseding navigation both complete the previous
/// navigation with a CANCELLED status, and reporting that would flash a spurious
/// error banner on every Stop. [`None`] means "do not move the lifecycle at all".
///
/// `recorded` is the reason the `ipfs://` scheme route already failed this load
/// with, if any. It is PREFERRED over the platform's status because it is the
/// honest one: "the bytes did not hash to their CID" is what happened, and
/// WebView2 can only say that some resource did not load. This is the Windows
/// equivalent of WebKitGTK's `finish_error` message reaching `load-failed`.
#[must_use]
pub fn navigation_failure(status: i32, recorded: Option<&str>) -> Option<String> {
    if status == WEB_ERROR_STATUS_OPERATION_CANCELED {
        return None;
    }
    if let Some(recorded) = recorded {
        let recorded = recorded.trim();
        if !recorded.is_empty() {
            return Some(recorded.to_string());
        }
    }
    Some(format!("load failed ({})", web_error_status_name(status)))
}

/// A legible name for a `COREWEBVIEW2_WEB_ERROR_STATUS`, so a failure reason
/// never reads as a bare integer.
#[must_use]
pub fn web_error_status_name(status: i32) -> String {
    let name = match status {
        0 => "unknown error",
        1 => "certificate common name is incorrect",
        2 => "certificate expired",
        3 => "client certificate contains errors",
        4 => "certificate revoked",
        5 => "certificate is invalid",
        6 => "server unreachable",
        7 => "timed out",
        8 => "invalid server response",
        9 => "connection aborted",
        10 => "connection reset",
        11 => "disconnected",
        12 => "cannot connect",
        13 => "host name not resolved",
        14 => "cancelled",
        15 => "redirect failed",
        16 => "unexpected error",
        17 => "authentication required",
        18 => "proxy authentication required",
        _ => return format!("WebErrorStatus({status})"),
    };
    name.to_string()
}

/// The HTTP reason phrase to answer an intercepted request with, for a status
/// the seam's [`SchemeResponse`](renderer::SchemeResponse) carried.
///
/// `CreateWebResourceResponse` takes the status code AND its reason phrase, so
/// the honest status the seam carries (200 for a resolved resource; 404 when a
/// content-addressed site named its OWN error page for a missing path through the
/// IPFS `_redirects` convention, IPIP-0002) arrives with a phrase that matches it
/// rather than a hard-coded `OK` that would contradict it.
#[must_use]
pub fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        410 => "Gone",
        500 => "Internal Server Error",
        // Anything else is answered with a phrase that claims nothing beyond the
        // code itself, which is better than asserting "OK" over a non-OK status.
        _ => "Status",
    }
}

/// Map the Windows personalisation setting `AppsUseLightTheme` to the shared
/// cross-platform [`OsColorScheme`] (`docs/adr/0009`: FOLLOW the OS, never force
/// dark).
///
/// Windows records the app light/dark preference at
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize` as a
/// DWORD: `1` means apps use the LIGHT theme, `0` means DARK. A missing or
/// unreadable value maps to [`OsColorScheme::NoPreference`], so werust never
/// guesses dark — the same rule the macOS `os_color_scheme_from_appearance` and
/// the GTK `os_color_scheme_from_portal` readers apply.
///
/// Note this reader is for the CHROME's benefit (the sibling window task paints
/// from the same signal every other edge paints from). The ENGINE itself follows
/// the OS through `COREWEBVIEW2_PREFERRED_COLOR_SCHEME_AUTO`, which is documented
/// to track the OS setting with no reading at all.
#[must_use]
pub fn os_color_scheme_from_apps_use_light_theme(value: Option<u32>) -> OsColorScheme {
    match value {
        Some(0) => OsColorScheme::Dark,
        Some(_) => OsColorScheme::Light,
        None => OsColorScheme::NoPreference,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_machine_without_the_runtime_is_told_what_is_missing_and_where_to_get_it() {
        // The pre-specified user-visible behaviour (ADR-0011 finding 6): NAME the
        // runtime, POINT at the download, and arrive as an ordinary seam error so
        // nothing crashes. Asserted here because this is exactly the case a
        // Windows CI runner can NEVER exercise: its image HAS the runtime.
        let error = missing_runtime_error("0x80070002 the system cannot find the file specified");
        let RendererError::Backend(message) = &error else {
            panic!("a missing runtime must be a backend error, got {error:?}");
        };
        assert!(
            message.contains(WEBVIEW2_RUNTIME_NAME),
            "the failure must NAME the missing runtime, got: {message}"
        );
        assert!(
            message.contains(WEBVIEW2_RUNTIME_DOWNLOAD),
            "the failure must POINT at the download, got: {message}"
        );
        assert!(
            message.contains("0x80070002"),
            "the platform's own detail must survive, so a present-but-broken runtime is not \
             reported as absent, got: {message}"
        );
        // And it is a Display-able error a shell can put in a banner unchanged.
        assert!(error.to_string().contains(WEBVIEW2_RUNTIME_DOWNLOAD));
    }

    #[test]
    fn a_runtime_failure_with_no_detail_still_reads_as_a_sentence() {
        let message = missing_runtime_error("   ").to_string();
        assert!(message.contains(WEBVIEW2_RUNTIME_NAME));
        assert!(!message.contains("()"), "got: {message}");
    }

    #[test]
    fn a_request_is_routed_to_its_scheme_and_each_scheme_gets_one_filter() {
        assert_eq!(scheme_of("ipfs://bafycid/index.html"), Some("ipfs"));
        assert_eq!(scheme_of("werust://settings"), Some("werust"));
        assert_eq!(scheme_of("https://example.com/"), Some("https"));
        for not_a_uri in ["", "ipfs", "://nohost", "ipfs://"] {
            assert_eq!(scheme_of(not_a_uri), None, "{not_a_uri:?} names no scheme");
        }
        assert_eq!(scheme_filter("ipfs"), "ipfs://*");
    }

    #[test]
    fn the_bridge_adapter_gives_webview2_the_shape_the_shared_core_shims_post_to() {
        // The core's provider shim is SHARED and unchanged: it posts to
        // `window.webkit.messageHandlers.<name>`. WebView2 has one channel, so
        // the adapter defines that shape over `window.chrome.webview` — exactly
        // as the Android edge's Kotlin preamble defines it over the JS interface.
        let script = bridge_adapter_script(&[
            werust_core::provider::PROVIDER_BRIDGE.to_string(),
            "werustSmoke".to_string(),
        ]);
        assert!(script.contains("window.chrome.webview.postMessage"));
        assert!(script.contains(&format!(
            "window.webkit.messageHandlers[\"{}\"]",
            werust_core::provider::PROVIDER_BRIDGE
        )));
        assert!(script.contains("window.webkit.messageHandlers[\"werustSmoke\"]"));
        // The SHARED shim must need no Windows-specific edit to work over it.
        let shim = werust_core::provider::provider_shim();
        assert!(
            shim.contains("window.webkit")
                && shim.contains("messageHandlers")
                && !shim.contains("chrome.webview"),
            "the shared shim must stay shared: the ADAPTER is what platform-specific"
        );
    }

    #[test]
    fn a_page_posted_envelope_is_delivered_to_the_named_bridge() {
        let message = parse_bridge_envelope(
            r#"{"handler":"werustProvider","body":"{\"id\":1,\"method\":\"eth_chainId\"}"}"#,
        )
        .expect("a well-formed envelope is delivered");
        assert_eq!(message.handler, "werustProvider");
        assert_eq!(message.body, r#"{"id":1,"method":"eth_chainId"}"#);
    }

    #[test]
    fn an_unaddressed_or_malformed_page_message_is_dropped_not_misdelivered() {
        // A page may call `window.chrome.webview.postMessage` for its own
        // reasons. Such a message belongs to no registered bridge, so it must not
        // be handed to one.
        for raw in [
            "",
            "hello",
            "{}",
            r#"{"body":"orphan"}"#,
            r#"{"handler":"","body":"x"}"#,
            r#"{"handler":"werustProvider"}"#,
            r#"{"handler":42,"body":"x"}"#,
        ] {
            assert!(
                parse_bridge_envelope(raw).is_none(),
                "{raw:?} addresses no bridge and must be dropped"
            );
        }
    }

    #[test]
    fn a_cancelled_navigation_is_not_a_load_failure() {
        // Stop() and a superseding navigation both complete the previous
        // navigation as CANCELLED. Reporting it would flash an error banner on
        // every Stop — the same rule the macOS backend applies to
        // NSURLErrorCancelled.
        assert_eq!(
            navigation_failure(WEB_ERROR_STATUS_OPERATION_CANCELED, None),
            None
        );
        assert_eq!(
            navigation_failure(
                WEB_ERROR_STATUS_OPERATION_CANCELED,
                Some("a stale scheme error")
            ),
            None
        );
    }

    #[test]
    fn a_real_failure_prefers_the_honest_verify_reason_over_the_platform_status() {
        // WebView2 can only say "a resource did not load". The scheme route knows
        // WHY, and that reason is what the user needs to see.
        assert_eq!(
            navigation_failure(16, Some("block hash mismatch for bafy…")),
            Some("block hash mismatch for bafy…".to_string())
        );
        // With nothing recorded, the platform status is named legibly rather than
        // as a bare integer.
        assert_eq!(
            navigation_failure(13, None),
            Some("load failed (host name not resolved)".to_string())
        );
        assert_eq!(
            navigation_failure(13, Some("   ")),
            Some("load failed (host name not resolved)".to_string())
        );
        assert_eq!(
            navigation_failure(999, None),
            Some("load failed (WebErrorStatus(999))".to_string())
        );
    }

    #[test]
    fn an_intercepted_response_carries_a_phrase_that_matches_its_honest_status() {
        // The `_redirects` / site-404 row: a site may declare its OWN error page
        // for a missing path, and answering it "200 OK" would lie about a page
        // the site said was missing.
        assert_eq!(reason_phrase(renderer::STATUS_OK), "OK");
        assert_eq!(reason_phrase(404), "Not Found");
        // Never "OK" for a status that is not OK.
        for status in [301u16, 418, 503] {
            assert_ne!(reason_phrase(status), "OK", "status {status} is not OK");
        }
    }

    #[test]
    fn only_an_explicit_dark_windows_setting_asks_for_dark() {
        // ADR-0009 on the Windows edge: FOLLOW the OS. `AppsUseLightTheme = 0`
        // is the only reading that means dark; an unreadable value supplies NO
        // preference rather than guessing.
        assert_eq!(
            os_color_scheme_from_apps_use_light_theme(Some(0)),
            OsColorScheme::Dark
        );
        assert_eq!(
            os_color_scheme_from_apps_use_light_theme(Some(1)),
            OsColorScheme::Light
        );
        assert_eq!(
            os_color_scheme_from_apps_use_light_theme(None),
            OsColorScheme::NoPreference
        );
        assert!(os_color_scheme_from_apps_use_light_theme(Some(0)).prefer_dark());
        assert!(!os_color_scheme_from_apps_use_light_theme(None).prefer_dark());
    }

    #[test]
    fn the_registered_scheme_flags_are_the_ones_the_probe_measured() {
        // The origin question is SETTLED by measurement (ADR-0011 Amendment 2):
        // `HasAuthorityComponent` + `TreatAsSecure` are what gave the document a
        // real `ipfs://<cid>` tuple origin, and the probe's negative control —
        // the identical run with the authority flag OFF — reproduced the Android
        // opaque-origin failure. Pinning them here means a later "simplification"
        // that flips either flag reds the Ubuntu gate instead of silently
        // re-opening a field bug this repo has already paid for once.
        assert_eq!(
            (SCHEME_HAS_AUTHORITY_COMPONENT, SCHEME_TREAT_AS_SECURE),
            (true, true),
            "the registered-scheme flags must stay the ones ADR-0011 Amendment 2 measured"
        );
    }
}
