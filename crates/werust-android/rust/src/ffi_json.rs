//! JSON wire form of [`ChromeState`] for the JNI boundary.
//!
//! The Kotlin edge reads the whole chrome across JNI as one JSON string (a single
//! string return is the simplest robust JNI marshalling — no per-field JNI calls,
//! no shared struct layout). This module hand-encodes exactly the fields the
//! Kotlin edge paints (URL bar text, Back/Forward enablement, loading vs settled,
//! the load state, and any surfaced failure), so the cdylib stays dependency-light
//! (no serde) and the wire shape is pinned by the tests below.

use renderer::{LoadState, TrustPosture};
use werust_core::ChromeState;

/// Encode a [`ChromeState`] as a compact JSON object for the Kotlin edge.
///
/// The shape (stable, asserted by the tests):
/// `{"url":..,"loadState":..,"loading":bool,"loadStep":..,"canGoBack":bool,"canGoForward":bool,"trustPosture":..,"error":..,"failureKind":..,"retryable":bool,"invalidEntry":..}`.
/// `error` is `null` when nothing has failed. `trustPosture` carries the current
/// load's [`TrustPosture`] so the Kotlin chrome can paint the trust indicator
/// from the core's truth (the actual load path), matching desktop. `loadStep`
/// carries the live pipeline step (name/record/content/render) so the mobile
/// chrome shows real loading progress, and `failureKind`/`retryable` carry the
/// transient-vs-hard distinction so a timeout is shown as retryable (task
/// `clearer-loading-and-error-indicator`). `failureKind` is `null` when nothing
/// has failed. `invalidEntry` carries the typed text of an INVALID URL-bar entry
/// (a scheme-less garbage entry that did not navigate) so the mobile chrome
/// paints the "invalid URL" badge + red-underlined URL bar from the SAME
/// orthogonal fact desktop uses (`null` when the entry is valid; task
/// `scheme-less-entry-https-fallback-and-keep-bar-on-error`).
pub fn chrome_to_json(state: &ChromeState) -> String {
    let error = match &state.last_error {
        Some(reason) => format!("\"{}\"", escape(reason)),
        None => "null".to_string(),
    };
    let failure_kind = match state.failure_kind() {
        Some(kind) => format!("\"{}\"", kind.wire_name()),
        None => "null".to_string(),
    };
    let invalid_entry = match &state.invalid_entry {
        Some(entry) => format!("\"{}\"", escape(entry)),
        None => "null".to_string(),
    };
    format!(
        "{{\"url\":\"{url}\",\"loadState\":\"{load_state}\",\"loading\":{loading},\"loadStep\":\"{load_step}\",\"canGoBack\":{back},\"canGoForward\":{forward},\"trustPosture\":\"{trust}\",\"error\":{error},\"failureKind\":{failure_kind},\"retryable\":{retryable},\"invalidEntry\":{invalid_entry}}}",
        url = escape(&state.url_text),
        load_state = load_state_name(state.load_state),
        loading = state.is_loading(),
        load_step = state.load_step().wire_name(),
        back = state.can_go_back,
        forward = state.can_go_forward,
        trust = trust_posture_name(state.trust_posture),
        retryable = state.failure_is_retryable(),
    )
}

/// The stable, wire name of a [`TrustPosture`] the Kotlin edge paints its trust
/// indicator from. Kept lower-kebab so the wire form is stable across platforms
/// (the iOS `ffi_json` uses the SAME names).
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
            "{\"url\":\"\",\"loadState\":\"idle\",\"loading\":false,\"loadStep\":\"idle\",\"canGoBack\":false,\"canGoForward\":false,\"trustPosture\":\"unverified-origin\",\"error\":null,\"failureKind\":null,\"retryable\":false,\"invalidEntry\":null}"
        );
    }

    #[test]
    fn encodes_the_live_pipeline_step_so_the_mobile_chrome_shows_progress() {
        // The chrome JSON carries the live pipeline step so the mobile edge shows
        // real loading progress (name/record/content/render), matching desktop.
        use renderer::LoadState;
        use werust_core::LoadStep;
        for (step, wire) in [
            (LoadStep::ResolvingName, "resolving-name"),
            (LoadStep::FetchingRecord, "fetching-record"),
            (LoadStep::FetchingContent, "fetching-content"),
            (LoadStep::Rendering, "rendering"),
        ] {
            let state = ChromeState {
                load_state: LoadState::Started,
                load_step: step,
                ..ChromeState::default()
            };
            let json = chrome_to_json(&state);
            assert!(
                json.contains(&format!("\"loadStep\":\"{wire}\"")),
                "step {step:?} must serialize as {wire}: {json}"
            );
        }
    }

    #[test]
    fn encodes_the_transient_vs_hard_failure_distinction_so_a_timeout_is_retryable() {
        // The chrome JSON carries the transient-vs-hard distinction so the mobile
        // edge can show a retry affordance for a timeout and keep the protocol-
        // named reason for a hard fail (task `clearer-loading-and-error-indicator`).
        use renderer::LoadState;
        let transient = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("transport error: timeout: global".into()),
            ..ChromeState::default()
        };
        let json = chrome_to_json(&transient);
        assert!(json.contains("\"failureKind\":\"transient\""), "{json}");
        assert!(json.contains("\"retryable\":true"), "{json}");

        let hard = ChromeState {
            load_state: LoadState::Failed,
            last_error: Some("points to Swarm, not supported".into()),
            ..ChromeState::default()
        };
        let json = chrome_to_json(&hard);
        assert!(json.contains("\"failureKind\":\"hard\""), "{json}");
        assert!(json.contains("\"retryable\":false"), "{json}");
    }

    #[test]
    fn encodes_an_invalid_entry_so_the_mobile_chrome_paints_the_badge() {
        // The chrome JSON carries the typed text of an INVALID URL-bar entry (a
        // scheme-less garbage entry that did not navigate) so the Kotlin edge
        // paints the "invalid URL" badge + red-underlined URL bar from the SAME
        // orthogonal fact desktop uses (task
        // `scheme-less-entry-https-fallback-and-keep-bar-on-error`). Distinct from
        // `error` (a load failure).
        let valid = chrome_to_json(&ChromeState::default());
        assert!(valid.contains("\"invalidEntry\":null"), "{valid}");
        let invalid = ChromeState {
            url_text: "not a url".into(),
            invalid_entry: Some("not a url".into()),
            ..ChromeState::default()
        };
        let json = chrome_to_json(&invalid);
        assert!(json.contains("\"invalidEntry\":\"not a url\""), "{json}");
        // An invalid entry is NOT a load error: `error` stays null.
        assert!(json.contains("\"error\":null"), "{json}");
    }

    #[test]
    fn encodes_each_trust_posture_so_the_kotlin_chrome_paints_the_indicator() {
        // The chrome JSON carries the current load's trust posture so the Kotlin
        // edge can paint the trust indicator from the core's truth (the actual
        // load path), matching desktop — including the ENS `NameViaTrustedRpc` and
        // the `MutableName` states. A distinct, stable wire name per posture.
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
