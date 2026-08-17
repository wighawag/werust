---
title: "Retire the explicit bless affordance, expand-migrate-contract, so exactly one surface reports a changed name"
slug: retire-the-bless-affordance
taskedAfter: [withhold-changed-content-until-trusted]
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.

## Problem Statement

Once `withhold-changed-content-until-trusted` records first use automatically and handles a change with a modal, the explicit bless control has no job left. Leaving it costs more than the code:

- **Two surfaces fire for one event.** The banner's visibility rule ORs in the changed-name condition, so a changed name can raise the retired banner alongside the modal.
- **The dead control is the one that failed.** The bless action lives in a trust-indicator popover reported to open on a first visit but not once the name had changed — i.e. failing in exactly the security-critical state. It should leave the security path rather than be debugged.
- **Only THREE edges have anything to remove.** GTK, Android and iOS carry the bless button; macOS and Windows have never had a trust surface at all (zero references to the derivations in either crate, and both cells recorded as pending in the capability matrix). So this is a three-step migrate, and the two remaining edges need their matrix cells repointed rather than a migrate step that would be empty.
- **Its removal is not a single edit.** `trust_pin_action_visible`, `trust_pin_action_label`, `trust_pin_detail` and `bless_current_name` are referenced from a dozen files, four of which compile on the Linux gate (`crates/werust/src/main.rs`, `crates/desktop-paint/src/lib.rs`, and both mobile Rust crates). Deleting the core derivations in one task reds `cargo build` on four crates at once. Shape tests assert the button EXISTS, and the capability matrix carries a `mutable-name-tofu-bless` row whose description reads "from an EXPLICIT action reached off the trust indicator (never a first-visit prompt)" — policed by a parity guard.

Two live backlog items also BUILD what this retires: `macos-trust-surface-bless-affordance` and `windows-trust-surface-bless-affordance`, each named as the resolving task by a `stubbed` matrix cell.

## Solution

### The affordance goes; the trust SURFACE stays

The **read-only posture explanation** is still valuable and is still MISSING on macOS and Windows — those edges have no trust surface at all. So this is a retirement of one ACTION, not of the surface that hosted it. The two backlog items are **superseded, not cancelled outright**: their surviving half (deliver the posture surface, plus the new decision modal) moves into `withhold-changed-content-until-trusted`'s per-edge work, and this spec owns repointing the matrix cells that name them so the parity guard stays green.

### Expand, migrate, contract

The removal is sequenced so every step is green on its own:

1. **Expand** — the structured decision detail lands beside the existing bless derivations (owned by the withholding spec). Both exist at once.
2. **Migrate** — one task per edge THAT HAS A BUTTON (GTK, Android, iOS) removes it AND inverts that edge's shape assertion in the same change, so the gate never sits red between them.
3. **Contract** — only once those three have migrated, the core derivations, the FFI verbs and the banner's changed-name branch are deleted.

A single "retire the bless action" task cannot be green alone, which is exactly the hazard the tasking protocol's wide-refactor rule exists to catch.

### The matrix, handled without redding the guard

The new decision modal's row is NOT created here — the withholding spec creates it, because that is where the per-edge tasks that flip its cells live. By the time this spec is tasked those tasks are finished and could flip nothing.

What this spec owns is the retired row: the existing bless capability is re-worded or retired in the contract step, never left describing a control that no longer exists. Its macOS and Windows cells currently name the two superseded backlog items, and the parity guard resolves such a cell only against the active task folders — NOT the terminal one. So if those items are moved to a terminal folder before the cells are repointed, `main` goes red on the pure-Rust gate. The repoint and the disposition are therefore ordered explicitly, and the repoint happens first.

## User Stories

1. As a user, I want exactly ONE surface to tell me a name changed, so that I am not shown a modal and a stale banner for the same event.
2. As a werust developer, I want each edge's button removal and its shape assertion changed together, so that the gate is never red between two tasks.
3. As a werust developer, I want the core derivations deleted only after every edge that had a button has stopped calling them, so that no single task reds the build on four crates at once.
4. As a reader of the capability matrix, I want no row describing a control that no longer exists, so that the matrix does not document a fiction.
5. As a maintainer, I want the retired row's pending cells repointed BEFORE the tasks they name leave the active folders, so that the parity guard never reds `main`.

## Implementation Decisions

1. **Expand -> migrate -> contract, in that order, with the contract step `blockedBy` all THREE migrate steps.** The compile fan-out reaches four gate-compiled crates, so a hard swap cannot compile alone.

2. **Three migrate steps, not five.** macOS and Windows have no button and no assertion to invert; they get a cell repoint, which is a different job.

3. **Each migrate step owns its edge's shape assertion.** Inverting the assertion in the same change as the removal is what keeps each step green. Note the mobile guard asserts BOTH the Kotlin and the Swift shape in ONE file, so the Android and iOS steps must be serialised against each other for conflict reasons even though they have no logical dependency.

4. **The two backlog items are SUPERSEDED, and their surviving half is delivered by the withholding spec, which says so explicitly.** Their posture-surface half is real work; it is now owned by that spec's macOS and Windows per-edge tasks (its story 15 and decision 10), so it is not orphaned by retiring them here. This spec records the disposition and repoints the cells; the `git mv` to a terminal folder is the runner's transition, and the repoint must precede it.

5. **The banner's changed-name branch is deleted in the contract step.** The banner remains for load FAILURES, which is a different surface for a different condition. Story 1 is not satisfied while the OR is still in the rule, so the assertion for it belongs in that step.

6. **The read-only posture explanation is retained everywhere it exists, and is BUILT on macOS and Windows by the withholding spec.** This spec removes an action, not an explanation. Whatever remains of the popover after the button goes is no longer on any security path, so a residual defect in it is cosmetic.

## Testing Decisions

- **Per-edge migrate steps** are proven by their own inverted shape assertions: the button is absent, the modal is wired.
- **The contract step** is proven by the core derivations being gone with `cargo build` green across all gate-compiled crates, and by an assertion that the banner no longer fires on a changed name.
- **The matrix** is proven by the existing parity guard staying green at each step, which is the property the create-once-then-flip-one-cell sequence exists to preserve.
- **Per-edge legs**: GTK is gate-compiled; Android names the instrumented emulator leg; iOS is build-verified on `mobile-ios.yml`. macOS and Windows have no migrate step, so they name no leg here.

## Out of Scope

- **The withholding, the modal, the auto-record, AND the macOS/Windows posture surface** — all `withhold-changed-content-until-trusted`, which must land first.
- **Creating the decision modal's capability row** — also that spec, for the ordering reason above.
- **Diagnosing the popover's failure to open.** The retirement removes it from the security path; that was the point.
- **A trust-management screen.** The narrow forget-this-name action lives in the withhold spec.
