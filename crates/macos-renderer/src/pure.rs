//! The macOS backend's HOST-INDEPENDENT half: the decisions the WKWebView
//! wiring makes, lifted out of the Objective-C layer so they are pure functions
//! the Ubuntu `verify` gate compiles and unit-tests with no Xcode, no SDK and no
//! Mac.
//!
//! This is the same discipline `crates/windows-origin-probe` uses to keep its
//! decision rule inside the gate while its WebView2 half is `#[cfg(windows)]`,
//! and the same one the WebKitGTK backend uses for
//! `os_color_scheme_from_portal`: anything that is a RULE rather than an SDK
//! call belongs here, where it can be asserted.

use renderer::OsColorScheme;

/// `NSURLErrorCancelled` — the `NSURLErrorDomain` code AppKit reports when a
/// load was deliberately cancelled (the user pressed Stop, or a new navigation
/// superseded an in-flight one).
pub const NSURL_ERROR_CANCELLED: isize = -999;

/// `WKErrorFrameLoadInterruptedByPolicyChange` — the `WKErrorDomain` code
/// WebKit reports when a navigation was interrupted by a policy decision (the
/// normal outcome of a redirect or an in-place new-window route), NOT a load
/// failure the user should see.
pub const WK_ERROR_FRAME_LOAD_INTERRUPTED: isize = 102;

/// Map the effective `NSAppearance` name to the shared cross-platform
/// [`OsColorScheme`] (`docs/adr/0009`: FOLLOW the OS, never force dark).
///
/// AppKit names the two system appearances `NSAppearanceNameAqua` and
/// `NSAppearanceNameDarkAqua`, plus the increased-contrast variants
/// `NSAppearanceNameAccessibilityHighContrastAqua` /
/// `…HighContrastDarkAqua`. Only a name that genuinely reads DARK maps to
/// [`OsColorScheme::Dark`]; a light name maps to [`OsColorScheme::Light`]; and an
/// unknown or unreadable name maps to [`OsColorScheme::NoPreference`], so a
/// future appearance can never silently flip the WebView to dark. werust never
/// guesses dark.
///
/// The pure twin of the desktop backend's `os_color_scheme_from_portal`, so both
/// desktop edges resolve the OS signal through the SAME
/// [`OsColorScheme`] vocabulary rather than each minting its own.
#[must_use]
pub fn os_color_scheme_from_appearance(name: &str) -> OsColorScheme {
    if name.contains("DarkAqua") {
        OsColorScheme::Dark
    } else if name.contains("Aqua") {
        OsColorScheme::Light
    } else {
        OsColorScheme::NoPreference
    }
}

/// Decide whether a `WKNavigationDelegate` error is a real, user-visible load
/// FAILURE, and with what reason.
///
/// WebKit reports two errors that are NOT failures of the page:
///
/// * `NSURLErrorCancelled` (-999) — the load was cancelled, which is exactly
///   what [`Renderer::stop`](renderer::Renderer::stop) and a superseding
///   navigation both produce. Reporting it would flash a spurious error banner
///   on every Stop and every fast re-navigation.
/// * `WKErrorFrameLoadInterruptedByPolicyChange` (102) — a policy decision
///   interrupted the frame load, the ordinary outcome of a redirect or of the
///   ADR-0010 new-window-in-place route.
///
/// Everything else is a genuine failure and carries its legible description into
/// [`LoadEvent::Failed`](renderer::LoadEvent::Failed). Returning [`None`] means
/// "do not move the lifecycle at all", which is what keeps a cancelled load in
/// its settled state instead of flipping it to
/// [`LoadState::Failed`](renderer::LoadState::Failed).
#[must_use]
pub fn navigation_failure(code: isize, description: &str) -> Option<String> {
    if code == NSURL_ERROR_CANCELLED || code == WK_ERROR_FRAME_LOAD_INTERRUPTED {
        return None;
    }
    let description = description.trim();
    if description.is_empty() {
        Some(format!("load failed (code {code})"))
    } else {
        Some(description.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_appearance_is_the_only_one_that_asks_for_dark() {
        // ADR-0009's rule, on the macOS edge: FOLLOW the OS. Only a genuinely
        // dark appearance name prefers dark; light stays light; an unknown or
        // future name supplies NO preference rather than guessing dark.
        assert_eq!(
            os_color_scheme_from_appearance("NSAppearanceNameDarkAqua"),
            OsColorScheme::Dark
        );
        assert_eq!(
            os_color_scheme_from_appearance("NSAppearanceNameAccessibilityHighContrastDarkAqua"),
            OsColorScheme::Dark
        );
        assert_eq!(
            os_color_scheme_from_appearance("NSAppearanceNameAqua"),
            OsColorScheme::Light
        );
        assert_eq!(
            os_color_scheme_from_appearance("NSAppearanceNameAccessibilityHighContrastAqua"),
            OsColorScheme::Light
        );
        for unknown in ["", "NSAppearanceNameVibrantSomethingNew", "who knows"] {
            assert_eq!(
                os_color_scheme_from_appearance(unknown),
                OsColorScheme::NoPreference,
                "{unknown:?} must not be guessed as dark"
            );
        }
        // And the shared rule agrees: only Dark asks the WebView to prefer dark.
        assert!(os_color_scheme_from_appearance("NSAppearanceNameDarkAqua").prefer_dark());
        assert!(!os_color_scheme_from_appearance("NSAppearanceNameAqua").prefer_dark());
    }

    #[test]
    fn a_cancelled_or_policy_interrupted_navigation_is_not_a_load_failure() {
        // Stop() and a superseding navigation both produce NSURLErrorCancelled.
        // Reporting them would flash an error banner on every Stop.
        assert_eq!(navigation_failure(NSURL_ERROR_CANCELLED, "cancelled"), None);
        assert_eq!(
            navigation_failure(WK_ERROR_FRAME_LOAD_INTERRUPTED, "frame load interrupted"),
            None
        );
    }

    #[test]
    fn a_real_error_carries_its_legible_reason_into_the_failed_event() {
        assert_eq!(
            navigation_failure(
                -1003,
                "A server with the specified hostname could not be found."
            ),
            Some("A server with the specified hostname could not be found.".to_string())
        );
        // An error with no description still names something actionable rather
        // than an empty banner.
        assert_eq!(
            navigation_failure(-1200, "   "),
            Some("load failed (code -1200)".to_string())
        );
    }
}
