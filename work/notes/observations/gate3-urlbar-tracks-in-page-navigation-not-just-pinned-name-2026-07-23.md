---
title: "Gate-3 conductor review: urlbar-tracks-in-page-navigation-not-just-pinned-name (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: urlbar-tracks-in-page-navigation-not-just-pinned-name
gate: gate-3-conductor
mergedCommit: f8f9662
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge. Driven in place from backlog via `dorfl do ... --allow-backlog --isolated --review --merge` (correct PATH ordering: real `/usr/local/bin/pi` + system `/usr/bin/node`, no volta shim - no ENOENT). Re-ran the in-page tests locally.

## Done-move + landing

- `work/tasks/backlog/urlbar-tracks-in-page-navigation-not-just-pinned-name.md` -> `done/` on origin/main (squash merge `f8f9662`).
- Files: `crates/renderer/src/lib.rs` (+16: a `LoadEvent::url()` accessor), `crates/werust-core/src/lib.rs` (+207: `pinned_root_key` + `drop_pin_on_in_page_nav` in pump + the FakeBackend `navigate_in_page`), a `pin-vs-follow-decision.md`, gate-2 nits note.

## Acceptance criteria (ticked, re-verified locally)

- [x] In-page navigation on an ENS page UPDATES the bar (drops the pinned `.eth` name, follows the backend URL) instead of freezing on the bare name. Mechanism: `pinned_root_key` records the normalized CID key the `url_override` name is pinned FOR; in `pump()`, `drop_pin_on_in_page_nav` compares each drained event's normalized URL key against it - the root's own events keep the pin, an in-page nav's DIFFERENT key drops it. Test `in_page_navigation_on_an_ens_page_updates_the_bar_and_back_re_derives_the_name`.
- [x] The resolved-ROOT ENS entry still shows the `.eth` name + posture (first load, and on history return via the normalized `ens_pages` re-derive from the blockedBy task). Dropping the pin on in-page nav is safe precisely because the root is recoverable via that re-derive - the two tasks compose exactly as designed.
- [x] Posture tracks the ACTUAL load path during in-page navigation. `refresh_chrome` re-marks ENS axes only for a known `ens_pages` entry; an in-page move to a non-ENS resource is not in `ens_pages`, and the backend resets posture to `UnverifiedOrigin` on each fresh `Started`, so no stale ENS/verified posture lingers. No posture-rule change needed beyond dropping the pin.
- [x] A plain (non-ENS) page tracks its URL on in-page nav, unregressed. Test `in_page_navigation_on_a_plain_page_tracks_its_url_unregressed`.
- [x] The pin-vs-follow decision recorded durably (`docs/spikes/.../pin-vs-follow-decision.md`): chose Option 2 (drop-the-pin/follow-the-URL), the task's recommended one, with the `name/<path>` nicety explicitly deferred (it needs the follow fallback anyway for off-root/off-site links, so it would be a second conditional rule over the same honesty).
- [x] Tests cover in-page nav updates the bar, root re-derives the name, posture tracks the path, plain unregressed - network-isolated. The FakeBackend gained `navigate_in_page`, modelling the previously-unmodeled link-click load the backend delivers WITHOUT the shell calling `navigate` - the exact seam path where the bar used to freeze. (Same class of harness-honesty upgrade as the ens-history task: the fake now expresses the real behaviour that hid the bug.)

## Design coherence

`pinned_root_key` is the missing companion to `url_override`: `url_override` is the display STRING, `pinned_root_key` is WHICH root the name is pinned for (its normalized CID), so the pin's root-only SCOPE is now representable. It reuses existing vocabulary (the "pin" language + the normalized-key concept), overlaps neither `ens_pages` (CID->identity for history) nor `url_override` (the string), and is consulted only in `pump` where in-page events arrive. Correct layer, no duplication. The new `LoadEvent::url()` accessor is a small, clean seam addition (inspect an event's target without matching every variant).

## Review-nits triage (Gate-2)

1. Ratify the pin-vs-follow choice (in-page nav follows the backend URL / drops the `.eth` pin, rather than `name/path`). User-visible default; the task's recommended Option 2; browser-idiomatic (Brave/Opera show the real location); avoids a conditional second display rule. RATIFIED.
2. The PR/commit body has no `## Decisions` block; the decision lives only in the spike file. The decision IS durably recorded (the spike md), so the intent is satisfied; captured here too for the PR-text reader. RATIFIED - minor, no action.

Neither blocks.

## Net effect

The v0.2.3 "bar frozen on the .eth name during in-page navigation" finding is fixed: the bar now follows the real location as the user navigates within an ENS page, while the root entry re-shows its name+posture on history return via the normalized ens_pages re-derive. This is the natural composition of this task with `ens-history-name-rederive-async-and-normalized`.
