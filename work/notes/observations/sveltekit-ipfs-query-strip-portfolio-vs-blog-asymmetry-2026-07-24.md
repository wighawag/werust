---
title: "SvelteKit-over-ipfs:// blog 500 root cause was the __data.json query-string leak; portfolio-vs-blog asymmetry is symptom-ordering, not a second bug"
date: 2026-07-24
status: open
kind: observation
---

Diagnosing finding D (desktop) for `diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture`: the ronan.eth blog "500" over `ipfs://` was `parse_ipfs_uri` leaking SvelteKit's `?x-sveltekit-invalidated=...` query into the DAG path, so `/blog/__data.json?x-sveltekit-invalidated=01` matched no directory entry and failed `PathNotFound` (fixed by stripping query+fragment at the `ipfs://` seam; see `docs/spikes/diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture/DIAGNOSIS.md`).

Residual, minor, NOT blocking: I attribute the observed "portfolio works, blog fails" asymmetry to symptom ordering (the initial full-page load renders the home/portfolio index.html with no client-nav `__data.json` fetch, so only the client-side nav to the blog list route triggered the buggy `__data.json` path). This was reasoned from the SvelteKit runtime source, not confirmed against a live `../ronan-eth` build on-device (that tree is not present in this isolated worktree). If a future device re-test shows portfolio ALSO failing its client-nav `__data.json` once navigated to client-side, that is consistent with this same single root cause, not a new one. Worth a one-line confirmation next time the real site is in hand.
