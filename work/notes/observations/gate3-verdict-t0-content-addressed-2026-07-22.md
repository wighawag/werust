---
title: Gate-3 (conductor) verdict — t0-content-addressed-floor-parity — APPROVE (T0 now fully reached)
date: 2026-07-22
kind: observation
reviewOf: t0-content-addressed-floor-parity
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit ca59552)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met (with the STRONGEST form of parity)

- ✅ A v0-subset fragment served via the content-addressed (`ipfs://`) path renders
  through the native T0 path.
- ✅ Parity: the rendered output is asserted BYTE-FOR-BYTE against the SAME committed
  server-floor goldens (`tests/fixtures/t0-server-floor/<name>.golden.txt`) at the
  SAME pinned viewport. So "content-addressed renders identically to served" is not a
  fresh drift-prone golden — it is literally the server floor's golden reached through
  the ipfs path. This is the correct, un-gameable definition of parity.
- ✅ Content is hash-verified on the way in (reuses `VerifyingContentFetcher` +
  `cid_v1_raw_sha256`); a mismatch does not render.
- ✅ Tests use a pinned fixture CID derived from the fragment bytes, no network.

### Nit triage

1. Verified ipfs bytes are rendered by re-wrapping them in a `data:text/html` URL and
   re-navigating the native path (not a direct ipfs navigate) — RATIFY/KEEP. This is
   the ONLY architecturally-available composition, not a shortcut: the T0
   `NativeRenderer` deliberately has NO networking/ipfs resolution (it renders only
   self-contained `data:text/html` documents — confirmed at the native-T0 review), so
   "verify via `resolve_ipfs_request`, then feed verified bytes to the T0 `data:`
   entry point" is the coherent seam composition. Flagged only for the missing
   Decisions block.
2. No `## Decisions` block in the commit — recurring benign traceability nit;
   reconstructable from code. KEEP.

### MILESTONE: T0 is now fully "reached"

Both T0 floors have landed: the SERVER floor (`t0-server-web-floor-golden-fixtures`,
#8) and this CONTENT-ADDRESSED floor. Per `docs/conformance-tiers.md`, a tier is only
"reached" when BOTH floors land — so T0 is complete, and the content-addressed path
is proven at parity with the served path for v0 content.

### What this unlocks

Leaf task (completes T0). Only `t1-content-addressed-floor-ipfs-static-site` remains
buildable on this host (mobile-ios + release are parked on the macOS wall).
