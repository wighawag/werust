//! The chrome's **navigation intent** mark: the one carrier that says "werust's
//! own chrome started this navigation", so a settings MUTATION can require the
//! user to have asked for it (`docs/adr/0013`, spec
//! `settings-mutations-require-user-intent`).
//!
//! # What this exists to stop
//!
//! `werust://settings?backend=<kind>&url=<endpoint>` is applied straight off the
//! request's query string, and the `werust` scheme is served through the same
//! scheme-handler seam that serves page content — for ANY request, including a
//! sub-resource. So before this module, one tag in any page silently repointed
//! the user's IPFS retrieval backend:
//!
//! ```html
//! <img src="werust://settings?backend=custom&url=http://attacker.example/">
//! ```
//!
//! The retrieval backend is an EGRESS choice (it sees every content-addressed
//! site the user visits), so that is a privacy + availability attack, and it is
//! how a hostile page would defeat the refusal path the trust work is built on.
//!
//! # The rule: MAIN FRAME **and** MARKED
//!
//! A mutation is applied only for a MAIN-FRAME request whose navigation the
//! chrome MARKED as intended, and both halves are required:
//!
//! * main-frame alone is insufficient — a page can navigate the top-level frame
//!   (`location = 'werust://settings?...'`), which says nothing about who asked;
//! * marked alone is insufficient — a mark says the chrome started a NAVIGATION,
//!   and a sub-resource request carries no navigation at all.
//!
//! [`NavigationIntent::take_mark_for`] answers both halves at once, which is why
//! an edge captures ONE handle rather than two.
//!
//! # Marked by the chrome, never inferred from the request
//!
//! Everything derivable from a request (a referrer, a header, the URL shape) is
//! under page control, so nothing here reads the request except to compare it to
//! what the chrome already said it was navigating to. Exactly two things mark:
//!
//! 1. [`BrowserShell::navigate`](crate::BrowserShell::navigate) — the chrome-only
//!    front door. An in-page link click, a `window.open` and a `location=` never
//!    pass through it (the [`RedirectSink`] documentation states and relies on
//!    this), while every edge's URL bar commits its raw typed text to it (the
//!    `address-bar` row of `docs/platform-capability-matrix.toml`). So the URL-bar
//!    half is shared by every edge for free.
//! 2. The per-edge hook for a navigation the user starts INSIDE a surface werust
//!    itself drew — the settings page's own link/form GET. That one cannot come
//!    from the shell (a form submission is a page-initiated navigation), so each
//!    edge supplies it from its own navigation-policy callback; on GTK that is
//!    `WebViewRenderer::install_settings_page`. An edge that has not wired it yet
//!    fails CLOSED: the page still renders, the change is refused.
//!
//! The `_blank` / `window.open` in-place navigation hook is the counter-example
//! that must NOT mark: every edge routes those into the view's own load path,
//! deliberately bypassing the shell (`docs/adr/0010`), and a page chooses that
//! URL.
//!
//! # A shared handle, because the reader is off the main thread
//!
//! On GTK the scheme handler runs OFF the UI thread (`docs/adr/0008`), so the
//! carrier is an `Arc`-shared, `Send + Sync` handle with its own interior
//! locking, cloned into the handler and handed to the shell
//! ([`BrowserShell::with_navigation_intent`](crate::BrowserShell::with_navigation_intent)) —
//! the same shape the [`RedirectSink`] already uses for the same reason, rather
//! than a second channel idiom.
//!
//! A shell whose edge wires no carrier keeps its own private one: it is marked
//! and never read, so nothing changes for a caller that does not serve
//! `werust://` at all.

use std::sync::{Arc, Mutex};

use crate::ipfs::{frame_key, RedirectSink};

/// The mark werust's chrome leaves on a navigation it started, readable by the
/// scheme handler that must decide whether a `werust://` request may MUTATE.
///
/// Cloning shares one mark (it is an `Arc` handle), which is the point: the
/// chrome's clone and the scheme handler's clone are the same carrier. See the
/// [module docs](self) for the rule and why nothing is inferred from the request.
#[derive(Debug, Clone, Default)]
pub struct NavigationIntent {
    /// The ONE outstanding mark, or `None` when the chrome has started nothing
    /// (or the mark has been spent). A new mark REPLACES any previous one, so a
    /// later navigation can never leave two live authorisations.
    mark: Arc<Mutex<Option<Mark>>>,
}

/// One outstanding mark: WHICH navigation the chrome started, and the frame sink
/// that will answer whether the request asking to spend it is the main frame.
#[derive(Debug)]
struct Mark {
    /// The [`intent_key`] of the URL the chrome navigated to — the frame key's
    /// normalization PLUS the query, so it is strictly TIGHTER than the
    /// main-frame key (see [`intent_key`]).
    key: String,
    /// The main-frame authority ([`RedirectSink::is_main_frame`]), captured at
    /// mark time so the carrier can answer BOTH halves of the gate on its own and
    /// an edge has one handle to capture rather than two. It is a shared handle,
    /// so this is the SAME sink the shell reports its navigations into, not a
    /// snapshot.
    frames: RedirectSink,
}

impl NavigationIntent {
    /// A fresh carrier holding no mark: it authorises nothing.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that werust's own chrome is starting a navigation to `url`, with
    /// `frames` the sink the shell reports its top-level navigations into.
    ///
    /// Call this ONLY from chrome code paths (see the [module docs](self)):
    /// anything a page can trigger must not reach here, because a mark is the
    /// whole authorisation. It REPLACES any outstanding mark, so at most one
    /// navigation is authorised at a time and a stale mark cannot survive a later
    /// navigation.
    ///
    /// Marking is harmless for a navigation that has nothing to do with settings
    /// (the overwhelming majority): the mark is matched against the exact URL, so
    /// a mark for `https://example.com` authorises nothing else.
    pub fn mark(&self, url: &str, frames: &RedirectSink) {
        if let Ok(mut held) = self.mark.lock() {
            *held = Some(Mark {
                key: intent_key(url),
                frames: frames.clone(),
            });
        }
    }

    /// Whether a request for `uri` may MUTATE: `true` only when the chrome marked
    /// exactly this navigation AND this request is its MAIN FRAME. Spends the mark.
    ///
    /// Both halves are checked here so a caller cannot accidentally honour one of
    /// them (the whole gate is one call), and the mark is SINGLE-USE so the page
    /// that renders as a result — or any later replay of the same URL — finds
    /// nothing left to spend.
    ///
    /// A poisoned lock, an unmarked carrier and a mismatch are all `false`: the
    /// fail-closed direction is to refuse the CHANGE (the caller still renders the
    /// page read-only), never to apply one werust cannot vouch for.
    ///
    /// A mismatch deliberately leaves the mark ALONE rather than clearing it: a
    /// page's sub-resource request must not be able to burn the authorisation the
    /// user's own pending change is holding.
    #[must_use]
    pub fn take_mark_for(&self, uri: &str) -> bool {
        let Ok(mut held) = self.mark.lock() else {
            return false;
        };
        let Some(mark) = held.as_ref() else {
            return false;
        };
        // Half 1: the chrome started exactly THIS navigation, query included.
        if mark.key != intent_key(uri) {
            return false;
        }
        // Half 2: and this request is that navigation's own document, not one of
        // its sub-resources.
        if !mark.frames.is_main_frame(uri) {
            return false;
        }
        *held = None;
        true
    }

    /// Whether a mark is outstanding, for tests that assert who marks (the chrome)
    /// and who does not (anything a page can trigger). Test-only: production must
    /// go through [`take_mark_for`](NavigationIntent::take_mark_for), which spends
    /// the mark and checks the main-frame half with it.
    #[cfg(test)]
    pub(crate) fn is_marked(&self) -> bool {
        self.mark.lock().is_ok_and(|held| held.is_some())
    }
}

/// The comparison key for "is this the navigation the chrome started?": the
/// main-frame [`frame_key`]'s normalization, PLUS the query the frame key drops.
///
/// # Why it must be tighter than the frame key
///
/// [`frame_key`] answers "is this the same DOCUMENT?" and therefore strips the
/// query and fragment. That is right for the main-frame half and fatal for this
/// one: while the user is legitimately ON `werust://settings`, a sub-resource
/// request for `werust://settings?backend=custom&url=http://attacker.example/`
/// reduces to the SAME frame key and passes the main-frame check. So the mark
/// carries the query, and a mark for the settings page authorises no other query
/// on it.
///
/// # Why it is not a raw string compare
///
/// WebKit hands the scheme handler its OWN spelling of the URL the chrome asked
/// for: the authority-less `scheme:///host` form (already observed for `ipfs://`,
/// which is why [`frame_key`] normalizes it) and a trailing slash on an empty
/// path (`werust://settings/?backend=…`, which the settings handler's own host
/// parse already tolerates). Both are the same navigation, so both reduce here to
/// one key. The QUERY itself is compared verbatim: it is the part that carries
/// the change, and re-spelling it is not a variance any edge has been observed to
/// introduce.
fn intent_key(url: &str) -> String {
    let head = url.split_once('#').map_or(url, |(head, _)| head);
    let (base, query) = match head.split_once('?') {
        Some((base, query)) => (base, query),
        None => (head, ""),
    };
    // The frame key first, so an `ipfs://` URL reduces exactly as the main-frame
    // check reduces it...
    let base = frame_key(base);
    // ...then the same two collapses for a NON-`ipfs://` scheme, which the frame
    // key leaves alone: the empty authority and a bare trailing slash.
    let base = base.trim_end_matches('/');
    let base = match base.split_once("://") {
        Some((scheme, rest)) => format!("{scheme}://{}", rest.trim_start_matches('/')),
        None => base.to_string(),
    };
    if query.is_empty() {
        base
    } else {
        format!("{base}?{query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sink reporting `top_level` as the document the shell is loading: what
    /// the shell's `note_navigation` leaves behind for the request that follows.
    fn frames_on(top_level: &str) -> RedirectSink {
        let frames = RedirectSink::new();
        frames.note_navigation(top_level);
        frames
    }

    #[test]
    fn an_unmarked_carrier_authorises_nothing() {
        // The default state, and the state every edge that has not wired the
        // chrome's marking yet is in: the gate refuses, and the caller renders the
        // page read-only.
        let intent = NavigationIntent::new();
        assert!(!intent.take_mark_for("werust://settings?backend=custom&url=http://x/"));
    }

    #[test]
    fn a_marked_main_frame_navigation_is_authorised_exactly_once() {
        // The user's own change: the chrome marked this navigation and the request
        // IS its main frame. Spending it is single-use, so the page that renders as
        // a result cannot re-spend it.
        let uri = "werust://settings?backend=custom&url=http://127.0.0.1:8080";
        let intent = NavigationIntent::new();
        intent.mark(uri, &frames_on(uri));

        assert!(
            intent.take_mark_for(uri),
            "the marked main frame is authorised"
        );
        assert!(
            !intent.take_mark_for(uri),
            "a spent mark authorises nothing further"
        );
    }

    #[test]
    fn a_mark_does_not_carry_over_to_a_different_query() {
        // THE hazard the intent key exists for: the frame key strips the query, so
        // while the user is on `werust://settings` a sub-resource request for a
        // MUTATING settings URL is the same frame key and passes the main-frame
        // half. The mark must not also be satisfiable by it.
        let attack = "werust://settings?backend=custom&url=http://attacker.example/";
        let frames = frames_on("werust://settings");
        let intent = NavigationIntent::new();
        intent.mark("werust://settings", &frames);

        assert!(
            frames.is_main_frame(attack),
            "the frame key really does strip the query (the hazard is real)"
        );
        assert!(
            !intent.take_mark_for(attack),
            "a mark for the settings page must not authorise a different query on it"
        );
        assert!(
            intent.is_marked(),
            "and a page's attempt must not burn the user's own outstanding mark"
        );
    }

    #[test]
    fn a_marked_navigation_the_view_is_not_on_authorises_nothing() {
        // Marked-intent alone is not enough: the mark says a NAVIGATION was
        // started, and a sub-resource request carries no navigation. Here the
        // marked navigation is not the document the shell is loading.
        let uri = "werust://settings?backend=custom&url=http://attacker.example/";
        let intent = NavigationIntent::new();
        intent.mark(uri, &frames_on("ipfs://bafyattacker/index.html"));

        assert!(!intent.take_mark_for(uri));
    }

    #[test]
    fn a_later_navigation_replaces_an_earlier_mark() {
        // At most one authorisation is outstanding, so a mark cannot lie in wait
        // across a session for a request that happens to name the same URL.
        let first = "werust://settings?backend=custom&url=http://127.0.0.1:8080";
        let intent = NavigationIntent::new();
        intent.mark(first, &frames_on(first));
        intent.mark("ipfs://bafyroot/index.html", &frames_on(first));

        assert!(!intent.take_mark_for(first), "the earlier mark is gone");
    }

    #[test]
    fn the_intent_key_collapses_the_spellings_webkit_reports() {
        // The chrome marks the URL it asked for; the handler is handed WebKit's
        // own spelling of it. Same navigation, one key — otherwise the user's own
        // change would be refused on every edge.
        assert_eq!(
            intent_key("werust://settings?backend=custom"),
            intent_key("werust:///settings?backend=custom"),
            "the empty-authority form is the same navigation"
        );
        assert_eq!(
            intent_key("werust://settings?backend=custom"),
            intent_key("werust://settings/?backend=custom"),
            "a trailing slash on an empty path is the same navigation"
        );
        assert_eq!(
            intent_key("werust://settings?backend=custom"),
            intent_key("werust://settings?backend=custom#top"),
            "a fragment never reaches the server and is not part of the change"
        );
        assert_ne!(
            intent_key("werust://settings"),
            intent_key("werust://settings?backend=custom"),
            "but the query is part of the key: that is what makes it tighter than the frame key"
        );
    }

    #[test]
    fn the_intent_key_reduces_an_ipfs_url_exactly_as_the_frame_key_does() {
        // It is built ON the one main-frame notion rather than beside it, so the
        // two can never disagree about what document a URL names.
        assert_eq!(
            intent_key("ipfs://bafycid/page"),
            intent_key("ipfs:///bafycid/page/")
        );
        assert_eq!(intent_key("ipfs://bafycid"), frame_key("ipfs:///bafycid/"));
    }
}
