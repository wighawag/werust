---
title: "The trust store writes atomically, preserves fields it does not understand, and stops swallowing its own save failure"
slug: trust-store-writes-atomically-and-keeps-fields-it-does-not-know
spec: trust-store-hardening
blockedBy: [trust-store-fails-closed-instead-of-reading-as-nothing-trusted]
covers: [3, 4, 12]
---

## What to build

Three properties of the same file, all missing, all cheap, all invisible until the day they matter.

**1. The write is not atomic.** `save_to` is a bare whole-file write straight into the live path. A crash, an OOM kill or a full disk between truncate and flush leaves a truncated `pins.json`, which is the worst possible state for a record the browser is about to trust. Write to a temp file in the SAME directory and rename over the target, so a reader observes either the old document or the new one and never a partial one. A rename within one filesystem is the only cheap atomicity available and it is enough. Leave no stray temp file behind on the failure path, and keep the "no settings directory" case behaving exactly as it does now (a refusal, not a panic).

**2. Fields the running version does not recognise are DISCARDED on rewrite.** The document is re-serialised from the parsed struct, so an older build silently strips whatever a newer one wrote. Two werust versions are two processes and the code already contemplates that. Preserve unknown members through a read-modify-write, at both levels the document has (the top-level document and each pin entry), so an old build can never quietly delete a new build's data. This is the same property as the previous task's "stop silently shrinking" seen from the other side: one task stopped dropping entries werust cannot read, this one stops dropping fields within entries it CAN read.

**3. The save result is thrown away by its callers.** The store's save already returns whether it reached disk, and the shell's bless path returns it too, but a store that silently failed to persist is indistinguishable from one that worked. Make the outcome surfaceable, and make it carry the new refusal (the previous task's "the store is unreadable so nothing is written") as a distinguishable outcome rather than a bare false: "there was nothing to record", "I could not write it" and "I refuse to write over a store I cannot read" are three different sentences and only the last two are interesting. Do not build a user-facing surface for it here (that is out of scope and there is no trust-management UI); make it available and asserted, so the surface that eventually shows it is not blocked on a plumbing change.

## Acceptance criteria

- [ ] A save writes through a temp file in the same directory and renames it into place; a simulated failure between the temp write and the rename leaves the PREVIOUS document intact and readable, asserted by a test.
- [ ] No temp file survives a successful save, and none survives the simulated failure path (the scratch directory contains exactly what it should).
- [ ] A document carrying an unknown top-level member AND an entry carrying an unknown member both survive a read-modify-write with those members intact and their values unchanged, asserted by a test.
- [ ] Saving with no settings directory is still a refusal rather than a panic (the existing assertion keeps passing).
- [ ] The save outcome distinguishes "nothing to record", "could not persist" and "refused because the store is unreadable", and the shell's bless path propagates it rather than collapsing it; covered by a test.
- [ ] Tests cover the new behaviour in the existing `pins.rs` style (the in-module `ScratchDir`, driven through the directory-taking cores).
- [ ] Every store test stays isolated to its own scratch directory and the real `pins.json` is asserted UNTOUCHED before/after, with no process-global `WERUST_SETTINGS_DIR` mutation.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-fails-closed-instead-of-reading-as-nothing-trusted`: it rewrites the same load/save/parse surface and the same tests, and this task's write-refusal outcome is defined by its third state.

## Prompt

> Goal: make the trusted-name pin store's WRITE trustworthy: atomic, non-destructive of data it does not understand, and honest about failure. The store is `werust_core::pins` (`TrustedNamePins`, `pins.json` beside `retrieval.json` under the `WERUST_SETTINGS_DIR` lever), whose save is currently a bare whole-file write and whose JSON round-trip re-serialises only the fields this build knows.
>
> Three things, one change:
>
> 1. Temp-file-plus-rename in the SAME directory (cross-directory rename is not atomic), so a reader sees the old document or the new one, never a truncated one. Clean up on the failure path. Keep the existing no-settings-directory refusal behaviour.
> 2. Preserve unknown members across a read-modify-write, at the document level and the per-entry level, so an OLDER build cannot strip what a NEWER one wrote (two versions are two processes, which this module's docs already contemplate). Do not change the wire spelling of the fields werust does own: the posture vocabulary is shared with the chrome JSON and the debug view.
> 3. Make the save OUTCOME surfaceable and distinguishable: nothing-to-record versus could-not-persist versus refused-because-unreadable (the third comes from the sibling task that landed the store's third state). Propagate it through the shell's bless path instead of collapsing it to a bare boolean. Do NOT build a user-facing surface: there is no trust-management UI and adding one is out of scope for this spec.
>
> Test at the directory-taking `load_from`/`save_to` seams against the in-module `ScratchDir`, and keep the real-store-untouched snapshot assertion. Simulating the mid-write failure is easier than it looks: exercise the temp-then-rename steps through a seam you can interrupt (a fault-injectable write step, or performing the temp write and then asserting the target is untouched before the rename). Prefer the shape that keeps production code free of test-only branching; this repo has exactly one `cfg!(test)` branch in production code and its review flagged it as a precedent not to widen.
>
> You own `crates/werust-core/src/pins.rs` plus, if the save outcome needs it, the shell's pin-store location and bless path in `crates/werust-core/src/lib.rs` (the file's worst collision point: touch only that region). Four sibling tasks from spec `trust-store-hardening` follow in a strict chain (canonical CID comparison, normalized-name keying and migration, the normalization corpus and stamp, the advisory lock). Note that `work/tasks/backlog/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns.md` is an unpromoted task touching the same region; if it has landed, rebase onto it.
>
> RECORD the atomicity mechanism and the outcome taxonomy where a reviewer will find them (a module doc at the site plus a `## Decisions` block in the done record; an ADR if it meets that gate).
