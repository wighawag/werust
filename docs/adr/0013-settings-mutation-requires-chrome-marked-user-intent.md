# A settings MUTATION requires user intent MARKED by the chrome, never inferred from the request

A `werust://settings?backend=…&url=…` change is applied only for a request that is BOTH the MAIN FRAME and the navigation werust's own chrome MARKED as intended, carried out of band in a shared `NavigationIntent` handle the chrome writes and the scheme handler reads. Nothing about the intent is derived from the request, because everything a request carries is under page control. A request that fails the gate still RENDERS the settings page, read-only, with its real current values; READS are never gated.

## Status

accepted

## Context

The settings page is served through the SAME custom-scheme seam that serves page content, on all five edges, for ANY request including sub-resources — and it applied and persisted a change straight off the request's query string, because the page is a GET form. So one tag in any page silently repointed the user's IPFS retrieval backend:

```html
<img src="werust://settings?backend=custom&url=http://attacker.example/">
```

The retrieval backend is an EGRESS choice: it sees every content-addressed site the user visits. Content stays hash-verified (`docs/adr/0004`), so this is not a content-integrity break; it is a privacy and availability attack, and it is also how a hostile page would defeat the refusal path the trust work is built on — `withhold-changed-content-until-trusted` falls back to re-fetching the trusted version THROUGH the configured backend, so a page that repoints that backend at a dead endpoint makes "refuse the update" fail and leaves accept as the only working option.

There was no notion of a user gesture or a chrome-initiated navigation anywhere in the tree. The tree DID already have a main-frame notion (`RedirectSink::is_main_frame`, the one main-frame predicate), and it is necessary here but not sufficient.

## Decision

**The gate is two conditions and both are required.** A mutation is applied only for a MAIN-FRAME request whose navigation the chrome MARKED. Main-frame alone is insufficient: a page can navigate the top-level frame (`location = 'werust://settings?…'`), which says nothing about who asked. Marked alone is insufficient: a mark says a NAVIGATION was started, and a sub-resource request carries no navigation at all. Both halves are answered by ONE call, `NavigationIntent::take_mark_for(uri)`, so no caller can honour half the rule.

**The mark is TIGHTER than the frame key.** The main-frame check compares a `frame_key`, which STRIPS the query and fragment (it answers "is this the same document?"). Reusing it as-is would be a hole: while the user is legitimately ON `werust://settings`, a sub-resource request for `werust://settings?backend=custom&url=…` reduces to the SAME key and passes. So the mark is keyed on the frame key's normalization PLUS the query (`intent::intent_key`), built ON the one main-frame notion rather than beside it. It is not a raw string compare, because WebKit hands the handler its own spelling of a URL (the authority-less `scheme:///host` form, a trailing slash on an empty path) and those are the same navigation.

**Exactly two things mark, and both are chrome.** `BrowserShell::navigate` is a chrome-only entry point (an in-page link click, a `window.open` and a `location=` never pass through it — the property the redirect sink's per-chain reset already relies on) and every edge's URL bar commits its raw typed text to it, so marking there gives every wired edge the URL-bar half for free. The settings page's OWN link/form GET is page-initiated and so cannot come from the shell; each edge marks it from its native navigation-policy callback, requiring both that the navigation was ACTIVATED in the page (a link click / form submit, never a script's `location =`) and that the document it starts FROM is a `werust://` page — a surface werust itself drew, which web content can never be at. The `_blank`/`window.open` in-place hook (`docs/adr/0010`) must NOT mark: that target is a page's choice.

**The carrier is a shared handle, not a widened seam.** `renderer::SchemeRequest` carries only a URI, and it is constructed by edge halves the Linux gate cannot compile (macOS, Windows, iOS, Android). Widening it would be both an invisible cross-platform break and the wrong shape — a flag ON the request is a claim the page's request carries. So intent lives core-side in an `Arc`-shared, `Send + Sync` `NavigationIntent`, cloned into the scheme handler and handed to the shell, exactly as `RedirectSink` already solves the same problem for the same reason (the GTK scheme handler runs off the UI thread, `docs/adr/0008`).

**The refusal is of the CHANGE, not of the surface.** A refused request still returns the page with its REAL current values and a stated reason (`retrieval::NOT_STARTED_BY_WERUST`), so a blocked attack does not look like a broken browser and the behaviour is observable, therefore testable. Reads need nothing at all.

## Considered options

- **Infer intent from the request** (a referrer, a header, a `Sec-Fetch-*`-style hint, the URL shape). Rejected: every one of those is under the attacking page's control at the point the gate reads it, so it is a claim, not an authorisation.
- **Widen `renderer::SchemeRequest` with an is-main-frame / user-initiated flag.** Rejected for the two reasons above; pinned by `crates/werust-core/tests/settings_user_intent_edge_wiring_shape.rs`.
- **A CSRF-style nonce embedded in the rendered page's form.** Rejected: reads are deliberately ungated, so the token sits in a document any page may cause to be rendered, and defending it would mean reasoning about the custom scheme's origin and CORS behaviour on five webview engines. The chrome already knows what it started; asking it is simpler and does not depend on engine-specific origin semantics.
- **A process-global intent carrier**, so every edge inherits the marking without wiring. Rejected: this repo has deliberately removed globals from exactly this kind of state (the pin store's `PinStoreLocation`, the settings directory's explicit-directory seam), and a global would let one window's mark authorise another window's request.

## Consequences

- **An edge that has not wired the carrier fails CLOSED, in both directions.** Its settings page still renders with real values, but BOTH change paths — its URL bar and its own form — are refused until its task lands (`macos-`, `windows-`, `ios-`, `android-marks-user-intent-for-settings-mutations`, tracked as `stubbed` cells of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml`). That is wider than a form-only gap, because the carrier reaches an edge's handler only through that edge's own wiring; it is the safe direction, and it is temporary.
- **A mark is SINGLE-USE and is REPLACED by the next navigation.** So a replay of a mutating URL, or a later request that happens to name it, finds nothing to spend. The visible corollary: a RELOAD of a mutating settings URL, and a Back/Forward onto one, re-render the page WITHOUT re-applying the change. That is deliberate (a history move is not a fresh decision to change a setting), and it is why the page's status line distinguishes a save from a refusal.
- **The intent concept is now available for any future internal `werust://` surface**, and any such surface inherits the rule rather than inventing a second one. That is the intended reading: the ADR is about MUTATIONS reached through a page-reachable internal scheme, not about the retrieval setting specifically.
