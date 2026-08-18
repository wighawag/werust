---
title: "One record per identity: key the trust store on the ENSIP-15-normalized name the resolution produced, and re-key what is already there"
slug: trust-store-keys-on-the-resolved-normalized-name-and-migrates
spec: trust-store-hardening
blockedBy: [trust-store-compares-cids-by-canonical-form]
covers: [6, 7, 12]
---

## Decisions (answered by the human, 2026-08-17)

These CLOSE the two questions this task launched with. They are recorded rather than deleted because they decide the store's key space, which is a security property, and because the sibling tasks behind this one inherit them.

1. **A name that FAILS ENSIP-15 normalization cannot be recorded at all.** The key becomes FALLIBLE: no key, no pin, no warning. It does NOT fall back to the old trimmed-and-lower-cased form (that is the second key space the spec forbids) and it does NOT get an explicitly marked separate key space (more machinery for a case the front door cannot produce). The grounding is that the resolution path already refuses an unnormalizable name with its typed `UnnormalizableName` error, so such a name never reaches a successful load: there is nothing to bless and therefore no warning to lose. Make the refusal legible at the call site rather than a silent `None` that a later caller mistakes for "unblessed".
2. **A non-ENS name goes through the SAME normalization, and that is a no-op on it.** No separate rule and no second key space. A bare IPNS key (`k51qzi…`, base36) is already a lowercase ASCII, dot-less label, so normalizing it returns it unchanged; assert that in a test rather than asserting it in prose, so a future normalizer change that started mangling such keys would red the gate. The existing behaviour that both ENS and IPNS names are blessable and checked identically MUST survive (there is already a test for it).

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
- [ ] The two decisions above are IMPLEMENTED as stated (an unnormalizable name yields no key and no pin, legibly; a bare IPNS key normalizes to itself, asserted by a test), tested, and recorded; the store never contains two key spaces without that being explicit.
- [ ] Tests cover the new behaviour in the existing style, isolated to a scratch directory, with the real `pins.json` asserted UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-compares-cids-by-canonical-form`: same file (`crates/werust-core/src/pins.rs`), serialised. The migration also relies on the reportable-duplicate-key and refuse-while-unreadable rules landed earlier in this chain.

## Prompt

> Goal: make the trusted-name pin store's key the RESOLVED IDENTITY, and migrate what is already recorded. The two questions this task launched with are ANSWERED, in the `## Decisions` section above: an unnormalizable name gets NO key and therefore no pin (a fallible key, never a fallback to the old ASCII fold), and a non-ENS name such as a bare IPNS key goes through the same normalization, which is a no-op on it. Implement those two rules as stated; they decide the key space, so do not re-open them.
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
