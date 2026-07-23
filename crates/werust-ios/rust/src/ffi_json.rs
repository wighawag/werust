//! JSON wire form of [`ChromeState`] for the C-ABI boundary.
//!
//! The Swift edge reads the whole chrome across the C-ABI as one JSON string (a
//! single NUL-terminated string return is the simplest robust FFI marshalling —
//! no per-field C calls, no shared struct layout). This module hand-encodes
//! exactly the fields the Swift edge paints (URL bar text, Back/Forward
//! enablement, loading vs settled, the load state, and any surfaced failure), so
//! the static lib stays dependency-light (no serde) and the wire shape is pinned
//! by the tests below. It is the byte-for-byte twin of the Android core's
//! `ffi_json` (the SAME chrome, a different OS edge), so the two mobile edges
//! decode an identical wire form.

use renderer::{LoadState, TrustPosture};
use werust_core::ChromeState;

/// Encode a [`ChromeState`] as a compact JSON object for the Swift edge.
///
/// The shape (stable, asserted by the tests):
/// `{"url":..,"loadState":..,"loading":bool,"canGoBack":bool,"canGoForward":bool,"trustPosture":..,"error":..}`.
/// `error` is `null` when nothing has failed. `trustPosture` carries the current
/// load's [`TrustPosture`] so the Swift chrome can paint the trust indicator from
/// the core's truth (the actual load path), matching desktop.
pub fn chrome_to_json(state: &ChromeState) -> String {
    let error = match &state.last_error {
        Some(reason) => format!("\"{}\"", escape(reason)),
        None => "null".to_string(),
    };
    format!(
        "{{\"url\":\"{url}\",\"loadState\":\"{load_state}\",\"loading\":{loading},\"canGoBack\":{back},\"canGoForward\":{forward},\"trustPosture\":\"{trust}\",\"error\":{error}}}",
        url = escape(&state.url_text),
        load_state = load_state_name(state.load_state),
        loading = state.is_loading(),
        back = state.can_go_back,
        forward = state.can_go_forward,
        trust = trust_posture_name(state.trust_posture),
    )
}

/// The stable, wire name of a [`TrustPosture`] the Swift edge paints its trust
/// indicator from. The SAME names the Android core's `ffi_json` uses, so the two
/// mobile edges decode an identical wire form.
fn trust_posture_name(posture: TrustPosture) -> &'static str {
    match posture {
        TrustPosture::UnverifiedOrigin => "unverified-origin",
        TrustPosture::ContentVerified => "content-verified",
        TrustPosture::NameViaTrustedRpc => "name-via-trusted-rpc",
        TrustPosture::MutableName => "mutable-name",
    }
}

/// The stable, lower-case name of a [`LoadState`] for the wire form.
fn load_state_name(state: LoadState) -> &'static str {
    match state {
        LoadState::Idle => "idle",
        LoadState::Started => "started",
        LoadState::Committed => "committed",
        LoadState::Finished => "finished",
        LoadState::Failed => "failed",
    }
}

/// Escape the characters a JSON string value must escape: backslash, double
/// quote, and the C0 control characters that appear in URLs/messages. Kept
/// minimal (the values are URLs and short human-readable reasons), and covered by
/// the tests so a surprising URL cannot produce invalid JSON.
fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_the_idle_default_chrome() {
        let json = chrome_to_json(&ChromeState::default());
        assert_eq!(
            json,
            "{\"url\":\"\",\"loadState\":\"idle\",\"loading\":false,\"canGoBack\":false,\"canGoForward\":false,\"trustPosture\":\"unverified-origin\",\"error\":null}"
        );
    }

    #[test]
    fn encodes_each_trust_posture_so_the_swift_chrome_paints_the_indicator() {
        // The chrome JSON carries the current load's trust posture so the Swift
        // edge can paint the trust indicator from the core's truth (the actual
        // load path), matching desktop — including the ENS `NameViaTrustedRpc` and
        // the `MutableName` states. The SAME stable wire names the Android core
        // uses, so both mobile edges decode an identical form.
        for (posture, name) in [
            (TrustPosture::UnverifiedOrigin, "unverified-origin"),
            (TrustPosture::ContentVerified, "content-verified"),
            (TrustPosture::NameViaTrustedRpc, "name-via-trusted-rpc"),
            (TrustPosture::MutableName, "mutable-name"),
        ] {
            let state = ChromeState {
                trust_posture: posture,
                ..ChromeState::default()
            };
            let json = chrome_to_json(&state);
            assert!(
                json.contains(&format!("\"trustPosture\":\"{name}\"")),
                "posture {posture:?} must serialize as {name}: {json}"
            );
        }
    }

    #[test]
    fn encodes_an_in_flight_load() {
        let state = ChromeState {
            url_text: "https://example.com/".into(),
            load_state: LoadState::Started,
            ..ChromeState::default()
        };
        let json = chrome_to_json(&state);
        assert!(json.contains("\"url\":\"https://example.com/\""), "{json}");
        assert!(json.contains("\"loadState\":\"started\""), "{json}");
        assert!(json.contains("\"loading\":true"), "{json}");
        assert!(json.contains("\"error\":null"), "{json}");
    }

    #[test]
    fn encodes_a_surfaced_failure() {
        let state = ChromeState {
            url_text: "https://bad.invalid/".into(),
            load_state: LoadState::Failed,
            last_error: Some("name not resolved".into()),
            ..ChromeState::default()
        };
        let json = chrome_to_json(&state);
        assert!(json.contains("\"loadState\":\"failed\""), "{json}");
        assert!(json.contains("\"error\":\"name not resolved\""), "{json}");
    }

    #[test]
    fn escapes_quotes_and_backslashes_so_the_json_stays_valid() {
        // A URL or reason with a quote/backslash must not break the JSON.
        let state = ChromeState {
            url_text: "https://x/\"a\\b".into(),
            last_error: Some("bad \"quote\"\nline".into()),
            ..ChromeState::default()
        };
        let json = chrome_to_json(&state);
        assert!(json.contains("\\\"a\\\\b"), "{json}");
        assert!(json.contains("\\\"quote\\\"\\nline"), "{json}");
    }
}
