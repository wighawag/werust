# Decisions: keying the trust store on the resolved normalized name (`trust-store-keys-on-the-resolved-normalized-name-and-migrates`)

The fourth task of spec `trust-store-hardening`, after the third state (`docs/adr/0014`), the atomic write (`docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md`) and the canonical-CID comparison (`docs/spikes/trust-store-compares-cids-by-canonical-form/DECISIONS.md`). That one made the CID side of a pin an identity rather than a spelling; this one does the same for the NAME side, and migrates what is already recorded.

The task's two open questions were answered by the human before the build and are recorded in the task body's `## Decisions` section (an unnormalizable name gets no key and therefore no pin; a non-ENS name goes through the same normalization, which is a no-op on it). They are implemented as stated and are NOT re-litigated here. What follows is the shape the build had to choose, one entry per choice, so a reviewer can ratify or reverse each.

Task: `work/tasks/*/trust-store-keys-on-the-resolved-normalized-name-and-migrates.md`.

## 1. ONE key derivation, reaching the ONE normalizer call site

**Chosen.** `pins::pin_key(name) -> Result<String, UnkeyableName>` is the store's whole key derivation, and its body is `ens::normalize_name(name.trim())`. `ens::normalize_name` is the only call of the bound `ens-normalize` crate in the codebase: `namehash` folds its output, `ens::resolve` reports it as `EnsResolution::normalized_name`, `name_resolution::ResolvedName` carries it out of both cases, and the shell stores it on `EnsIdentity::normalized_root_name` and hands THAT to `TrustedNamePins::check` / `bless`.

**Why.** The failure being fixed is two derivations of one identity disagreeing (an ENSIP-15 normalization inside the namehash, an ASCII `trim().to_lowercase()` in the store), so the fix is one derivation with one call site, not two derivations that agree today. It also leaves the follow-on (`trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`, which must record WHICH normalization version wrote a store) exactly one site to stamp.

The trim lives in `pin_key`, not in `normalize_name`: it is the STORE's existing tolerance for a stray space around a typed name (` ronan.eth ` has always been `ronan.eth` here), and pushing it down into the shared normalizer would silently make `namehash(" ronan.eth ")` resolve where it fails today, a user-visible change to ENS resolution that this task has no business making.

**Rejected.** Letting the store re-derive the key from whatever string a caller holds (that is the second call site the task forbids: the caller's string is the typed spelling, and keying on the spelling is the bug). Passing an opaque `PinKey` newtype from the resolution to the store (it would move the derivation to the resolution's edge, but every test and every FFI boundary that holds a name as a `String` would need to mint one, for no property `pin_key` does not already give).

**Touches.** `ens::resolve`'s return type changed from `DecodedContenthash` to `EnsResolution` and `ResolvedName`'s two variants gained a `normalized_name` field, so `crates/werust/src/main.rs`'s CLI tests construct one more field. The CLI's printed output is deliberately UNCHANGED (it still echoes the name the user asked for); if `werust resolve` should ever print the normalized identity, that is the `cli-resolve-help-and-comment-accuracy-and-the-lost-refusal-test` task's territory, not this one's.

The shell keeps BOTH names per site: `EnsIdentity::root_name` is what the URL bar shows (the spelling the user typed, unchanged — there is still no rewrite of the bar) and `normalized_root_name` is what the store is keyed on. One user-visible consequence, deliberate: `MutableNameTrust::name` — and through it the trust popover's sentence, the change banner and `chrome_json`'s `mutableName` — now names the NORMALIZED identity. That is the name the user's trust is recorded under, so it is the honest thing to quote back to them, and it is identical for every ASCII name (which is every name any test or edge exercises today).

## 2. The migration is a READ-TIME re-key, persisted by the next ordinary write

**Chosen.** `read_entry` applies today's `pin_key` to the name it reads out of `pins.json`. That is the entire migration: an old ASCII-folded record is found by ONE lookup immediately, the first bless after the upgrade RE-blesses that record instead of adding a second, and the new key reaches disk through the ordinary `save_to`. There is no startup rewrite and no migration verb.

**Why.** Every property the task asks for falls out of it rather than being re-implemented: it is idempotent by construction (normalizing an already-normalized key returns it, so a second launch re-keys nothing), it inherits the write rules WHOLE because the only write is the existing atomic, refuse-while-unreadable `save_to`, and a store nobody ever writes again is still read correctly forever. It also keeps the sibling task's stated rule intact: `docs/spikes/trust-store-compares-cids-by-canonical-form/DECISIONS.md` explicitly rejected "a write triggered by a READ, in a store whose whole design says a read must not write", and a load-time rewrite would have been exactly that, landing before the advisory lock of `trust-store-serialises-read-modify-write-so-no-bless-is-lost` exists to serialise it.

**Rejected.** A migration pass that loads, re-keys and SAVES at startup (a write no user action asked for, racing every other window, and needing the refusal/atomicity rules restated at a second call site). A `migratedFrom` / dual-key lookup that checks both the new key and the old fold (that is precisely the "two key spaces" the task forbids, and it would have to be carried forever). Re-keying in `bless` only (a store that is never blessed again keeps warning about nothing).

**Touches.** The persisted document changes shape for non-ASCII names only, and only on the next save. `unknown_entry_members` is keyed by the same `pin_key`, so a LATER build's fields on a re-keyed entry follow the entry to its new key rather than being orphaned.

## 3. Two old records that collapse onto one key are REPORTED, via the rule that already exists

**Chosen.** Nothing new: `from_json` already sorts by key and reports `UndeterminableTrust::DuplicateName` for two entries with one key, and the re-key happens before that check, so a collapse falls straight into it. The store is then undeterminable (the chrome says so, the write refuses, nothing is overwritten).

**Why.** The collapse IS the bug being fixed showing up in the data: the user has two records for one identity, blessed at two different CIDs, and there is no honest way to pick one — the "newest" is a guess about which content they actually inspected. The sibling task that landed the third state made this reportable exactly so a later task would not invent a second answer. Fail-closed also protects the two records: while it holds, no write can flatten them.

**Rejected.** Keeping the most recently blessed entry (chooses for the user which content they trusted). Keeping both under a disambiguated key (a second key space, forbidden). Merging when both CIDs name the same content root (a plausible special case, but it makes the honest-collapse path depend on the CID comparison, and it silently resolves the one case a human should look at).

**Touches.** There is still no trust-management surface, so a user who hits this sees "werust cannot determine what you have trusted" and no bless offer, with the file untouched. That is the same dead end every other `UndeterminableTrust` reaches today; giving it an exit is the unbuilt trust-management surface's job, not this task's.

## 4. The FALLIBLE key gets a SPOKEN refusal: `UnkeyableName`, `check -> Option`, and a fifth `PinSaveOutcome`

**Chosen.** Three linked pieces implement the task's decision 1 ("no key, no pin, no warning; make the refusal legible at the call site rather than a silent `None`"):

* `pin_key` returns `Result<String, UnkeyableName>`, carrying the normalizer's own reason;
* `TrustedNamePins::check` returns `Option<MutableNameTrust>` — `None` for a name with no key, so the chrome shows NO TOFU axis at all;
* `TrustedNamePins::bless` returns `Result<(), UnkeyableName>`, and `BrowserShell::bless_current_name` reports the new `PinSaveOutcome::Unkeyable(UnkeyableName)`.

**Why.** The alternative for `check` was a `MutableNameTrust { blessed: None }`, which reads as "the user has simply not blessed this name" — it would OFFER a bless that the write could never record, which is precisely the silent `None` the decision forbids. The alternative for the write was to swallow the refusal: the user clicks bless, `save_to` writes an unchanged document, the outcome says `Recorded`, and no warning ever follows. In a TOFU store a swallowed refusal is a lost warning, so the write path is given no "cannot happen" branch even though this one genuinely cannot happen through the browser (`ens::resolve` refuses an unnormalizable name with its own typed error long before there is a page to bless, asserted by `an_unnormalizable_name_never_reaches_a_resolution_so_it_never_reaches_a_pin`).

`PinSaveOutcome::Unkeyable` is a REFUSAL, so it is deliberately neither `CouldNotPersist` (whose doc says "werust WOULD have written it") nor `NothingToRecord` (there was something the user asked to record). It joins a taxonomy nothing displays yet, for the same reason the other two refusals are in it: the surface that eventually says one of these sentences must not be blocked on a plumbing change.

**Coherence.** `UnkeyableName` is a NEW named concept, and it overlaps `ens::ResolutionError::UnnormalizableName`: both say "ENSIP-15 refuses this name". It is minted anyway because they sit at different layers (one is a resolution failure, the other is "this name has no store key") and because a store returning a `ResolutionError` would promise failure modes — no resolver, a bad RPC — it cannot produce. It carries the normalizer's own `detail` string rather than wording a second reason, so there is one explanation with two envelopes, not two explanations.

**Touches.** `TrustedNamePins::check`'s and `bless`'s signatures changed (call sites: the shell, and this module's tests). The mobile FFI boundaries are unaffected: they read `PinSaveOutcome::is_recorded()`, which is `false` for the new variant.

## 5. A RECORDED name this build cannot key makes the store undeterminable, rather than being dropped

**Chosen.** If `read_entry` meets a name today's normalizer refuses, the read fails with `UndeterminableTrust::UnreadableEntry` naming the entry and quoting the normalizer's reason.

**Why.** It is the rule this module already applies to a posture spelling this build does not know, and for the same reason: dropping the entry would be silent, and the next save would persist the survivors and make the drop PERMANENT — the user loses a pin they are relying on, with nothing on screen to say so. Reporting is loud, refuses every write while it holds, and leaves the file byte for byte intact.

**The cost, stated.** It means a normalizer version that STOPS accepting some name makes a whole store undeterminable until a human looks at it. That is the fail-closed direction (werust says "I cannot tell" rather than quietly un-trusting a name), and it is exactly the risk the next task in the chain (`trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`) exists to pin down with a fixed corpus and a stamp recording which normalization version wrote the store. This task leaves the derivation in one place so that stamp has one site to attach to.

**Rejected.** Skipping unkeyable entries (a silent, permanent pin loss). Keeping them under their recorded name (a second key space that never converges). Treating them as `Unparseable` (that is "this document is not the wire form"; the document is fine, one entry is not).
