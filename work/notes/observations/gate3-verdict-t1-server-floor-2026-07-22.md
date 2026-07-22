---
title: Gate-3 (conductor) verdict — t1-server-web-floor-article-and-blog — APPROVE (with orphaned line-height defect flagged)
date: 2026-07-22
kind: observation
reviewOf: t1-server-web-floor-article-and-blog
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ (merged to main, commit 348d9e0)

`do` ran Gate-1 + Gate-2, both green. Conductor diff-vs-criteria review.

### Acceptance criteria — all met

- ✅ Two INDEPENDENTLY-authored real static pages pinned as goldens: `article.html`
  (semantic-HTML content page) + `blog-post.html` (independent blog/news post), so
  the tier is not tuned to one page. Provenance in `SOURCE.md`.
- ✅ Native T1 path renders each correctly, asserted against its golden
  (`tests/t1_server_floor_goldens.rs`); the T0 goldens remain asserted too.

### FORWARD-NOTES HONOURED

My forward-notes (font-determinism + D4 block-background scope) were respected: the
goldens are stable against the bundled DejaVuSans font, and the fixtures do not rely
on filled block-container backgrounds (the D4 gap).

### Nit triage

1. Goldens pinned to a hardcoded 800px viewport (`FIXTURE_VIEWPORT_WIDTH`) governing
   inline wrapping — KEEP. Test-local, regenerable; a sensible fixed pin for
   reproducible goldens. Human-ratify traceability nit, benign.
2. **Unitless `line-height` inherits as absolute px, not a multiplier** — a REAL
   cross-task cascade defect (verified at `css.rs:826-830`; the existing test covers
   only a single element, missing the inherited-child case). Per CSS, a unitless
   `line-height: 1.5` must inherit as the MULTIPLIER (each child recomputes against
   its own font-size), not a resolved px. This task correctly avoids it in its
   fixtures (D3) and its criteria are met, BUT the defect lives in the ALREADY-LANDED
   `t1-core-css-stylo-and-latin-shaping-parley` cascade and NO task owns the fix.
   Flagged to the stuck-set (see below).

### Orphaned defect flagged to the human (stuck-set)

`t1-unitless-line-height-inherits-as-absolute-px.md` (agent-filed) is a real
correctness bug in the landed T1 cascade with no owning task. It will affect the WPT
meter (`t1-wpt-subset-regression-meter`) and real pages with inherited unitless
line-heights. Since the core-CSS task is DONE, I cannot fold the fix into it, and I do
not hand-author a fresh task (that is `to-task`/human work). RECOMMENDATION for the
human: task a small fix — store unitless `line-height` as the multiplier and inherit
it unresolved, resolving against each element's own font-size at use; add a test with
a parent/child of different font-size. Not a blocker for THIS task; surfaced in the
end-of-run batch.

### What this unlocks

Leaf-ish task; does not unlock new tasks. Contributes the T1 server-web floor half of
the T1 capability proof.
