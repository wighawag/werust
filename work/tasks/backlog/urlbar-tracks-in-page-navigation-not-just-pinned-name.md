---
title: "URL bar must track in-page navigation (show the new path), not stay frozen on the pinned .eth name after a link click"
slug: urlbar-tracks-in-page-navigation-not-just-pinned-name
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [ens-history-name-rederive-async-and-normalized]
covers: [2]
---

## What to build

FIELD FINDING (v0.2.3, human, ALL PLATFORMS, noticed on an ENS page): "when navigating in the page, the url bar do not update to show the new path; it shows when going back but only after navigating to a new site first and then back." So after loading an ENS page and clicking a link WITHIN it, the URL bar stays frozen on the pinned `.eth` name and never shows where you actually navigated to. Root-cause source: `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md` (finding C).

READ-FIRST / drift check: confirm the mechanism. In `BrowserShell` (`crates/werust-core/src/lib.rs`), the ENS front door sets `url_override = Some(name)` and it PERSISTS across pumps ("the override PERSISTS across pumps so the name stays put for the whole load"). In `pump()`, the `LoadEvent` handlers write `chrome.url_text` ONLY when `!pinned` (i.e. only when there is no `url_override`). So while an ENS name is pinned, ANY subsequent in-page navigation that changes the backend URL is SUPPRESSED from the bar - the name stays but the path never appears. The "shows on back only after a new site first" symptom is the same stickiness clearing only once a non-ENS entry drops the pin.

Fix direction: distinguish "PIN the `.eth` name for the front-door ROOT load the user typed" from "FOLLOW the backend URL as the user navigates WITHIN or AWAY from that page." The pin should hold for the resolved-root entry (so `ronan.eth` shows while its CID loads), but a subsequent in-page navigation (a link click - a NEW backend load, not the front-door entry) should update the bar to reflect where the user now is. Decide + record which of these (recommended: the second, with the first as a nicety if cheap):
- Show the `.eth` NAME plus the in-page PATH suffix (e.g. `ronan.eth/some/page`) so the identity AND location are both honest - but only if the path can be derived cleanly from the backend URL relative to the resolved root; OR
- Drop the name pin once the user navigates OFF the resolved-root entry (a link click starts a fresh load), so the bar follows the backend URL for in-page navigation, while the ENS name is re-shown for the ROOT entry (including via the `ens_pages` re-derive when history returns to it).

Must stay coherent with:
- `ens-history-name-rederive-async-and-normalized` (this task is BLOCKED BY it): the `ens_pages` re-derive is how the ROOT entry re-shows its name on back/forward, so build on the normalized-key re-derive rather than fighting it. Dropping the pin on in-page nav is safe precisely because the root entry is recoverable via `ens_pages`.
- The reload re-resolve decision and the posture rules: the trust posture must keep tracking the ACTUAL load path (an in-page navigation to a non-verified resource must not keep showing a verified/ENS posture); loading/error stay orthogonal to trust.

## Acceptance criteria

- [ ] After loading an ENS page and navigating WITHIN it (a link click that changes the backend URL), the URL bar UPDATES to reflect the new location (either `name/<path>` or the backend URL per the recorded decision), instead of staying frozen on the bare pinned `.eth` name.
- [ ] The resolved-ROOT ENS entry still shows the `.eth` name + posture (on first load and when history returns to it, via the normalized `ens_pages` re-derive); the name is not lost for the root.
- [ ] The trust posture tracks the ACTUAL current load path during in-page navigation (an in-page move to a non-ENS/non-verified resource does not keep a stale ENS/verified posture); loading/error remain orthogonal to trust.
- [ ] A plain (non-ENS) page tracks its URL on in-page navigation exactly as a browser does (this was already fine for non-pinned pages; do not regress it).
- [ ] The pin-vs-follow decision is recorded durably (`docs/spikes/<slug>/` or an observation) with the chosen behaviour and why.
- [ ] Tests cover: in-page navigation on an ENS page updates the bar; the root entry still shows the name; back to the root re-derives the name; posture tracks the load path. Fake backend, network-isolated.

## Blocked by

- `ens-history-name-rederive-async-and-normalized` (the normalized `ens_pages` re-derive this task builds on; landing it first keeps the root-entry name recovery correct when the pin is dropped for in-page nav).
