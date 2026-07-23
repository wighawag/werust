---
title: "Gate-3 conductor review: chrome-loading-state-resets-trust-indicator (APPROVE)"
date: 2026-07-23
status: open
reviewOf: chrome-loading-state-resets-trust-indicator
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: ded8c07
---

## Verdict: APPROVE ✅ — merged as ded8c07 (field-issue #5)

Closes the trust-honesty gap the human spotted: on navigation the stop-cross appeared but the trust indicator kept showing the PREVIOUS page's posture while a new (possibly differently-trusted) site loaded.

## Acceptance criteria — all met
- While `is_loading()`, the trust indicator shows a NEUTRAL loading badge (`trust-loading`), NOT the carried-over posture. On settle it shows the new page's real posture.
- A fresh navigation clears stale name + error into the new load (`a_fresh_navigation_clears_the_stale_name_and_error_into_the_new_load`).
- Tests: `a_fresh_navigation_shows_a_neutral_loading_state_hiding_the_prior_posture_until_settle`.
- Applied on desktop + Android (WerustCore.kt) + iOS (WerustCore.swift) — parity honoured.

## Gate-2 nits: 3 non-blocking, recorded.
