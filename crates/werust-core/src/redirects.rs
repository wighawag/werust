//! The IPFS `_redirects` web-pathing convention (IPIP-0002): parse a site's
//! root `_redirects` file and decide what a NOT-FOUND path resolves to, per the
//! SITE's own rules.
//!
//! Spec: <https://specs.ipfs.tech/http-gateways/web-redirects-file/>.
//!
//! # What this module is (and is not)
//!
//! This is the PURE half of the fallback: grammar + matching, no I/O and no
//! retrieval. It turns the bytes of a `_redirects` file into [`RedirectRule`]s
//! ([`parse_redirects`]) and turns a requested path into the [`FallbackAction`]
//! the first matching rule names ([`match_fallback`]). The verified retrieval of
//! both the `_redirects` file itself AND the rule's target is done by the caller
//! ([`crate::ipfs::resolve_ipfs_request`]) through the SAME hash-verified
//! `ContentRetriever` every other resource goes through, so the fallback is NOT
//! a verification bypass: it only ever names another path under the SAME root
//! CID, which is then fetched and verified exactly like a normal resource.
//!
//! # The unique-origin security rule (why a target may not leave the root CID)
//!
//! IPIP-0002 §3.1/§4 only permit `_redirects` evaluation where Same-Origin
//! isolation per root CID holds (a subdomain/DNSLink gateway, or a browser with
//! a native `ipfs://` handler — werust), precisely because a rewrite/redirect is
//! a PER-SITE capability that must not be able to speak for another content
//! root. werust serves `ipfs://<cid>` as its own content root, so a `_redirects`
//! under `<rootcid>` governs ONLY paths under `<rootcid>`: a target that leaves
//! the root (an absolute `https://`/`ipfs://` URL, a protocol-relative
//! `//host/…` authority, or a `..` escape) is REJECTED as
//! [`RedirectsError::OffRootTarget`] rather than followed or silently skipped,
//! so a site's own rules can never make it impersonate another site. Silently
//! skipping such a rule would be worse than refusing: the request would fall
//! through to a LATER rule and serve a different page than the site's author
//! wrote, so the honest answer is a legible fail-closed refusal.
//!
//! # The subset that landed (recorded per the task's "record what landed")
//!
//! * `200` (rewrite / SPA + PWA), `404` (custom not-found page), `410`, `451` —
//!   SERVED: the target's verified content is returned for the requested URL
//!   with that status, nothing navigates, the URL bar is untouched.
//! * `301`/`302`/`303`/`307`/`308` — PARSED (so an unrelated redirect line never
//!   breaks a file whose catch-all is what matters) but NOT applied: a redirect
//!   is a NAVIGATION (it changes the URL bar and the trust identity shown with
//!   it), which the scheme-resolution seam cannot express today. A rule of this
//!   kind that actually MATCHES fails the load with a legible
//!   [`RedirectsError::RedirectNotSupported`] rather than silently falling
//!   through to a later rule (which would serve a page the author did not name
//!   for that path). Landing navigation is a follow-on task.
//! * Placeholders (`:name`) and the trailing catch-all splat (`*` / `:splat`)
//!   are supported in both `from` matching and `to` injection.
//! * A `to`'s query string (`/target?a=b`) is DROPPED when the target is served:
//!   a query is a request modifier, not part of the content-addressed DAG path
//!   (the same rule [`crate::ipfs::parse_ipfs_uri`] applies to a request URI).
//!   IPIP-0002 §3.5's query-parameter merging only affects the `Location` of a
//!   3xx redirect, which is not supported yet.
//!
//! The full supported/unsupported table, the alternatives weighed for each
//! choice, and the security rationale in long form live in
//! `docs/spikes/ipfs-web-redirects-and-404-fallback-support/DECISIONS.md`.

use std::collections::BTreeMap;
use std::fmt;

/// The path of a site's redirects file, under the ROOT CID (IPIP-0002 §1).
pub const REDIRECTS_PATH: &str = "/_redirects";

/// The path of the DEFAULT custom error page a site may ship with no
/// `_redirects` at all: a root `404.html`, served (with a not-found status) for
/// a path that is not in the DAG, exactly as an HTTP gateway does.
pub const DEFAULT_404_PATH: &str = "/404.html";

/// The maximum size of a `_redirects` file (IPIP-0002 §2.4.5: 64 KiB).
///
/// A hard ceiling, so a hostile site cannot turn the not-found path into a
/// denial-of-service vector. A larger file is refused
/// ([`RedirectsError::TooLarge`]), never truncated-and-parsed.
pub const MAX_REDIRECTS_BYTES: usize = 64 * 1024;

/// The HTTP status codes IPIP-0002 §2.3 allows in a rule.
const ALLOWED_STATUSES: [u16; 9] = [200, 301, 302, 303, 307, 308, 404, 410, 451];

/// The default status when a rule omits it (IPIP-0002 §2.3: 301).
const DEFAULT_STATUS: u16 = 301;

/// One parsed `from to [status]` rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectRule {
    /// The path pattern to match (may carry `:placeholder` segments and a single
    /// trailing `*` catch-all).
    pub from: String,
    /// The path to serve/redirect to (may inject `:splat` / `:placeholder`).
    pub to: String,
    /// The HTTP status the rule asks for.
    pub status: u16,
}

/// What a matched rule asks werust to do for a not-found path.
///
/// Only [`Serve`](FallbackAction::Serve) exists today: the target's verified
/// content is returned FOR THE REQUESTED URL with the rule's status (a `200`
/// rewrite, or a `404`/`410`/`451` error page), so nothing navigates and the URL
/// bar is untouched. A matched 3xx rule is a navigation and is refused (see the
/// module docs); it is not represented here so a caller cannot accidentally
/// treat it as a same-URL serve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FallbackAction {
    /// Serve the verified content of `path` (a path under the SAME root CID) as
    /// the response to the requested URL, with `status`.
    Serve {
        /// The in-site path of the target, always root-relative and inside the
        /// root CID (validated by [`match_fallback`]).
        path: String,
        /// The status to answer the requested URL with.
        status: u16,
    },
}

/// A fail-closed problem with a site's `_redirects` file or with the rule that
/// matched, each cause DISTINCT so the failure the user sees is legible.
///
/// IPIP-0002 §3.4 requires an unreadable/unparseable redirects file to surface
/// as an error rather than be ignored; werust surfaces every variant here as a
/// failed load (the caller maps it onto the seam's error), so a broken or
/// hostile `_redirects` can never silently serve the wrong page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedirectsError {
    /// The file exceeds [`MAX_REDIRECTS_BYTES`] (IPIP-0002 §2.4.5).
    TooLarge {
        /// The observed size in bytes.
        size: usize,
    },
    /// The file is not valid UTF-8 text.
    NotText,
    /// A line did not parse as `from to [status]`.
    InvalidLine {
        /// The 1-based line number.
        line: usize,
        /// Why the line is invalid.
        reason: String,
    },
    /// A rule's `to` leaves the root CID (an absolute URL, a protocol-relative
    /// authority, or a `..` escape). Refused: see the module docs' unique-origin
    /// rule.
    OffRootTarget {
        /// The offending target as written.
        to: String,
    },
    /// The matching rule asks for a 3xx REDIRECT (a navigation), which the
    /// scheme-resolution path cannot express yet. Refused rather than silently
    /// serving some other rule's page.
    RedirectNotSupported {
        /// The status the rule asked for.
        status: u16,
        /// The target the rule named.
        to: String,
    },
}

impl fmt::Display for RedirectsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RedirectsError::TooLarge { size } => write!(
                f,
                "_redirects is {size} bytes, over the {MAX_REDIRECTS_BYTES} byte limit"
            ),
            RedirectsError::NotText => write!(f, "_redirects is not valid utf-8 text"),
            RedirectsError::InvalidLine { line, reason } => {
                write!(f, "_redirects line {line}: {reason}")
            }
            RedirectsError::OffRootTarget { to } => write!(
                f,
                "_redirects target `{to}` leaves the site's root cid; a site's rules may only name content under its own root"
            ),
            RedirectsError::RedirectNotSupported { status, to } => write!(
                f,
                "_redirects rule asks for a {status} redirect to `{to}`, which werust does not follow yet"
            ),
        }
    }
}

impl std::error::Error for RedirectsError {}

/// Parse the bytes of a `_redirects` file into its rules, in file order.
///
/// The grammar is IPIP-0002 §2: one `from to [status]` per line, lines separated
/// by `\n` or `\r\n`, blank lines and surrounding whitespace ignored, an omitted
/// status defaulting to 301. Lines starting with `#` are treated as comments
/// (the spec's own query-parameter test vector uses them). Anything else — a
/// line with the wrong field count, a `from`/`to` that is not root-relative, a
/// status outside the allowed set, or a placeholder name repeated in `from`
/// (§2.4: implementations MUST error) — is a distinct fail-closed
/// [`RedirectsError`], never a silently-dropped rule.
pub fn parse_redirects(bytes: &[u8]) -> Result<Vec<RedirectRule>, RedirectsError> {
    if bytes.len() > MAX_REDIRECTS_BYTES {
        return Err(RedirectsError::TooLarge { size: bytes.len() });
    }
    let text = std::str::from_utf8(bytes).map_err(|_| RedirectsError::NotText)?;

    let mut rules = Vec::new();
    for (idx, raw) in text.split('\n').enumerate() {
        let line_no = idx + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (from, to, status) = match fields.as_slice() {
            [from, to] => (*from, *to, DEFAULT_STATUS),
            [from, to, status] => {
                let parsed = status
                    .parse::<u16>()
                    .map_err(|_| RedirectsError::InvalidLine {
                        line: line_no,
                        reason: format!("`{status}` is not an http status code"),
                    })?;
                if !ALLOWED_STATUSES.contains(&parsed) {
                    return Err(RedirectsError::InvalidLine {
                        line: line_no,
                        reason: format!("status {parsed} is not one of {ALLOWED_STATUSES:?}"),
                    });
                }
                (*from, *to, parsed)
            }
            _ => {
                return Err(RedirectsError::InvalidLine {
                    line: line_no,
                    reason: format!("expected `from to [status]`, got {} fields", fields.len()),
                })
            }
        };
        if !from.starts_with('/') {
            return Err(RedirectsError::InvalidLine {
                line: line_no,
                reason: format!("`from` must be a root-relative path, got `{from}`"),
            });
        }
        check_unique_placeholders(from, line_no)?;
        rules.push(RedirectRule {
            from: from.to_string(),
            to: to.to_string(),
            status,
        });
    }
    Ok(rules)
}

/// IPIP-0002 §2.4: the same placeholder name MUST NOT appear twice in `from`.
fn check_unique_placeholders(from: &str, line: usize) -> Result<(), RedirectsError> {
    let mut seen: Vec<&str> = Vec::new();
    for segment in from.split('/') {
        if let Some(name) = segment.strip_prefix(':') {
            if seen.contains(&name) {
                return Err(RedirectsError::InvalidLine {
                    line,
                    reason: format!("placeholder `:{name}` is used more than once in `from`"),
                });
            }
            seen.push(name);
        }
    }
    Ok(())
}

/// Decide what a NOT-FOUND `path` resolves to under `rules`: the action named by
/// the FIRST matching rule, or [`None`] when no rule matches.
///
/// IPIP-0002 §3.2/§3.3: rules are evaluated top to bottom and the first match
/// wins, and this whole evaluation happens ONLY for a path that is absent from
/// the DAG (the caller enforces that: an existing resource is served as is and
/// never reaches here, so a catch-all cannot shadow a real page).
///
/// The matched rule's target is expanded (placeholders/`:splat` injected), its
/// query string dropped, and it is CHECKED to stay within the root CID; a target
/// that leaves the root, or a rule asking for an unsupported 3xx navigation, is
/// a distinct fail-closed [`RedirectsError`] rather than a fall-through.
pub fn match_fallback(
    rules: &[RedirectRule],
    path: &str,
) -> Option<Result<FallbackAction, RedirectsError>> {
    let request_segments: Vec<&str> = split_segments(path);
    for rule in rules {
        let Some(captures) = match_from(&rule.from, &request_segments) else {
            continue;
        };
        return Some(resolve_target(rule, &captures));
    }
    None
}

/// Expand and validate a matched rule's target into the action to take.
fn resolve_target(
    rule: &RedirectRule,
    captures: &BTreeMap<String, String>,
) -> Result<FallbackAction, RedirectsError> {
    // The target is expanded FIRST so an off-root escape hidden in a captured
    // segment (e.g. a `:splat` that starts with `//` or contains `..`) is caught
    // by the same root check as a literal one.
    let expanded = inject_placeholders(&rule.to, captures);
    let target = within_root_path(&expanded).ok_or_else(|| RedirectsError::OffRootTarget {
        to: expanded.clone(),
    })?;
    if !matches!(rule.status, 200 | 404 | 410 | 451) {
        // A 3xx: a NAVIGATION, not a same-url serve. Refused with its reason (see
        // the module docs) rather than falling through to another rule.
        return Err(RedirectsError::RedirectNotSupported {
            status: rule.status,
            to: expanded,
        });
    }
    Ok(FallbackAction::Serve {
        path: target,
        status: rule.status,
    })
}

/// The non-empty segments of a path (`/a/b/` -> `["a", "b"]`).
fn split_segments(path: &str) -> Vec<&str> {
    path.split('/').filter(|s| !s.is_empty()).collect()
}

/// Match a rule's `from` against the requested path's segments, returning the
/// captured placeholders (including `splat`) on a match.
///
/// A literal segment must be equal; a `:name` segment matches exactly one
/// segment and captures it; a single TRAILING `*` slurps the remainder into
/// `splat` (IPIP-0002 §2.4.1, a greedy match, only as the last segment).
fn match_from(from: &str, request: &[&str]) -> Option<BTreeMap<String, String>> {
    let pattern = split_segments(from);
    let mut captures = BTreeMap::new();

    let splat = pattern.last() == Some(&"*");
    let fixed = if splat {
        &pattern[..pattern.len() - 1]
    } else {
        &pattern[..]
    };

    if splat {
        if request.len() < fixed.len() {
            return None;
        }
    } else if request.len() != fixed.len() {
        return None;
    }

    for (pat, seg) in fixed.iter().zip(request.iter()) {
        match pat.strip_prefix(':') {
            Some(name) => {
                captures.insert(name.to_string(), (*seg).to_string());
            }
            None if pat == seg => {}
            None => return None,
        }
    }

    if splat {
        captures.insert("splat".to_string(), request[fixed.len()..].join("/"));
    }
    Some(captures)
}

/// Inject `:name` placeholders (including `:splat`) into a rule's `to`.
///
/// A placeholder name may be reused (IPIP-0002 §2.4: implementations MUST allow
/// that in `to`). A `:name` with no capture is left VERBATIM, so a target whose
/// literal filename contains a colon is not mangled into something else.
fn inject_placeholders(to: &str, captures: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(to.len());
    let mut rest = to;
    while let Some(idx) = rest.find(':') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + 1..];
        let name_len = after
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(after.len());
        let name = &after[..name_len];
        match captures.get(name) {
            Some(value) if !name.is_empty() => out.push_str(value),
            _ => {
                out.push(':');
                out.push_str(name);
            }
        }
        rest = &after[name_len..];
    }
    out.push_str(rest);
    out
}

/// Reduce an expanded target to the in-site path it names, or [`None`] if it
/// leaves the site's root CID.
///
/// Refused (the unique-origin rule, module docs): anything carrying a scheme
/// (`https://…`, `ipfs://…`), a protocol-relative authority (`//host/…`), a
/// non-root-relative path, or a `..` that climbs above the root. On success the
/// path is normalized (`.`/`..` resolved, query string dropped) so the caller
/// can hand it straight to the verifying retriever.
fn within_root_path(to: &str) -> Option<String> {
    if to.contains("://") || to.starts_with("//") || !to.starts_with('/') {
        return None;
    }
    // A query string is a request modifier, not part of the content-addressed
    // DAG path (same rule the `ipfs://` uri parse applies); drop it, and any
    // fragment with it.
    let path = to.split(['?', '#']).next().unwrap_or(to);
    let mut resolved: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                // A `..` that would climb above the root is an off-root escape.
                resolved.pop()?;
            }
            other => resolved.push(other),
        }
    }
    Some(format!("/{}", resolved.join("/")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The IPIP-0002 spec appendix's own example file (§5.1), so the grammar is
    /// pinned against the spec's fixture rather than only our own shapes.
    const SPEC_EXAMPLE: &[u8] = b"/redirect-one /one.html\n\
        /301-redirect-one /one.html 301\n\
        /302-redirect-two /two.html 302\n\
        /200-index /index.html 200\n\
        /posts/:year/:month/:day/:title /articles/:year/:month/:day/:title 301\n\
        /splat/* /redirected-splat/:splat 301\n\
        /not-found/* /404.html 404\n\
        /gone/* /410.html 410\n\
        /unavail/* /451.html 451\n\
        /* /index.html 200\n";

    fn rules(text: &str) -> Vec<RedirectRule> {
        parse_redirects(text.as_bytes()).expect("valid _redirects")
    }

    #[test]
    fn parses_the_spec_example_file_in_order_with_the_default_status() {
        let parsed = parse_redirects(SPEC_EXAMPLE).expect("the spec's example file parses");
        assert_eq!(parsed.len(), 10, "every rule line is kept, in file order");
        assert_eq!(
            parsed[0],
            RedirectRule {
                from: "/redirect-one".into(),
                to: "/one.html".into(),
                status: 301,
            },
            "an omitted status defaults to 301 (IPIP-0002 §2.3)"
        );
        assert_eq!(parsed[9].from, "/*");
        assert_eq!(parsed[9].status, 200);
    }

    #[test]
    fn ignores_blank_lines_surrounding_whitespace_and_comments() {
        let parsed = rules("\n  \t\n  /a  /b  200  \n# a comment\n\r\n/c /d 404\r\n");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].from, "/a");
        assert_eq!(parsed[0].to, "/b");
        assert_eq!(parsed[1].status, 404);
    }

    #[test]
    fn refuses_a_file_over_the_64_kib_limit() {
        // IPIP-0002 §2.4.5: the size cap is a denial-of-service guard, so an
        // oversized file is refused outright, never truncated-and-parsed.
        let big = vec![b'\n'; MAX_REDIRECTS_BYTES + 1];
        assert_eq!(
            parse_redirects(&big),
            Err(RedirectsError::TooLarge {
                size: MAX_REDIRECTS_BYTES + 1
            })
        );
    }

    #[test]
    fn refuses_a_malformed_line_rather_than_dropping_it() {
        // A silently-dropped rule would change which rule matches first, so every
        // malformed line is a distinct, legible failure (IPIP-0002 §3.4).
        assert!(matches!(
            parse_redirects(b"/only-one-field\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
        assert!(matches!(
            parse_redirects(b"/a /b 200 extra\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
        assert!(matches!(
            parse_redirects(b"/a /b 999\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
        assert!(matches!(
            parse_redirects(b"/a /b nope\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
        assert!(matches!(
            parse_redirects(b"relative /b 200\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
    }

    #[test]
    fn refuses_a_repeated_placeholder_name_in_from() {
        // IPIP-0002 §2.4: implementations MUST error when the same placeholder
        // name is used more than once in `from`.
        assert!(matches!(
            parse_redirects(b"/a/:x/:x /b/:x 301\n"),
            Err(RedirectsError::InvalidLine { line: 1, .. })
        ));
    }

    #[test]
    fn the_jolly_roger_catch_all_serves_the_custom_404_page() {
        // The field case: jolly-roger.eth's whole `_redirects` is one catch-all
        // naming a directory-index 404 page.
        let parsed = rules("/* /404.html/index.html 404\n");
        assert_eq!(
            match_fallback(&parsed, "/unknown"),
            Some(Ok(FallbackAction::Serve {
                path: "/404.html/index.html".into(),
                status: 404,
            }))
        );
    }

    #[test]
    fn a_200_rule_is_a_same_url_rewrite() {
        let parsed = rules("/app/* /app/index.html 200\n");
        assert_eq!(
            match_fallback(&parsed, "/app/deep/route"),
            Some(Ok(FallbackAction::Serve {
                path: "/app/index.html".into(),
                status: 200,
            }))
        );
    }

    #[test]
    fn the_first_matching_rule_wins_and_a_non_matching_rule_is_skipped() {
        // IPIP-0002 §3.2: top-to-bottom, first match wins.
        let parsed = rules("/app/* /app/index.html 200\n/* /404.html 404\n");
        assert_eq!(
            match_fallback(&parsed, "/app/x"),
            Some(Ok(FallbackAction::Serve {
                path: "/app/index.html".into(),
                status: 200,
            }))
        );
        assert_eq!(
            match_fallback(&parsed, "/elsewhere"),
            Some(Ok(FallbackAction::Serve {
                path: "/404.html".into(),
                status: 404,
            }))
        );
    }

    #[test]
    fn no_rule_matching_is_not_an_error() {
        // A `_redirects` that simply says nothing about this path leaves the
        // caller free to fall back to a default `404.html` (or the honest
        // not-found), rather than inventing a match.
        let parsed = rules("/only/this /that.html 200\n");
        assert_eq!(match_fallback(&parsed, "/something-else"), None);
    }

    #[test]
    fn placeholders_are_captured_and_injected_including_repeats() {
        let parsed = rules("/posts/:year/:month/:title /articles/:year/:year-:month/:title 200\n");
        assert_eq!(
            match_fallback(&parsed, "/posts/2026/07/hello"),
            Some(Ok(FallbackAction::Serve {
                path: "/articles/2026/2026-07/hello".into(),
                status: 200,
            })),
            "a placeholder may be injected more than once (IPIP-0002 §2.4)"
        );
        // A placeholder segment matches exactly ONE segment, so a deeper path
        // does not match this rule.
        assert_eq!(match_fallback(&parsed, "/posts/2026/07/a/b"), None);
    }

    #[test]
    fn the_trailing_splat_slurps_the_remainder_greedily() {
        let parsed = rules("/splat/* /redirected-splat/:splat 200\n");
        assert_eq!(
            match_fallback(&parsed, "/splat/a/b/c"),
            Some(Ok(FallbackAction::Serve {
                path: "/redirected-splat/a/b/c".into(),
                status: 200,
            }))
        );
        // The splat may also slurp NOTHING (`/splat` itself matches `/splat/*`).
        assert_eq!(
            match_fallback(&parsed, "/splat"),
            Some(Ok(FallbackAction::Serve {
                path: "/redirected-splat".into(),
                status: 200,
            }))
        );
    }

    #[test]
    fn an_off_root_target_is_refused_not_skipped() {
        // The unique-origin rule: a site's `_redirects` may only name content
        // under its OWN root cid. Refusing (rather than skipping to the next
        // rule) keeps the served page the one the author named.
        for to in [
            "https://evil.example/404.html",
            "ipfs://bafyotherroot/404.html",
            "//evil.example/404.html",
            "relative.html",
            "/../bafyotherroot/404.html",
        ] {
            let parsed = rules(&format!("/* {to} 404\n"));
            let got = match_fallback(&parsed, "/unknown");
            assert!(
                matches!(got, Some(Err(RedirectsError::OffRootTarget { .. }))),
                "`{to}` must be refused as off-root, got: {got:?}"
            );
        }
    }

    #[test]
    fn an_off_root_escape_hidden_in_a_capture_is_refused_too() {
        // The root check runs AFTER placeholder injection, so a `..` smuggled in
        // through a captured segment cannot climb out of the root either.
        let parsed = rules("/x/* /assets/:splat 200\n");
        let got = match_fallback(&parsed, "/x/../../etc/passwd");
        assert!(
            matches!(got, Some(Err(RedirectsError::OffRootTarget { .. })))
                || matches!(
                    got,
                    Some(Ok(FallbackAction::Serve { ref path, .. })) if !path.contains("..")
                ),
            "a capture may never escape the root, got: {got:?}"
        );
    }

    #[test]
    fn a_target_query_string_is_dropped_from_the_dag_path() {
        // A query is a request modifier, never part of a content-addressed path.
        let parsed = rules("/s/:code /target.html?code=:code 200\n");
        assert_eq!(
            match_fallback(&parsed, "/s/42"),
            Some(Ok(FallbackAction::Serve {
                path: "/target.html".into(),
                status: 200,
            }))
        );
    }

    #[test]
    fn a_matching_3xx_rule_is_refused_with_its_reason_not_silently_skipped() {
        // What did NOT land (recorded in the module docs): a redirect is a
        // NAVIGATION the scheme-resolution seam cannot express yet. A matching
        // 3xx rule fails the load with a legible reason; falling through to the
        // next rule would serve a page the author never named for this path.
        for status in [301u16, 302, 303, 307, 308] {
            let parsed = rules(&format!("/old/* /new/:splat {status}\n/* /404.html 404\n"));
            assert_eq!(
                match_fallback(&parsed, "/old/thing"),
                Some(Err(RedirectsError::RedirectNotSupported {
                    status,
                    to: "/new/thing".into(),
                }))
            );
        }
    }

    #[test]
    fn the_410_and_451_error_page_statuses_are_served_like_the_404_one() {
        // The spec's other served-with-a-status codes behave exactly like 404:
        // the named page's verified content, with that status.
        let parsed = rules("/gone/* /410.html 410\n/unavail/* /451.html 451\n");
        assert_eq!(
            match_fallback(&parsed, "/gone/x"),
            Some(Ok(FallbackAction::Serve {
                path: "/410.html".into(),
                status: 410,
            }))
        );
        assert_eq!(
            match_fallback(&parsed, "/unavail/x"),
            Some(Ok(FallbackAction::Serve {
                path: "/451.html".into(),
                status: 451,
            }))
        );
    }
}
