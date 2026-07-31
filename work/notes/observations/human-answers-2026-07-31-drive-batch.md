---
title: "Human answers to the 2026-07-31 drive-tasks batch (ratified; these are settled decisions)"
date: 2026-07-31
status: open
---

The conductor's `drive-tasks` run of 2026-07-31 surfaced a batch of open questions at the end of twelve merged tasks. The human answered "yes agree" to the batch, accepting each suggested default and each explicit ratify. Recorded here so later builds treat them as SETTLED and do not re-litigate them, in the same spirit as `trust-indicator-decisions-2026-07-22.md`.

## Settled, with the action taken

1. **The macOS leg's PR trigger on `crates/werust-core/**` was an UNINTENDED cost. Narrow it** to match `windows-renderer.yml` (narrow `pull_request`, wide `push`, `workflow_dispatch` for the deliberate case), and PIN it as an exact set so the next widening is an edit to a test rather than an accretion — it has drifted wider twice in three tasks. Action: added as item 5 of `macos-harness-guard-teeth-and-paint-path-residue`, which already opens that workflow.
2. **`verify` becomes `cargo clippy --all-targets`**, as its own task that clears the existing `cfg(test)` debt in the SAME change, so the gate is never knowingly red between commits. Action: cut `verify-lints-test-targets-and-clears-the-existing-debt`.
3. **`CssClassFamily` as a public enum, and `STOP_AFFORDANCE_LABEL` putting a UI affordance glyph in core: RATIFIED.** No action. The family enum earned its keep during this drive — the Windows shell bound to it without re-deriving anything, which is precisely what it was exported for.
4. **`CHROME_CSS_CLASS_SETS` is RETIRED**, not given a consumer. Action: cut `retire-the-unconsumed-chrome-css-class-sets-aggregate`, which must first decide the fate of the narrower toggling-classes meaning that constant documents (move it onto `CssClassFamily`, or record that nothing needs it) rather than deleting blind.
5. **CI-measurement tasks stay in `--merge`; the standing rule is instead "land the CI leg on `main` FIRST".** The evidence from this drive is that the leg, not the run mode, was the bottleneck: once `windows-renderer.yml` was on `main`, the very next build agent measured itself unaided. Action: written into `CONTEXT.md`'s Conventions, so it binds every future task rather than living in a report.

## Also ratified in the same answer (no action, recorded so they are not reopened)

- The **`werust resolve --json` wire break** (`kind` vocabulary, `reference` semantics, new keys, all in one release). No in-repo consumer; batching the changes was kinder than two breaks.
- **`error_banner_visible` / `error_banner_text` now mean a failure-CLASS state** (a failed load OR a changed trusted name), a vocabulary widening every edge inherits, rather than a second banner surface on four edges.
- **`werust_core::chrome_json` becoming public core**, with the hand-rolled encoder swapped for `serde_json` and JSON key order now sorted — beyond the task's stated scope, taken because adding ten fields to two hand-rolled encoders would have committed the same duplication one level down.
- **The changed-name banner staying ungated on `is_loading`** while the badge is gated: deliberate, so the warning cannot be made to flicker away by reloading.

## Still OPEN after this batch (not answered by it)

- Whether an **Apple Developer account** exists or is wanted, which is the gate on `macos-app-signing-and-notarization` (`needsAnswers: true`).
- The **`desktop` platform key meaning Linux-only** with a third desktop column queued: rename to `linux`, or pin the term in the glossary.
- Who owns the **human-on-a-Mac sweep** that six "wired but unwitnessed" macOS matrix cells defer to.
- The **`desktop-paint` crate name** and the **`com.github.wighawag.werust` bundle id**: both cheap to change now, expensive later.
- `retrieval-default-egress-before-final-release`, which carries its own `needsAnswers` block.
