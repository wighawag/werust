---
title: "A republish of IDENTICAL content must not warn: compare trusted CIDs by canonical form, not by spelling"
slug: trust-store-compares-cids-by-canonical-form
spec: trust-store-hardening
blockedBy: [trust-store-writes-atomically-and-keeps-fields-it-does-not-know]
covers: [10, 12]
---

## What to build

The change warning compares the blessed CID string with the current one by plain string equality. CIDs have more than one spelling for the same content: the ENSIP-7 decoder emits whatever form the contenthash carried, so a CIDv0 `Qm...` and the CIDv1 `bafy...` of the SAME root are different strings today; and the Android edge deliberately canonicalises CIDs to lowercase base32 CIDv1 for its internal `https://<cid>.ipfs.werust.invalid` origin, so the form that reaches the core is not necessarily the form the resolution produced.

The consequence is a FALSE POSITIVE: a site that republished byte-identical content under a different CID spelling, or the same site seen through a different edge, reports "this changed since you trusted it". False positives are what train a user to click through the one warning that matters, and this warning is about to become load-bearing (spec `withhold-changed-content-until-trusted` withholds content on it).

Compare on ONE canonical form. Both sides of the comparison go through the same canonicalisation, so a stored CIDv0 and a current CIDv1 of one root are equal, while two genuinely different roots stay different. The `cid` crate is already in the tree (the fetcher and the Android origin map both use it), so this is not a new dependency and not a hand-rolled multibase decoder.

Two edges of the behaviour need a stated rule, not an accident:

- **A string that does not parse as a CID at all.** The comparison must not silently treat two unparseable strings as equal, and must not panic. Decide and test it.
- **What is STORED.** Either the store keeps what it was given and canonicalises at comparison time, or it canonicalises on the way in. Pick one, say why, and make sure the choice does not make an existing record's CID unmatchable (if you canonicalise on the way in, existing records were written under the other rule).

## Acceptance criteria

- [ ] The CIDv0 and CIDv1 spellings of ONE content root compare EQUAL, so no change warning is raised for a re-encoding of identical content; asserted with real CID fixtures (derived, not pasted by hand).
- [ ] Two genuinely different roots still compare unequal, including a pair that differs only late in the string.
- [ ] A CID string that cannot be parsed is handled by a stated rule (never a panic, never two unparseable strings silently equal), covered by a test.
- [ ] Whether the store canonicalises on write or on comparison is a recorded decision, and an already-recorded pin written under the OLD rule still matches after this change (no user loses a warning to this task).
- [ ] The Android edge's lowercase-base32 canonicalisation is exercised in the comparison test as a real case (a CID in the form Android hands the core matches a pin recorded in the resolution's form).
- [ ] Tests cover the new behaviour in the existing `pins.rs` style, isolated to a scratch directory, with the real `pins.json` asserted UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-writes-atomically-and-keeps-fields-it-does-not-know`: same file (`crates/werust-core/src/pins.rs`), serialised to avoid a merge conflict. No logical dependency beyond that.

## Prompt

> Goal: stop the trusted-name change warning from firing on a REPUBLISH OF IDENTICAL CONTENT under a different CID spelling. The comparison lives in `werust_core::pins` (the mutable-name trust value's changed/unchanged predicates, which the chrome's TOFU axis and the change banner are derived from) and is currently a raw string equality of the blessed CID against the current one.
>
> Why it is wrong: the ENSIP-7 contenthash decoder yields whatever form the name's contenthash carried (a CIDv0 `Qm...` or a CIDv1 `bafy...`), and the Android edge canonicalises CIDs to lowercase base32 CIDv1 for its internal origin (`crates/werust-android/rust/src/origin_map.rs` documents exactly that), so one content root reaches the core under more than one spelling. Compare on a single canonical form using the `cid` crate that is already in the tree (see how `crates/fetcher` and the Android origin map use it) rather than hand-rolling multibase.
>
> Decide and RECORD two things: what happens to a string that does not parse as a CID (never a panic, and two unparseable strings must not silently compare equal), and whether canonicalisation happens on the way INTO the store or at comparison time. If you canonicalise on the way in, make sure a pin already recorded under the old rule still matches, because a user losing a warning to this task is the exact failure the whole spec exists to prevent.
>
> Do NOT change what the warning SAYS or when it is offered: the banner wording, the bless action's visibility and the loudest-wins posture rule are settled (`docs/adr/0006`) and other tasks own them. Do NOT make the pin steer a load: it is advisory in this wave.
>
> Test in the module's existing style: derive CID fixtures rather than pasting strings, drive the directory-taking store cores against the in-module `ScratchDir`, and keep the real-store-untouched assertion. Pure Linux gate, no display, no network.
>
> You own `crates/werust-core/src/pins.rs`. Sibling tasks from spec `trust-store-hardening` land before and after you in a strict chain; the next one re-keys the store on the ENSIP-15-normalized name and will touch the same file.
