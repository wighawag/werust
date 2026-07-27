---
title: "Review verdict: in-app-debug-menu-console-and-network spec + its 5 tasks (BLOCK->fixed->APPROVE), moved proposed->tasked"
date: 2026-07-26
status: open
kind: observation
reviewOf: in-app-debug-menu-console-and-network
---

## Applied the review protocol (work/protocol/REVIEW-PROTOCOL.md) to the spec + its 5 derived tasks

Adversarial, verified-against-the-code pass across the lenses, ending in the destination check.

- **Lens 1 (claim-vs-reality)** — PASS. Every load-bearing spec claim checked against origin/main: no console capture exists (0 hits), desktop has no resource-load signal (only registered ipfs://` /werust:// schemes), Android `shouldInterceptRequest` present, no menu on any platform, `CARGO_PKG_VERSION` is the version source, ADR-0006 is the trust-posture ADR. All held.
- **Lens 2 (cleanup-vs-behaviour)** — n/a (no removals; additive feature).
- **Lens 3 (cross-artifact composition / contract)** — one NON-BLOCKING finding: the `capture` and `menu` tasks both edit `BrowserActivity.kt` (and `menu`+`desktop-view` both edit `main.rs`, already serialised by blockedBy). The blockedBy graph does not express the capture/menu shared-file ordering, but the spec's PRACTICAL ORDER (store -> menu -> capture -> desktop view -> mobile view) serialises it, and this conductor drives STRICTLY SEQUENTIAL (one task lands before the next), so the merge-conflict risk is fully mitigated by ordering. Left the graph honest (no false dep) rather than add a spurious blockedBy. Frontmatter otherwise conforms: content-derived slugs, camelCase, real `spec`/`blockedBy` slugs, `## Prompt` self-contained with drift-checks.
- **Lens 4 (conceptual coherence)** — PASS. The spec reuses "trust posture" per ADR-0006 (does not re-mean it; the Network tab uses the trust-indicator vocabulary), uses the existing `notes/observations` + `specs/` buckets correctly, and "general menu" / "debug view" / "capture store" are new concepts that do not clash with the CONTEXT.md glossary.
- **Lens 5 (destination check)** — one BLOCKING finding, now FIXED. All 5 tasks originally carried `covers: [2]` (a copy-paste artifact from the earlier ENS-spec field-fix tasks), but they deliver DIFFERENT stories, so "every story covered exactly once" could not be verified and story 1 (the payoff) appeared uncovered while story 2 looked quadruple-covered. CORRECTED: store -> [5,6], menu -> [2], capture -> [4,5], desktop-view -> [1,3], mobile-view -> [1,3]. Re-audited: all 6 stories covered, none orphaned; stories 1 & 3 covered by BOTH views is legitimate (desktop+mobile split the same stories by platform), story 5 by store+capture is correct (store IS the shared store, capture FEEDS it).

## Verdict: APPROVE (after fixing the covers map)

The blocking `covers` defect is fixed; the non-blocking composition ordering is mitigated by the spec's practical order + sequential driving. The spec is taskable and its decomposition provably reaches the spec goal. Moved `work/specs/proposed/in-app-debug-menu-console-and-network.md` -> `work/specs/tasked/`.
