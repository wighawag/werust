---
title: "Scheme-less entry routing: .eth -> ENS, else valid host -> try https:// (browser-style in-page error on load failure), else INVALID -> red-underline badge + keep the typed text"
slug: scheme-less-entry-https-fallback-and-keep-bar-on-error
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

FIELD FINDING (v0.2.3, human, ALL PLATFORMS): typing a scheme-less host like `github.com` does NOT try `https://github.com` - it is rejected and the URL bar RESETS to the previous page with NO error. The human's desired behaviour (verbatim intent): "on `.eth` => go for ENS; else, check valid url: if valid, try and show error on main page like other browsers; if invalid, show invalid url, maybe simply a little badge and show url underlined red or something." Root-cause source: `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md` (finding D).

READ-FIRST / drift check: confirm the mechanism. `eth_name_from_entry` (`crates/werust-core/src/lib.rs`) only recognises a bare `.eth` name; anything else scheme-less falls to `renderer.navigate(entry)`, whose `validate_url` (`crates/webview-renderer/src/lib.rs`, and the iOS/Android twins `crates/werust-ios/rust/src/backend.rs`, `crates/werust-android/rust/src/backend.rs`) REQUIRES a `scheme://` and returns `Err(RendererError::InvalidUrl)` BEFORE the chrome is touched, so `BrowserShell::navigate` returns that `Err` early - no `url_override`, no `last_error` - leaving the bar on the prior page with nothing shown.

Implement the three-way entry routing in the shell's `navigate` (the single front door), so it is consistent across all platforms:

1. **Bare `.eth` name** -> the ENS front door, exactly as today (`navigate_ens_name`). Unchanged.
2. **Else, a VALID host/URL** (has a scheme already, OR is a scheme-less string that parses as a valid host/authority - e.g. `github.com`, `example.com/path`, `localhost:8080`): NAVIGATE it, prepending `https://` when there is no scheme (the browser-idiomatic default). If the LOAD then fails (DNS/unreachable/etc.), surface it like a normal browser: an in-page error on the main page (the backend's own failed-load surface + the chrome's honest reason via `last_error`), with the bar KEEPING the attempted URL (not resetting to the previous page). This is a LOAD failure of a valid target.
3. **Else, INVALID** (neither a bare `.eth` name nor a parseable host/URL - e.g. a stray token, spaces, garbage): do NOT navigate. Surface an INVALID-URL state distinct from a load failure: a small BADGE indicating "invalid URL" and the URL-bar text rendered as INVALID (underlined red / error styling), KEEPING the typed text so the user can fix it. NEVER silently reset the bar to the previous page.

Design notes:
- The valid-vs-invalid CLASSIFIER (is this scheme-less string a plausible host/URL?) belongs in the toolkit-free core (`werust-core`) so all edges share ONE rule and it is unit-testable - mirror the existing `eth_name_from_entry` placement. Keep it conservative and honest (a dotted host, an authority with an optional port/path, an IP, `localhost`; reject empty/space/garbage). Do not over-engineer a full URL spec parser; a pragmatic host/URL check is fine, recorded.
- The INVALID surface is a NEW chrome state distinct from `last_error` (which is a load failure). Add it as its own small axis on `ChromeState` (e.g. an `invalid_entry: Option<String>` or a `entry_validity` field) so each edge (desktop + mobile) can paint the badge + red-underline from ONE fact, exactly as `last_error` / `load_step` / `trust_posture` are read. Do NOT re-mean `last_error` (a load failure) or the trust posture. Loading/error/validity stay orthogonal to trust.
- Retry/https-prepend must not double-prepend a scheme, and must not hijack an explicit scheme (`ipfs://...`, `https://...`, `http://...` are taken literally as today).

## Acceptance criteria

- [ ] A scheme-less `.eth` name still routes to the ENS front door (unchanged).
- [ ] A scheme-less VALID host (e.g. `github.com`) navigates as `https://github.com`; an entry that already has a scheme is taken literally (no double scheme, no hijack of `ipfs://`/`http://`).
- [ ] When a VALID target's LOAD fails (DNS/unreachable), the app shows a normal browser-style in-page error on the main page and the bar KEEPS the attempted URL - it does NOT reset to the previous page.
- [ ] An INVALID entry (not `.eth`, not a parseable host/URL) does NOT navigate: it surfaces a distinct INVALID-URL state - a small badge + the URL-bar text shown invalid (red underline / error styling) - and KEEPS the typed text for the user to fix; the bar is never silently reset to the previous page.
- [ ] The valid-vs-invalid classifier lives in `werust-core` (one shared, unit-tested rule) and is conservative + honest; the invalid-entry state is a new orthogonal chrome axis, not a re-meaning of `last_error` or the trust posture.
- [ ] Applied on desktop and mobile (the entry routing is in the shared core; the badge + red-underline are painted per edge from the shared chrome fact), or tracked per the parity guard.
- [ ] Tests cover: `.eth` -> ENS; scheme-less valid host -> https prepend + navigate; explicit scheme -> literal; valid-but-failing load -> in-page error + bar keeps the URL; invalid entry -> invalid state + typed text kept + no navigation + no bar reset. Fake backend, network-isolated.

## Blocked by

- None. (Independent of the ENS-history and in-page-nav tasks, though it touches the same `navigate` front door; land order is flexible.)
