---
title: "One record per identity: key the trust store on the ENSIP-15-normalized name the resolution produced, and re-key what is already there"
slug: trust-store-keys-on-the-resolved-normalized-name-and-migrates
spec: trust-store-hardening
needsAnswers: true
blockedBy: [trust-store-compares-cids-by-canonical-form]
covers: [6, 7, 12]
---

## Open questions

1. **What is the key for a name that FAILS ENSIP-15 normalization?** The key function is public and infallible today, and the resolution path already has a typed `UnnormalizableName` refusal, so such a name never reaches a successful load. Candidate rules: (a) the key becomes fallible and an unnormalizable name simply cannot be recorded (nothing is blessed, nothing is warned); (b) it falls back to the trimmed/lower-cased form, which is a SECOND key space inside one store and is what the spec forbids doing accidentally; (c) it is recorded under an explicitly marked non-normalized key space. Which rule ships?
2. **What is the key for a name that is not an ENS label at all?** A bare IPNS key (`k51qzi...`) is blessable today and is not an ENSIP-15 label. Does it go through the same normalization (it is plain ASCII, so it survives it), or is it keyed by a stated separate rule? The answer must not create an unmarked second key space, and it must keep the existing "both ENS and IPNS names are blessable and checked the same way" behaviour.

## What to build

The store's key is `trim().to_lowercase()`: ASCII case folding. Name resolution, meanwhile, normalizes through ENSIP-15 (the bound `ens-normalize` crate, inside the namehash computation) and THROWS THE NORMALIZED STRING AWAY. So for any name whose normalization is not plain case folding (emoji with and without the U+FE0F variation selector, fullwidth or circled Latin, other confusables) ONE resolved identity produces TWO store keys. The module's own doc calls a casing split "the one failure mode a TOFU store cannot have", and its guard covers ASCII only.

Two halves, one vertical change:

**1. Surface the normalized name from the resolution path.** The namehash already computes it; the resolution result must carry it out instead of discarding it, so the key and the resolved identity CANNOT diverge. Re-deriving it at the store layer is explicitly not the fix: that is a second call site of a library whose output is the identity, and the two would drift. The shell then keys the store on what the resolution returned, for both the check-on-load and the bless.

**2. Re-key the records that already exist.** A non-ASCII record written under the old fold would MISS after the change, and the caller would then record a SECOND pin for a name the user already trusts, which is the same missed-warning failure the spec exists to close. So the migration is part of this task, not an afterthought: an existing store is re-keyed once, in place, and afterwards ONE lookup finds the record with no second entry created. Re-keying two old records onto the SAME new key is possible (that is the bug being fixed) and needs a stated collision rule that never silently picks a winner (the sibling task that landed the store's third state made duplicate keys reportable: reuse that, do not invent a second answer).

The migration must be safe under the store's other new rules: it is a write, so it obeys the refuse-while-unreadable rule; it goes through the atomic write; and it must be idempotent (a second launch re-keys nothing).

## Acceptance criteria

- [ ] The resolution path RETURNS the ENSIP-15-normalized name (immutable and mutable cases alike), and the shell keys the trust store on that value rather than re-deriving or case-folding it; asserted by a test that a name whose normalization is not case folding is looked up under exactly one key.
- [ ] A non-ASCII name typed in two spellings that normalize to the SAME identity resolves to ONE record: blessing under one spelling makes the other spelling report unchanged, with no second record created.
- [ ] A store written under the OLD ASCII fold containing a non-ASCII name is re-keyed on load, found by ONE lookup afterwards, and no second record is created by the first bless after the upgrade.
- [ ] Re-keying is idempotent (running it twice changes nothing) and obeys the store's write rules: it does not write while the store is unreadable, and it goes through the atomic write.
- [ ] Two old records that re-key onto one new key are REPORTED by the store's existing duplicate-key rule, never silently resolved to one of them.
- [ ] Existing behaviour is preserved: `Ronan.ETH` and ` ronan.eth ` still resolve to one record, and both ENS and IPNS-style names stay blessable and checked identically.
- [ ] The rule for a name that fails normalization, and for a name that is not an ENS label, is IMPLEMENTED as answered above, tested, and recorded; the store never contains two key spaces without that being explicit.
- [ ] Tests cover the new behaviour in the existing style, isolated to a scratch directory, with the real `pins.json` asserted UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-compares-cids-by-canonical-form`: same file (`crates/werust-core/src/pins.rs`), serialised. The migration also relies on the reportable-duplicate-key and refuse-while-unreadable rules landed earlier in this chain.

## Prompt

> Goal: make the trusted-name pin store's key the RESOLVED IDENTITY, and migrate what is already recorded. Answer the two open questions above (they are in the task frontmatter as `needsAnswers`) before building: they decide the key space, which is a security-relevant choice, not an implementation detail.
>
> Today `pin_key` is `trim().to_lowercase()` in `werust_core::pins`, while `werust_core::ens`'s namehash normalizes the name through the bound `ens-normalize` crate (ENSIP-15) and discards the result; `werust_core::name_resolution`'s resolved-name value carries the URI, the CID and the followed mutable pointer but NOT the normalized name. Surface it there (both the immutable `ipfs-ns` and the followed mutable `ipns-ns` cases) and let the shell key the store on it. Do NOT re-derive normalization at the store layer: the point is that the key and the resolved identity cannot diverge, and a second call site is a second chance to drift. The headless `werust resolve` CLI and the GUI share that one resolution path: keep them sharing it.
>
> Then migrate. A non-ASCII record written under the old ASCII fold will MISS after the change and the next bless will write a SECOND record for a name the user already trusts, which is precisely the missed-warning failure this spec exists to close. Re-key once, in place, idempotently, honouring the store's new write rules (refuse while unreadable; atomic write). If two old records collapse onto one new key, use the store's existing reportable-duplicate-key outcome rather than picking a winner.
>
> Constraints: keep `Ronan.ETH` and ` ronan.eth ` collapsing to one record (an existing test); keep both ENS and bare-IPNS names blessable and checked identically (an existing test); the pin stays ADVISORY in this wave (it steers no load and upgrades no posture, `docs/adr/0006`); the posture wire vocabulary is shared, do not mint a spelling.
>
> The very next task in this chain pins normalization STABILITY (a fixed non-ASCII corpus plus a stamp recording which normalization version wrote the store), because deriving the key from a library means a version bump re-keys records. Do not build that here, but do not make it harder: leave the key derivation in one place.
>
> You own `crates/werust-core/src/pins.rs`, `crates/werust-core/src/ens.rs`, `crates/werust-core/src/name_resolution.rs`, and the pin-store/bless region of `crates/werust-core/src/lib.rs` (8000+ lines, this repo's worst collision point: stay in that region). Test on the pure Linux gate with the module's `ScratchDir` and the real-store-untouched snapshot; the resolution tests already run the whole ENS path off the network with scripted providers and real key fixtures, so extend those rather than reaching for the network.
