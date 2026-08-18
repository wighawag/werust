# Decisions: the normalization corpus and the store's normalization stamp (`trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`)

The fifth task of spec `trust-store-hardening`, after the third state (`docs/adr/0014`), the atomic write (`docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md`), the canonical-CID comparison (`docs/spikes/trust-store-compares-cids-by-canonical-form/DECISIONS.md`) and the normalized key (`docs/spikes/trust-store-keys-on-the-resolved-normalized-name-and-migrates/DECISIONS.md`). That one made the store's key the output of a LIBRARY; this one makes a change to that library LOUD instead of a silent trust reset, and records in the file which normalization wrote it.

Nothing here migrates anything and nothing here bumps `ens-normalize`: the point is to make the NEXT bump a red gate and a reviewable moment. What follows is one entry per choice the build had to make, so a reviewer can ratify or reverse each.

Task: `work/tasks/*/trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp.md`. Code: `crates/werust-core/src/pins.rs`, `crates/werust-core/tests/normalization_corpus.rs`.

## 1. The stamp names a LIBRARY and its LOCKED version, not an ENSIP-15 revision

**Chosen.** `pins::NORMALIZATION_VERSION` is `"ens-normalize 0.1.1"`, written into every saved document as the top-level `normalizationVersion` member. `crates/werust-core/tests/normalization_corpus.rs` asserts it against the version `Cargo.lock` resolves for `ens-normalize`, and asserts that `crates/werust-core/Cargo.toml`'s requirement names that same version.

**Why.** The question the stamp answers is "which code produced the keys in this file?", and that is a crate at a version: two crates claiming the same ENSIP-15 revision can still disagree about a mapping, and a spec revision is not something the build can check. The LOCK rather than the manifest is what links (`"0.1.1"` in a manifest is `^0.1.1`, so a routine `cargo update` could move every store key without a manifest diff to read), and holding the manifest requirement equal to the locked version keeps a bump a one-file, reviewable edit.

**Rejected.** A stamp derived at build time from cargo metadata (there is no cargo env var for a dependency's version; a `build.rs` or a metadata call would be new build machinery for a string that must be reviewed by a human anyway). A monotonically-bumped werust-owned schema number (it says nothing about which normalization ran). A hash of the corpus outputs (unreadable in a file a human is meant to inspect, and it would change for reasons the corpus test already reports better).

**Touches.** Bumping `ens-normalize` is now a THREE-part single change: the manifest/lock, `NORMALIZATION_VERSION`, and any corpus row whose mapping moved. Both halves are asserted, so forgetting one reds the gate. The constant lives in `pins` rather than `ens`, because the store is the only thing that records it and `pins` owns the wire form; `ens` still owns the one normalizer call site.

## 2. What a MISMATCH means: recorded and readable, acted on by nothing

**Chosen.** `TrustedNamePins::normalization_version()` reports what the document said (`None` = written before stamping existed), and `normalization_version_mismatch()` reports it when it is not this build's. Neither changes any behaviour: a mismatched stamp does not make the store `UndeterminableTrust`, does not refuse a write, does not re-key, and does not change what the chrome says. The statement is in the module docs and asserted by `a_mismatched_stamp_is_recorded_and_readable_and_changes_nothing_else`.

**Why.** The task's honest minimum, and the ceiling on what this task can justify. Acting on a mismatch means re-keying or withholding trust, and both are exactly the silent trust reset the corpus exists to prevent, decided by a file the user cannot see. Reporting it needs a trust-management surface, which does not exist (the same dead end every other store fact reaches today). Meanwhile the read-time re-key from the previous task already handles the case that actually matters — an old key folded onto today's derivation for every name this build can normalize — so a mismatch is information for a maintainer, not an input to a decision.

**Rejected.** Making a mismatch `UndeterminableTrust` (a dependency bump would then fail-close every user's whole store, in the one direction the module's own doc calls a silent re-trust from the write side, and it would fire on the FIRST launch after any bump, correct or not). Re-keying every record on a mismatch (the migration this task is explicitly not). Ignoring the stamp on read (then the file records something nothing can read, which is a comment, not a mechanism).

**Touches.** `withhold-changed-content-until-trusted` and any future re-key inherit a readable fact instead of a guess. If a later task decides a mismatch must DO something, this is the entry it reverses, and the statement it must update in the same change.

## 3. A `normalizationVersion` that is not a string is `Unparseable`

**Chosen.** A document whose stamp member is present but not a JSON string is reported as `UndeterminableTrust::Unparseable` (naming the member), so the store is undeterminable and the write refuses. This is a NEW refusal path, hence this entry.

**Why.** `normalizationVersion` is now a member werust OWNS, so the next save would overwrite whatever is there. Overwriting a value this build could not read is silent loss, and the module's whole read rule is "report, never silently shrink"; the existing precedent is the `pins` member, whose wrong TYPE is already `Unparseable` rather than an empty store. Refusing also keeps the odd bytes on disk for whoever looks. It is unreachable from any build that exists: werust extends the document with NEW members, it does not re-type existing ones.

**Rejected.** Ignoring a non-string stamp and replacing it on the next save (silent, and it destroys the one clue about what wrote the file). Carrying it as an unknown member (it is not unknown; and it would then be shadowed by this build's own stamp on the way out, so it would be lost anyway).

**Touches.** Any future build that wants a richer stamp (an object with crate + revision) must introduce it as a NEW member, not by re-typing this one — or it makes this build refuse to read the store.

## 4. The stamp is a known DOCUMENT member with its own field, so store equality changed

**Chosen.** `normalizationVersion` joined `pins` in `KNOWN_DOCUMENT_MEMBERS` and is read into a `TrustedNamePins` field, rather than being left to the unknown-member carrier. `TrustedNamePins`'s derived equality therefore includes it, and the module's tests compare a saved-then-reloaded store through one helper, `as_read_back`, which adds the stamp this build's save writes.

**Why.** A carried member cannot be READ (the carrier is opaque by design) and would be shadowed by the stamp the save writes anyway, so the "readable" half of the task would not exist. Keeping equality honest — a document that now records something it did not record before is NOT the same document — is what keeps the round-trip tests able to notice a save that failed to stamp; dropping the field out of `PartialEq` would have made four assertions pass for the wrong reason.

**Touches.** `trust-store-serialises-read-modify-write-so-no-bless-is-lost` edits this same file next: a test there that compares an in-memory store to one read back off disk needs `as_read_back` (or its own expectation of the stamp), exactly as the four existing ones now do.

## 5. The corpus asserts through `pin_key`, pins REFUSALS as well as mappings, and reports the whole drift at once

**Chosen.** `crates/werust-core/tests/normalization_corpus.rs` is a checked-in table of `(input, expected)` where `expected` is either the store KEY the input must produce or `Refused`, asserted through `pins::pin_key` (not `ens::normalize_name`), collecting every mismatching row before it fails. A second test asserts the RELATIONS between rows (one identity is one key; a Cyrillic homograph is not the Latin name). Refusal rows deliberately do not pin the normalizer's wording.

**Why.** `pin_key` is the thing whose stability the store depends on (it is `normalize_name` plus the store's trim and its empty-name rule), so asserting it covers the derivation end to end. Refusals are half the security-relevant behaviour — a mixed-script confusable and a bare ZWJ/ZWNJ get NO key today, and a version that started ACCEPTING one would create keys where there were none — so a corpus of accepted mappings only would miss the more dangerous direction. Reporting all drift at once is what a maintainer meeting this after a bump actually needs. Wording is not pinned because a reworded error message is noise, not a mapping change.

Two facts the corpus pins that are worth calling out, because they are invisible in an editor: U+200B ZERO WIDTH SPACE and U+00AD SOFT HYPHEN are IGNORED (an invisible variant keys onto the SAME record, which is the outcome a TOFU store wants), while U+200C/U+200D outside the sequences that allow them are REFUSED.

**Teeth, demonstrated (not assumed).** Flipping the expected key of `"\u{2764}\u{FE0F}.eth"` to keep the variation selector reds `the_normalization_corpus_still_maps_every_fixed_input_to_its_recorded_key` with the full drift report and the maintainer procedure; flipping `NORMALIZATION_VERSION` to `"ens-normalize 0.1.2"` reds `the_recorded_normalization_version_names_the_library_that_is_actually_linked` against the lockfile. Both were flipped locally, observed red, and put back.

**Rejected.** A corpus inside `pins.rs`'s test module (it is a dependency tripwire a maintainer looks for by file name, and this repo already keeps its declarative-file guards in `crates/werust-core/tests/*_shape.rs`). Generating the table from the library at test time (that asserts nothing — it can only ever agree with itself).

## 6. "An older build preserves the stamp" is asserted as a MECHANISM, and says so

**Chosen.** `a_stamp_survives_a_rewrite_by_a_build_that_does_not_know_it` asserts the two halves the claim rests on: that `KNOWN_DOCUMENT_MEMBERS` is exactly `["pins", "normalizationVersion"]` (so every build before this change knew only `pins`, and the stamp is unknown to all of them), and that an unknown top-level string member survives a real read-modify-write untouched — exercised on a stamp-shaped member this build does not know, through the same carrier. It also asserts the stamp is a plain top-level string, so there is nothing about its shape an older reader could fail to carry.

**Why.** The older build cannot be linked into this test suite, so the alternative to an argument-plus-exercise would be a test that only re-states this build's behaviour. The test names the limitation in its own comment rather than implying it ran two werust versions.

**Rejected.** Vendoring the previous `pins.rs` as a test-only module (a second copy of the store to keep alive, for one assertion). A `cfg!` -gated "old build" mode (this repo retired its last `cfg!(test)` branch and `crates/werust-core/tests/pin_store_edge_wiring_shape.rs` reds if one returns).

## 7. The sibling test's unknown-member fixture is now a name nobody will implement

**Chosen.** `members_this_build_does_not_know_survive_a_read_modify_write` used the literal `normalizationVersion` as its example of a member THIS BUILD DOES NOT KNOW. This change makes that member real, so the fixture was replaced with `somethingNoWerustWillEverImplement`, and the test gained an assertion that the stamp — a member werust DOES own — is rewritten by the save.

**Why.** A fixture whose whole job is to be unknown must not be a name anybody would plausibly implement; the trap fired once already (the conductor caught it before dispatch). Naming it unimplementable is what stops the same test quietly stopping asserting what it says a third time. The test proves strictly more than before: unknown members are carried, AND owned members are werust's to write.
