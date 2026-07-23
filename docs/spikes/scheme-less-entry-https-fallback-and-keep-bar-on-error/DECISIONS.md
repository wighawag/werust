# Scheme-less entry routing: decisions

Durable record for task `scheme-less-entry-https-fallback-and-keep-bar-on-error` (spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`, field finding D in `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`). Linked from the task's done record. These are the load-bearing choices a reviewer or a later task should be able to ratify or reverse.

## The three-way routing lives in one shared core classifier

`werust-core::classify_entry` (a sibling to `eth_name_from_entry`) is the ONE rule every OS edge shares: a NON-`.eth` URL-bar entry is either an `ExplicitScheme` (take literally), an `HttpsCandidate` (scheme-less plausible host, prepend `https://`), or `Invalid` (garbage, do not navigate). `BrowserShell::navigate` is the single front door that applies it. This mirrors the existing `eth_name_from_entry` placement so the classification is unit-tested at the seam boundary and cannot drift per platform.

Alternative considered: classify per edge (as the iOS `normalizeURL` did). Rejected: it duplicated the rule, sat at the wrong layer, and pre-empted the core (see below).

## The classifier is conservative and honest, not a URL-spec parser

`is_plausible_authority` accepts `localhost` (bare or `:port`), or a DOTTED host (at least one internal `.` with non-empty labels), optionally with a numeric `:port` and an optional `/path?query#frag`. It rejects: empty, any whitespace/control char, a bare dotless token (`garbage`), userinfo (`user@host`), a non-numeric port, and a malformed authority. It is deliberately pragmatic: the backend + the network are the final arbiters of whether a plausible host actually loads. A too-liberal classifier would silently turn a typo (`garbage`) into a doomed `https://garbage` load; a too-strict one would reject real hosts. The dotted-host + `localhost` rule is the honest middle.

Touches: the browser-idiomatic `https://` default for a scheme-less host (matches Brave/Chrome/Firefox). It does NOT touch the `.eth` rule (peeled off first by `eth_name_from_entry`) nor any explicit scheme (taken literally, never re-prefixed or hijacked).

## The INVALID state is a NEW orthogonal chrome axis, not a re-meaning of `last_error`

`ChromeState::invalid_entry: Option<String>` is a distinct axis from `last_error` (a LOAD failure of a valid target) and from the `trust_posture`. An invalid entry is NOT a load failure: nothing was navigated. Keeping them separate lets each edge paint the small "invalid URL" badge + red-underlined URL bar from ONE fact, while a valid-but-failing load still shows the normal in-page error banner (keeping the attempted URL in the bar). Re-using `last_error` would have conflated "you typed garbage" with "the page failed to load", which the field finding explicitly distinguishes.

The typed text is KEPT in the bar (pinned via `url_override`) and the bar is never reset to the previous page, on both the invalid path and the valid-but-failing-load path.

## The edges pass RAW typed text; the iOS `normalizeURL` was removed

The desktop and Android edges already passed the raw URL-bar text to `core.navigate`. The iOS Swift edge had its own `normalizeURL` that prepended `https://` for a bare host BEFORE the core saw it. That pre-empted the core classifier: a scheme-less garbage entry became `https://garbage`, which the core routed as an explicit scheme and tried to LOAD, surfacing a load error instead of the honest invalid-URL badge. That edge-level rule was removed so all three edges pass the raw text and the ONE core rule decides the route. This is the conceptual-coherence fix: one concept (entry routing) at one layer (the core front door), not forked per edge.
