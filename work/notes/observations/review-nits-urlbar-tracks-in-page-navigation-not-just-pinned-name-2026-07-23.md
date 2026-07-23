---
title: review-gate non-blocking nits for 'urlbar-tracks-in-page-navigation-not-just-pinned-name' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: urlbar-tracks-in-page-navigation-not-just-pinned-name
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'urlbar-tracks-in-page-navigation-not-just-pinned-name' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the pin-vs-follow design choice: on in-page navigation the bar FOLLOWS the backend URL (drops the .eth pin) rather than showing name/path. This is user-visible default behaviour and the task's recommended option 2.
  (Recorded in docs/spikes/urlbar-tracks-in-page-navigation-not-just-pinned-name/pin-vs-follow-decision.md with rationale (browser-idiomatic, avoids a conditional second display rule; name/path left as a later cosmetic nicety). Well-justified; flagging only for human ratification.)
- The commit/PR message carries no ## Decisions block; the pin-vs-follow decision lives only in the spike file. Minor: the decision IS durably recorded, but a reviewer starting from the PR text would miss it.
  (git log for bba63c5 is a one-line subject; decision is in the spike md. Intent of durable recording is satisfied.)
