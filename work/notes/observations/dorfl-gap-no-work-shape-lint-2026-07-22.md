---
title: dorfl gap — no lint catches non-contract work/ shape (orphan top-level files, status:-frontmatter); proposal for a work/-shape lint
date: 2026-07-22
kind: observation
tags: [dorfl, work-contract, lint, upstream-proposal]
source: "read ../dorfl @ 1f37af5f: skills/setup/protocol/WORK-CONTRACT.md, packages/dorfl/src/{work-layout,ledger-lint}.ts"
---

## What happened (the mistake this analyses)

While brainstorming werust specs I created `work/ROADMAP.md` (an orphan top-level file NOT
in the `work/` contract's five sanctioned surfaces) and put `status: proposed` frontmatter on
specs (violating "status = the FOLDER, never a frontmatter field"). Both are real contract
violations; nothing in dorfl flagged them. A human caught it.

## Root cause: the rules EXIST but are unenforced

The `work/` contract DOES state both rules \u2014 WORK-CONTRACT.md line ~97 ("status = the folder,
never a frontmatter field") and line ~11 (the five top-level surfaces are enumerated:
notes/ tasks/ specs/ questions/ protocol/). And dorfl KNOWS the canonical legal set:
`packages/dorfl/src/work-layout.ts` `WORK_FOLDER_NAME` exhaustively defines every legal
folder. But the rules are buried in a long, dense contract, and dorfl has NO check that the
tree conforms:

- `ledger-lint.ts` exists but only checks ONE invariant: one-slug-one-folder (a slug present
  in >1 status folder). It does NOT check for unexpected top-level entries or bad frontmatter.
- So an orphan `work/ROADMAP.md`, or a stray `work/foo/`, or `status:`/other non-contract
  frontmatter on a spec/task, is INVISIBLE to `status`/`scan`/any validation.

The gap: dorfl has the canonical shape (`WORK_FOLDER_NAME`) and a lint FRAMEWORK
(`ledger-lint` + its `status`/`scan`/`gc --ledger` surfacing) but never asserts "the `work/`
tree matches the contract's shape".

## Proposal: extend the ledger lint to a work/-SHAPE lint (warn, never auto-fix)

Same posture as the existing duplicate lint (WARN in `status`/`scan` + report in `gc`; a
human fixes; never auto-delete). Add checks derived from `WORK_FOLDER_NAME` + the frontmatter
contract:

1. **Unexpected top-level `work/` entry.** List `work/` and flag any entry that is NOT one of
   the sanctioned surfaces (`notes/`, `tasks/`, `specs/`, `questions/`, `protocol/`) \u2014 catches
   an orphan `ROADMAP.md`, a stray folder, a misplaced file. (A `<slug>/` asset sidecar under
   notes/ is already legal per rule 8; keep that carve-out.)
2. **Non-contract frontmatter on a work item.** For a spec/task `.md`, flag frontmatter keys
   NOT in the contract's allowed set (esp. a `status:` key \u2014 the exact mistake \u2014 since status
   IS the folder). Warn, do not strip (matches "silent-on-malformed" only for the wire
   grammar; here a human should see it).
3. (Optional) **Item in a folder whose frontmatter contradicts residence** \u2014 e.g. a body
   claiming a status the folder disagrees with.

Surfacing: fold into the existing `status`/`scan` ledger-warning block and `gc --ledger`, so
an agent or human sees "work/ shape: 1 unexpected top-level entry (ROADMAP.md); 5 specs carry
a non-contract `status:` field" without a new command to remember.

## Bonus: a cheaper, complementary guardrail

Even before a lint lands, the contract could add a short, PROMINENT "what does NOT go in
work/" callout near the top (the rules are there but easy to miss in the density): "Do NOT
add top-level files to work/ (only the five surfaces); a cross-spec roadmap/ordering is
`taskedAfter` on the specs + an ADR, NOT a work/ file. Do NOT put `status:` (or any
lifecycle-status) in frontmatter \u2014 status is the folder." A blunt DON'T list is what an agent
skims; the current phrasing states the positive rule but never the tempting anti-pattern.

## Why this matters

The whole `work/` design is conflict-safe BECAUSE the shape is constrained; an un-linted tree
lets that erode silently (orphan files that drift, frontmatter that shadows folder-status).
dorfl already enforces one-slug-one-folder on WRITE (integration-core) + lints it on READ; a
work/-shape lint is the same idea one level up \u2014 assert the tree IS the contract's shape. Low
risk (warn-only), reuses the lint surface, and directly prevents the class of mistake made
here.

(NOTE: not filed into ../dorfl directly \u2014 that repo had uncommitted maintainer work in its
tree at the time, and the AGENTS.md clean-tree rule bars adding to it. Captured here in werust
as an upstream proposal for the maintainer to lift into dorfl's own work/.)
