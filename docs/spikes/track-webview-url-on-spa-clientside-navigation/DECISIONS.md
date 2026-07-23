# Decisions: track SPA client-side URL change + root-CID-prefix ENS association

Task `track-webview-url-on-spa-clientside-navigation`, spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`. Two coupled fixes; the design choices they force are recorded here and linked from the done record.

## Decision 1 — a same-document URL change is a NEW `LoadEvent` variant, not a faked load lifecycle event, and not a separate poll

The acceptance criteria require a same-document URL change to be modelled as a signal DISTINCT from a load lifecycle event (never a faked `Started`/`Committed`/`Finished`). Two shapes were on the table (per the task): a new `LoadEvent` variant, or a separate `poll_url_change()` on the seam.

Chosen: a new `LoadEvent::UrlChanged { url }` variant.

Why:

- It flows through the ONE existing drain (`BrowserShell::pump()` -> `renderer.poll_event()`), so the pin-drop/follow + `ens_pages` re-derive machinery the `urlbar-tracks-in-page-navigation-not-just-pinned-name` task already built runs unchanged. A separate `poll_url_change()` would fork the drain and duplicate that logic.
- `LoadEvent::url()` already exists and covers every variant, so `drop_pin_on_in_page_nav(event.url())` works for it with no extra accessor.
- It composes with the FakeBackend's existing `navigate_in_page` shape: a new `change_url_in_page` twin emits ONLY this event without a full load.

What it must NOT do (the "do not fake a load lifecycle" / "do not re-mean trust" rule):

- `UrlChanged` MUST NOT move `LoadState` and MUST NOT touch the trust posture on any backend. A same-document nav is not a fresh load: the document (and its already-verified origin) is unchanged, the SPA only rewrote the history URL. So each backend emits `UrlChanged` WITHOUT calling `begin`/`commit`/`finish` and WITHOUT resetting `posture`/`ens_origin`/`mutable_name`.
- In `pump()`, `UrlChanged` drives the SAME pin-drop/follow + `ens_pages` re-derive as an in-page load event, but it is NOT matched by the lifecycle arms that clear/set `last_error` — it only updates the displayed URL when not pinned. `refresh_chrome()` (called at the end of `pump`) then re-derives the ENS identity + posture from the backend's current URL exactly as for any other entry.

What it TOUCHES: the seam `LoadEvent` enum (a new variant every `match` must handle — desktop backend's `load-changed` match, the shell `pump` match, and any exhaustive match elsewhere), the FakeBackend, and each platform's OS-edge observation. It does NOT change `LoadState` or `TrustPosture`.

## Decision 2 — the ENS association is matched on the ROOT CID PREFIX of the current entry, not only its exact normalized key

Part 2, the `ipfs://`-reappears leak: `ens_pages` was populated root-only (keyed on the resolved entry's exact `normalize_ens_page_key`, e.g. the bare `<rootcid>` for `ronan.eth`, or `<rootcid>/blog` for `ronan.eth/blog/`). A back/forward/reload (or SPA nav) that lands `current_url` on a DIFFERENT sub-path of the same site (`<rootcid>/blog/post-1`) has a normalized key that misses the exact-key lookup, so `refresh_chrome` leaked `ipfs://<rootcid>/blog/post-1` into the bar.

Chosen: each `ens_pages` entry additionally records the site's ROOT CID + the ROOT `.eth` name (`ronan.eth`, never the sub-path display). Lookup for a current URL:

1. Exact normalized-key hit -> use its stored display name (unchanged behaviour; preserves the `.eth/blog/` exact-entry display and its `mutable` flag).
2. Else, extract the current URL's root CID (the first `ipfs://` segment) and its in-site path; if that root CID matches a known site's root CID, derive the display as `<rootname>/<in-site-path>` (or bare `<rootname>` at the root) and re-mark the posture with that site's `mutable` axis.

So the association is with the WHOLE SITE (its root CID), and ANY `<rootcid>/<anypath>` re-derives `ronan.eth/<path>` + posture, never the raw CID. A non-`ipfs://` (plain served) URL has no root CID, so it never matches — plain pages are wholly unaffected.

What it TOUCHES: the `EnsIdentity` struct (gains `root_cid` + `root_name`), `load_resolved_content` (records them), and the `ens_pages` lookup used by both `refresh_chrome` and `reload`. It reuses `normalize_ens_page_key` for the CID/path split (the same canonicalization used at insert), so the WebKit authority-variance fix still holds.

## Decision 3 - a NEW `spa-url-tracking` capability row in the parity matrix, not folded into `address-bar`

The parity matrix (`docs/platform-capability-matrix.toml`, ADR-0005) already carries an `address-bar` capability ("reflect the current URL in the chrome"). SPA same-document URL tracking could be read as part of that. Chosen: a SEPARATE `spa-url-tracking` capability row, `implemented` on all three platforms.

Why separate (the coherence check): `address-bar` is driven by the shared core over `navigate`/`current_url` (backend-neutral logic). SPA tracking is DIFFERENT: each platform wires a DISTINCT OS-edge observation that has nothing to do with `navigate`: desktop `notify::uri`, Android `WebViewClient.doUpdateVisitedHistory`, iOS KVO on `webView.url`. A regression that re-stubs ONE of those edges (drops the KVO observer, forgets `doUpdateVisitedHistory`) would ship the frozen-bar bug on that platform ONLY, exactly the silent-one-platform gap the matrix exists to forbid. Folding it into `address-bar` (already `implemented` everywhere) would hide that: the guard cannot distinguish "the address bar works" from "same-document tracking works". So it is its own row, whose cells pin the three OS-edge observations. It does not re-mean `address-bar`; it is the narrower, edge-specific capability underneath it.

## Coherence with existing concepts

- Reuses `pinned_root_key` / `url_override` (the pin-vs-follow decision, `docs/spikes/urlbar-tracks-in-page-navigation-not-just-pinned-name/`): an `UrlChanged` off the pinned root drops the pin exactly as an in-page load event does.
- Composes with `eth-name-with-path-routes-to-ens-and-subpath`: a `.eth/blog/` load stores its root CID + `ronan.eth` root name, so an in-SPA nav to `/portfolio` re-derives `ronan.eth/portfolio` via the prefix lookup.
- Does NOT introduce a new trust concept: `UrlChanged` never marks or clears trust; the posture keeps tracking the actual document (a same-document nav within a verified `ipfs://` site stays verified because the backend never reset it).
