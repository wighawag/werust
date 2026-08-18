# Decisions: the trust store's write (`trust-store-writes-atomically-and-keeps-fields-it-does-not-know`)

The second task of spec `trust-store-hardening`, after the third state landed (`docs/adr/0014`). It makes the WRITE of `pins.json` trustworthy in three ways at once: atomic, non-destructive of data it does not understand, and honest about failing. None of the three met the ADR gate on its own (temp-plus-rename is the obvious mechanism, not a surprising one, and the taxonomy is cheap to reverse), so they are recorded here plus as a module doc at the site (`crates/werust-core/src/pins.rs`), and ADR 0014 carries a pointer where this refines it.

Task: `work/tasks/*/trust-store-writes-atomically-and-keeps-fields-it-does-not-know.md`. Prior decisions this builds on: `docs/spikes/trust-store-fails-closed-instead-of-reading-as-nothing-trusted/DECISIONS.md` (the write refusal, decision 5, which this task keeps and now REPORTS distinguishably) and `docs/spikes/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns/DECISIONS.md` (the pin-store location, and the retired `cfg!(test)` branch this task did not reintroduce).

## 1. Atomicity is temp-file-plus-rename in the SAME directory, with an fsync each side

**Chosen.** `save_to` writes the document to `pins.json.tmp-<pid>-<n>` beside the store, `sync_all`s that file, and `fs::rename`s it onto `pins.json`; the directory is fsynced best-effort after the rename. Any failure removes the temp file and reports `CouldNotPersist`. The refusal read (`docs/adr/0014`) still happens BEFORE any of it, so a store werust cannot read is not even temp-written.

**Why.** A rename within one filesystem is the only cheap atomicity available and it is enough: a reader observes the old document or the new one. SAME directory is the load-bearing half, not tidiness: a temp directory is routinely a different filesystem, where the rename degrades into copy-then-delete and the guarantee silently disappears. The file `sync_all` matters for the same reason the rename does: renaming a file whose bytes are still only in the page cache can atomically swap in an EMPTY document after a power cut, which is the state this exists to prevent. The directory fsync is best-effort because it is not openable as a file on every platform (Windows) and its absence costs durability, never correctness for a reader.

**Rejected.** A `tempfile` crate dependency (a dependency for eight lines, in a crate that deliberately hand-rolls this kind of thing); writing into the system temp dir and renaming (not atomic across filesystems, which is the exact failure this task exists to remove); a `.bak` copy of the previous document (a second file to reason about, and a window where BOTH are partial).

**Touches.** `pins.json` now has transient siblings during a save. Anything that lists the settings directory should ignore `pins.json.tmp-*` (nothing does today; `load_from` reads the one file by name). A process KILLED between the write and the rename leaves one behind, and nothing sweeps them: deliberately, because a sweep run by one window could delete another window's in-flight temp file, and a stray temp file costs a few hundred bytes where the store itself is correct. The advisory-lock task will want the lock held AROUND this whole read-modify-write, not inside it.

## 2. The mid-write failure is exercised through an INJECTED write step, not a `cfg!(test)` branch

**Chosen.** `save_to` delegates to a private `save_to_through(dir, write_temp)` whose last argument is the step that puts bytes in the temp file; production passes `write_document`. Two tests drive that seam: one asserts, from INSIDE the step, that the temp file is a sibling and that the live document is untouched at that moment; the other writes half a document and returns an `io::Error`, then asserts the previous document is intact byte for byte and no temp file survives.

**Why.** The property is an ORDER (temp, then rename), and only an interruptible seam can prove an order; asserting end states alone would pass against a bare `fs::write`. It also keeps production free of test-only behaviour: `cfg!(test)` in production code was this repo's only such branch, it was deliberately retired by `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`, and `crates/werust-core/tests/pin_store_edge_wiring_shape.rs` reds the gate if it returns. A closure parameter is ordinary production shape, not a test hook: the default caller is the production one.

**Rejected.** A `#[cfg(test)]` fault-injection flag (the retired precedent, widened); making the target un-renameable to force a failure (it fails in the wrong place and destroys the "previous document is intact" assertion); accepting the failure path as untested (it is the whole point of the change).

**Touches.** `save_to_through` is private and in-module, so the seam adds no public API. A future writer ("forget this pin") should go through `save_to` and inherit all three properties.

## 3. Unknown members are carried in the STORE, keyed by pin key, not as a field on `TrustedNamePin`

**Chosen.** `TrustedNamePins` gains two private fields: the document's unknown top-level members, and a `BTreeMap<pin key, members>` of each entry's unknown members. `from_json` collects them, `to_json` writes them back FIRST and the four fields werust owns second, so a carried member can never shadow one werust is authoritative for. `TrustedNamePin` is unchanged.

**Why.** The obvious shape (a fifth field on `TrustedNamePin`) cannot be done without breaking the struct literals in three other files' tests (`crates/desktop-paint/src/lib.rs`, `crates/werust-core/tests/mobile_chrome_presentation_shape.rs`, plus this crate's own), and a PRIVATE field would make the type unconstructible outside this module entirely. Keying by the pin key also keeps the residue where the merge happens, which is exactly what the normalized-name keying task will have to migrate (its re-key must carry the residue with the entry).

Two consequences chosen deliberately: derived equality now compares the residue too (two stores are equal when they hold the same pins AND carried the same unrecognised data), which is why the residue is the RESIDUE and not the raw document. An in-memory store and a freshly-loaded one with no unknown members are still equal, so every existing round-trip assertion still means what it meant. And key ORDER in the output is `serde_json`'s (sorted; this repo does not enable `preserve_order`), so the document stays byte-stable across a no-op re-save, which one test asserts.

**Rejected.** Carrying the whole parsed `Value` and editing it in place (equality then depends on formatting, and every in-memory-vs-loaded assertion breaks); `#[serde(flatten)]` with derived structs (this module hand-builds JSON precisely to keep the wire spelling under its own control, and the store is four fields); dropping unknown members with a warning (that IS today's behaviour, and it is what user story 4 forbids).

## 4. An unknown ENTRY member survives a RE-bless of that entry

**Chosen.** Re-blessing a name replaces its cid, timestamp and posture, and KEEPS whatever unrecognised members that entry carried.

**Why.** The user story is blanket: an older build must not strip what a newer one wrote. The alternative reading (a newer build's per-entry metadata may describe the OLD cid, so drop it on re-bless) trades a certain loss for a possible staleness, and the newer build can re-validate its own field where it cannot resurrect a deleted one. It is also the only rule that is simple to state at the seam that will next touch it (the re-keying migration).

**Touches.** A later task that adds a field OF ITS OWN must remember it is now the authority for it on re-bless: once werust knows a member, it is in `KNOWN_ENTRY_MEMBERS` and is written from the struct, not carried.

## 5. The save outcome is a four-variant `PinSaveOutcome`, and `Refused` is reserved for the policy refusal

**Chosen.** `Recorded` / `NothingToRecord` / `CouldNotPersist(String)` / `Refused(UndeterminableTrust)`, returned by `TrustedNamePins::save`, `save_to`, `PinStoreLocation::save` and `BrowserShell::bless_current_name`. `is_recorded()` is the old boolean; `problem()` is the one place the two interesting outcomes are put into words, and returns `None` for the other two.

**Why.** "There was nothing to record", "I could not write it" and "I refuse to write over a store I cannot read" are three different sentences: the first is uninteresting, the second is an environment failure werust WOULD have written through, and the third is a deliberate policy decision that protects records werust cannot read. A bare `false` says none of them, which is what made a store that silently failed to persist indistinguishable from one that worked.

The naming is deliberate on two points a reviewer might trip over. `Refused` is reserved for the ONE policy refusal (the unreadable store, `docs/adr/0014`), so "no settings directory" and "no durable store on this shell" report `CouldNotPersist` even though the old test name calls that case "a refusal": there IS something to record and werust would have recorded it, there is simply nowhere. And `NothingToRecord` is the CALLER's answer, never the file's: `save_to` cannot produce it.

**Rejected.** A fifth `NoStore` variant mirroring `PinStoreRead::NoStore` (the read side needs that distinction because a caller's in-memory pins are still the truth when there is nowhere to read FROM; the write side does not: "nowhere to write" and "the write failed" are the same sentence to a user, and the reason string carries the difference for a developer). A `Result<(), …>` (a failure to record must not read as an error at a call site, per `docs/adr/0014`). Keeping `bool` and adding a separate `last_save_problem()` accessor (two ways to ask one question, and the state can go stale).

**Touches.** `BrowserShell::bless_current_name` changes its return type. The GTK edge already discarded the value; the two mobile FFI wrappers COLLAPSE it with `.is_recorded()` at their boundary, because a `bool` is those boundaries' existing contract and widening the FFI is a surface change this task is explicitly out of scope for. When a trust-management surface is built, it reads `problem()` rather than minting its own wording.

## 6. The withdrawn affordance reports `Refused`, not `NothingToRecord`

**Chosen.** `bless_current_name` keeps its ONE gate (`trust_pin_action_visible`, the very rule the edges paint the button from), but the gate is closed for two different reasons and the outcome now says which: with the shell's undeterminable axis set it returns `Refused(why)`, otherwise `NothingToRecord`.

**Why.** The affordance withdraws while the store cannot be read (`docs/adr/0014`), so without this the most interesting outcome would be reported as the least interesting one, which is the same flattening the ADR exists to prevent, one layer up. The deeper `PinStoreRead::Undeterminable` branch still refuses on the RACE (the store goes bad after the navigation that read it), so both paths report `Refused` and neither writes.

**Touches.** The reported reason comes from the shell's axis, which is as fresh as the last navigation or bless. It is a report, never a decision: the write itself re-reads the file before touching a byte.
