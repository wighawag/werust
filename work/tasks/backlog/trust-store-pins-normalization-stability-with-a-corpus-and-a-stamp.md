---
title: "A normalization-library bump must RED the gate, not silently re-key the trust store: a fixed corpus plus a recorded normalization version"
slug: trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp
spec: trust-store-hardening
blockedBy: [trust-store-keys-on-the-resolved-normalized-name-and-migrates]
covers: [8, 12]
---

## What to build

Once the store's key is derived from a normalization LIBRARY (the previous task in this chain), a version bump of that library that changes any mapping silently re-keys every affected record: the user's trusted names quietly stop being found, and the next visit records fresh pins for names they already trusted. A dependency bump would be a trust reset, and nothing would say so.

Two mechanisms, deliberately both:

**1. A fixed non-ASCII corpus test that pins the mapping.** A checked-in table of inputs and their expected normalized outputs, chosen to cover the classes that actually bite: emoji with and without the U+FE0F variation selector, fullwidth and circled Latin, mixed-script confusables, a name that differs from another only by an invisible character, plus a couple of plain ASCII controls so the table also documents the boring case. The corpus is the TRIGGER: bumping `ens-normalize` without updating the corpus reds the gate, which is exactly the reviewable moment the bump needs. Say so in the test's own documentation, including what a maintainer should DO when it reds (verify the change is intended, then update the corpus and the stamp in the same deliberate change, and consider whether stored records need re-keying).

**2. The store records which normalization version wrote it.** A stamp in the document, so a change is DETECTABLE from the file rather than inferred. It rides the unknown-field-preserving round trip that landed earlier in this chain (an older build must not strip it), and its absence means "written before stamping existed" rather than an error, so an existing store is not invalidated by this task.

Keep the two honest about what they claim: the corpus proves the mapping is what it was when the corpus was written; the stamp proves which version last wrote the file. Neither claims the library is correct, and neither is a migration. What a stamp MISMATCH should cause is a stated decision (report, re-key, or nothing yet), and the honest minimum for this task is that it is recorded and readable rather than acted on silently.

## Acceptance criteria

- [ ] A checked-in corpus test asserts input to normalized output for a fixed non-ASCII set covering at least: an emoji with and without U+FE0F, fullwidth Latin, circled Latin, a mixed-script confusable, an invisible-character pair, and plain ASCII controls.
- [ ] The corpus test's documentation states that it is the dependency-bump tripwire and what a maintainer does when it reds.
- [ ] Deliberately altering one expected value reds the gate (demonstrated while building, and stated in the done record), so the corpus is proven to have teeth rather than assumed to.
- [ ] The persisted document records the normalization version that wrote it; a store with no stamp still loads (it predates stamping) and is not treated as corrupt.
- [ ] The stamp survives a read-modify-write by a build that does not know it, riding the unknown-field preservation from earlier in this chain; asserted by a test.
- [ ] What a stamp MISMATCH means is a stated, recorded decision, and the store's behaviour matches that statement (nothing silent).
- [ ] Tests cover the new behaviour in the existing style, isolated to a scratch directory, with the real `pins.json` asserted UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-keys-on-the-resolved-normalized-name-and-migrates`: there is nothing to stabilise until the key IS the normalized name, and it edits the same file.

## Prompt

> Goal: turn a normalization-library upgrade into a RED GATE instead of a silent trust reset. The trusted-name pin store (`werust_core::pins`) now keys on the ENSIP-15-normalized name produced by the bound `ens-normalize` crate (pinned in `crates/werust-core/Cargo.toml`), so any mapping change in that crate re-keys stored records.
>
> Build two things. First, a FIXED non-ASCII corpus test: a checked-in table of inputs and expected normalized outputs covering emoji with and without U+FE0F, fullwidth and circled Latin, a mixed-script confusable, an invisible-character pair, and a couple of ASCII controls. Document IN the test that it exists as the dependency-bump tripwire and what a maintainer does when it reds (confirm the change is intended, update corpus plus stamp in one deliberate change, decide whether stored records need re-keying). This repo already treats "a property of text in a declarative file" as something to assert by parsing rather than trusting (see `crates/werust-core/tests/toolchain_pin_shape.rs`, `verify_gate_shape.rs`): the corpus is the same discipline applied to a library's output. Prove it has teeth by flipping one expected value locally, watching the gate red, and putting it back.
>
> Second, stamp the STORE with the normalization version that wrote it, so a change is detectable from the file. A store with no stamp must still load (it predates stamping), and the stamp must survive a rewrite by a build that does not know about it, which the unknown-field preservation earlier in this chain already provides. State what a MISMATCH means and make the code match the statement: recorded and readable is an acceptable answer for this task, silently acting on it is not.
>
> Do not migrate anything here (the previous task owns the one-time re-key) and do not bump `ens-normalize` as part of this task: the point is to make the NEXT bump loud.
>
> You own `crates/werust-core/src/pins.rs` and the new corpus test. Test on the pure Linux gate with the module's `ScratchDir` and the real-store-untouched snapshot. The last task in this chain adds the advisory lock around read-modify-write and will touch the same file after you.
