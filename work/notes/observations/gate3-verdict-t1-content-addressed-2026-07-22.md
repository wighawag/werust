---
title: Gate-3 (conductor) verdict — t1-content-addressed-floor-ipfs-static-site — APPROVE (T1 now fully reached)
date: 2026-07-22
kind: observation
reviewOf: t1-content-addressed-floor-ipfs-static-site
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit a601250)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review. This is the
LAST buildable task on this host.

### Acceptance criteria — all met

- ✅ A real `ipfs://` static site (pinned single-block sha2-256 CID) renders via the
  native T1 path.
- ✅ Parity asserted TWO ways (stronger than the T0 sibling): (1) in-test dual-render
  byte-equality — the SAME bytes rendered via the served `data:` path vs the verified
  `ipfs://` path, asserted equal (direct parity proof); (2) against a committed
  `site.golden.txt` at the SAME pinned viewport + bundled font as the server floor,
  so a regression anywhere in parse/cascade/shaping/layout/paint turns it red.
- ✅ Content hash-verified on load (`VerifyingContentFetcher` + `cid_v1_raw_sha256`);
  a mismatch does not render.
- ✅ Tests pin the CID, isolated from the live network.

### Nit triage — all RATIFY/KEEP

1. Parity-reference divergence from the T0 sibling (T1 = in-test dual-render equality
   + new golden; T0 = reuse the server floor's own golden) — KEEP. The T1 dual-render
   equality is a STRONGER, more direct parity proof; the added golden catches
   cross-stage regressions. An improvement, not a drift. Cross-task pattern nod for
   the human.
2. Fixture is a hand-authored ORIGINAL Jekyll/Hugo-shaped page (frozen, CID-derived),
   NOT a captured live-IPFS site — KEEP. Correct hermetic interpretation: a
   single-block, offline, network-isolated fixture is exactly what "isolated from the
   live network" + the fetcher's single-block sha2-256 scope require (consistent with
   the ipfs forward-note). Documented in SOURCE.md with a re-pin path.
3. No Decisions block — recurring benign traceability nit. KEEP.

### MILESTONE: T1 is now fully "reached"

Both T1 floors have landed: the SERVER floor (`t1-server-web-floor-article-and-blog`,
#16) and this CONTENT-ADDRESSED floor. Per `docs/conformance-tiers.md`, T1 is
"reached". The thesis has "landed FIRST here": a verifiable content-addressed static
site renders as a first-class page at parity with the served path, on the pure-Rust
stack (html5ever + stylo-stack cascade + parley shaping), verify-gated end to end.

### What this unlocks

Leaf task (completes T1). This is the LAST task buildable on this Linux host; the
remaining two (mobile-ios + release) are parked on the macOS/Xcode wall.
