---
title: "Gate-3 conductor review: debug-view-console-network-tabs-desktop (APPROVE)"
date: 2026-07-28
status: open
reviewOf: debug-view-console-network-tabs-desktop
verdict: approve
---

## Verdict: APPROVE

Merged as `47569be`, after two Gate-2 blocks (both CORRECT) and two conductor-prescribed recoveries, on the kimi-k3 model. No review-nits note was left on this one: the two rounds' blocks were the substance, and both were fixed. 329 `werust-core` tests and 18 `werust` (desktop) tests re-run locally green.

## Acceptance criteria, ticked against the merged tree

- [x] **The menu's Debug entry opens a Console + Network tabbed view rendering the core store.** A GTK `Notebook` with the two tabs, filling the open-debug-view hook the menu task left, as a separate window (not a busy panel). The menu's Debug item is now wired to the real view.
- [x] **Console shows level + message + source:line; Network shows method/url/status/mime/size + the honest per-request trust, in the indicator's own vocabulary.** The decisive coherence point: the Network tab reuses the SAME trust wire names as the chrome trust indicator, pinned by `the_network_trust_column_speaks_the_chrome_trust_indicators_exact_vocabulary`. **No new trust label was minted.**
- [x] **The view updates as new entries are captured, and does NOT freeze at the cap.** This is where the two rounds went. The first fix added a monotonic per-entry `sequence` so eviction is detectable; Gate-2 correctly caught that this detected eviction but the AppendFrom path still never REMOVED the evicted rows (row removal existed only on the Rebuild path), so the view climbed past 300 toward ~600 and stayed stale for ~300 pushes. The second fix added `TailPlan::AppendFrom { drop, from }` + `drop_top_rows`, dropping exactly the rows below the snapshot head. The regression test is precisely the requirement: `pushing_past_the_cap_still_renders_the_newest_entry_and_drops_the_evicted_rows`.
- [x] **Refresh reuses the existing pump cadence; no busy loop.** The view refreshes on the same pump the rest of the shell uses.
- [x] **Clear action empties the store.** `DebugCapture::clear` on BOTH buffers, wired to the view.
- [x] **The F12 WebKit inspector coexists.** There is a test asserting F12 opens the web inspector and the GTK debugger chord does not, so the debug view and the deep devtools do not collide.
- [x] **Desktop-scoped, parity-tracked**, with recorded manual steps.

## Coherence

The view renders the ONE shared store rather than its own copy, uses the established trust vocabulary, and refreshes on the existing cadence — three separate chances to fork a concept, none taken. The sequence-anchor refresh is a clean fit: it anchors on the store's monotonic sequence, NOT on a length that is meaningless once a ring buffer saturates.

## The two Gate-2 blocks, and why they were worth the rounds

Both were the SAME defect class — a refresh that is correct for the append-only case and silently wrong exactly when the ring buffer does its job. The first round caught that `len == rendered` at the cap fires nothing, so the view froze; the second caught that the sequence fix detected eviction without dropping the rows. Each was a real bug a user would hit within one busy session, and each regression test now pins the behaviour. This is the review loop working as intended: two rounds, two real bugs, neither shipped.

## For the human

The GTK view itself is exercised only by source-shape and logic tests, not a compiled-and-rendered pass, so the one thing left for a human is to OPEN the menu -> Debug on a real desktop build and confirm the two tabs render, the Network tab shows the trust badges in the indicator's vocabulary, and a long session keeps rendering past 300 entries.
