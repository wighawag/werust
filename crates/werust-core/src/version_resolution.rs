// How werust's ONE version string is RESOLVED at build time.
//
// This file is deliberately dependency-free and `include!`d by
// `crates/werust-core/build.rs`: the build script resolves the version once,
// emits it as `cargo:rustc-env=WERUST_VERSION`, and `crate::version` reads THAT.
// The same file is compiled into the crate under `#[cfg(test)]` (see the
// `version_resolution` module declaration in `lib.rs`) so the precedence rules
// below are unit-tested inside the pure-Rust `verify` gate (`cargo test`) rather
// than only exercised implicitly by whatever the build machine happened to have.
// It uses plain `//` comments rather than `//!` module docs precisely because it
// is `include!`d into a module body, where inner doc comments are not permitted.
//
// WHY a build-time resolution at all: the workspace Cargo version is only ever
// bumped by hand, and NOTHING in the release path injected it into the compiled
// binary (GoReleaser derives the ARCHIVE NAME from the tag, never the code), so
// every version surface — the desktop startup banner and all three browser menus
// — read `0.0.0` at tag `v0.2.6`. A version that is always `0.0.0` is worse than
// no version: it is a confident lie in a user-facing menu. So the version is
// resolved HERE, once, from the most authoritative source available at build
// time.

/// Resolve werust's version string from the three sources, in precedence order.
///
/// 1. `injected` — the `WERUST_VERSION` environment variable, when set and
///    non-empty. This is what CI exports from the release tag, so a tagged build
///    reports EXACTLY the released version.
/// 2. `git_describe` — the output of `git describe --tags --always`, when git is
///    present and the source is a checkout. An informative DEV build
///    (`0.2.6-3-gabc1234`, or a bare short hash when no tag is reachable).
/// 3. `cargo_pkg_version` — the workspace Cargo version, the last resort (git
///    absent, or an unpacked source tarball). Never fails the build.
///
/// Whichever source wins, a leading `v` immediately followed by a digit is
/// stripped (`v0.2.6` -> `0.2.6`), since that `v` belongs to the TAG name, not
/// to the version. The digit guard keeps the rule from mangling a legitimately
/// `v`-initial string an operator might inject. Surrounding whitespace (notably
/// `git describe`'s trailing newline) is trimmed, so the string is always safe to
/// splice straight into a menu label.
fn resolve_version(
    injected: Option<&str>,
    git_describe: Option<&str>,
    cargo_pkg_version: &str,
) -> String {
    for candidate in [injected, git_describe] {
        let Some(candidate) = candidate.map(str::trim) else {
            continue;
        };
        if !candidate.is_empty() {
            return strip_tag_prefix(candidate).to_string();
        }
    }
    strip_tag_prefix(cargo_pkg_version.trim()).to_string()
}

/// Strip a release tag's leading `v` when it prefixes a version number
/// (`v0.2.6` -> `0.2.6`), leaving anything else untouched.
fn strip_tag_prefix(version: &str) -> &str {
    match version.strip_prefix('v') {
        Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => rest,
        _ => version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_injected_version_wins_over_git_and_cargo() {
        // The release path: CI exports WERUST_VERSION from the tag, so a tagged
        // build reports the released version even though the checkout's `git
        // describe` and the Cargo metadata would say something else.
        assert_eq!(
            resolve_version(Some("0.2.6"), Some("0.2.6-3-gabc1234"), "0.0.0"),
            "0.2.6"
        );
    }

    #[test]
    fn a_tag_shaped_injection_loses_its_leading_v() {
        // The injection is derived from a tag NAME (`v0.2.6`); the `v` is the
        // tag's, not the version's. Normalised whichever source it arrives from,
        // so the menus never read "werust v0.2.6" on one platform and
        // "werust 0.2.6" on another.
        assert_eq!(resolve_version(Some("v0.2.6"), None, "0.0.0"), "0.2.6");
        assert_eq!(
            resolve_version(None, Some("v0.2.6-3-gabc1234"), "0.0.0"),
            "0.2.6-3-gabc1234"
        );
    }

    #[test]
    fn a_v_initial_string_that_is_not_a_version_is_left_alone() {
        // The digit guard: only a `v` that PREFIXES a version number is a tag
        // prefix. An operator injecting a named build must get it back verbatim.
        assert_eq!(
            resolve_version(Some("vendor-build"), None, "0.0.0"),
            "vendor-build"
        );
    }

    #[test]
    fn an_unset_or_empty_injection_falls_through_to_git_describe() {
        // An unset variable and an EMPTY one must behave identically: CI that
        // exports `WERUST_VERSION=` on a non-tag path must not blank the version.
        for injected in [None, Some(""), Some("   ")] {
            assert_eq!(
                resolve_version(injected, Some("0.2.6-3-gabc1234"), "0.0.0"),
                "0.2.6-3-gabc1234",
                "injected {injected:?} must fall through to git describe"
            );
        }
    }

    #[test]
    fn git_describes_trailing_newline_is_trimmed() {
        // `git describe` output ends in a newline; splicing that into a menu
        // label would break the menu, so it is trimmed at the source.
        assert_eq!(
            resolve_version(None, Some("0.2.6-3-gabc1234\n"), "0.0.0"),
            "0.2.6-3-gabc1234"
        );
    }

    #[test]
    fn without_git_the_cargo_version_is_the_last_resort_and_never_fails() {
        // An unpacked source tarball, or a machine with no git: the build must
        // still succeed and report SOMETHING honest (the Cargo metadata), never
        // fail and never yield an empty string.
        assert_eq!(resolve_version(None, None, "0.2.6"), "0.2.6");
        assert_eq!(resolve_version(Some(""), Some(""), "0.2.6"), "0.2.6");
        assert!(!resolve_version(None, None, "0.2.6").is_empty());
    }
}
