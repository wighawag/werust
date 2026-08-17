---
title: "The trust store gets a THIRD state: unreadable is not nothing-trusted, and nothing is written while it holds"
slug: trust-store-fails-closed-instead-of-reading-as-nothing-trusted
spec: trust-store-hardening
blockedBy: [settings-mutation-requires-marked-user-intent-in-core-and-on-gtk]
covers: [1, 2, 5, 11, 12]
---

## What to build

Today an unreadable trust store is INDISTINGUISHABLE from a fresh install, in both directions, and the loss is then persisted. `TrustedNamePins::load_from` returns the default (empty) store for any read failure and for any parse failure; `from_json` silently DROPS an entry with no cid, no timestamp or a posture spelling it does not know, and silently keeps the FIRST of two entries that share a key; and the next save writes the survivors, so the dropped ones are gone for good. While the store is only advisory that costs a missing warning. The moment anything READS it to decide what loads (spec `withhold-changed-content-until-trusted`, which is tasked AFTER this one for exactly this reason) it becomes a silent browser-wide re-trust.

Give the store a third outcome, distinct from "no records", and give BOTH sides of it a rule:

- **Reading** yields either a store, or "cannot determine trust" carrying WHY (unreadable file, unparseable document, an entry werust cannot read honestly, two entries for one key). An unknown posture spelling, a missing cid or timestamp, and a duplicate key are all in the second class: the read REPORTS them instead of quietly shrinking the file, so a future fifth trust posture cannot un-trust every name recorded under it.
- **Writing REFUSES while that state holds.** This is the half a naive fix forgets and it is the exploitable one: inserting into what merely LOOKS like an empty store is how one transient read failure permanently replaces every record with a single fresh one, which is the same destruction from the write side.

The absence of records (a fresh install, no settings directory) stays exactly what it is today: an empty store, no warning, the pre-TOFU behaviour. The distinction is between "nothing recorded" and "cannot tell".

Carry the third state through the ONE reader the shell has, so no caller can flatten it back into "empty" by accident: the shell reads the store once per load and the chrome's TOFU axis is derived from that read, so the shell must decide, explicitly, what the chrome shows and what a bless attempt does while trust cannot be determined. Whatever it does, it must not claim a name is unblessed (which would offer the user a bless that would be REFUSED by the write rule) and must not break browsing: a store werust cannot read is not a reason to fail a load.

**This task deliberately INVERTS an existing assertion, in the same change.** `pins.rs`'s `a_missing_or_corrupt_store_degrades_to_no_pins_never_to_a_broken_load` currently asserts that a truncated document, an unknown posture, a missing cid and an empty name all degrade to no-pins, and the module's own fail-safe doc says so in prose. That assertion and that paragraph are what this task changes: rewrite them here rather than leaving the gate red between two tasks. Keep the half of the assertion that stays TRUE: no panic, no claimed pin nobody made, and a MISSING file (as opposed to an unreadable one) is still an empty store.

## Acceptance criteria

- [ ] A truncated document, a document whose `pins` member is not an array, an entry missing its cid or timestamp, an entry carrying an unknown posture spelling, and two entries sharing one key each yield the "cannot determine trust" outcome with a legible reason, NOT an empty store.
- [ ] A missing file, and no settings directory at all, still yield an empty store (a fresh install is not an error).
- [ ] No write reaches the store while the read said "cannot determine trust": asserted by a test that puts a corrupt document in a scratch directory, attempts the write path, and then asserts the corrupt bytes are STILL THERE byte-for-byte.
- [ ] The shell's per-load read handles the third state explicitly (it neither reports the name as unblessed nor fails the load), and what the chrome shows in that state is a stated, tested decision rather than a fallthrough.
- [ ] The module's fail-safe documentation and the existing corrupt-store test are rewritten in THIS change to state the new rule; the gate is green at the end of this task with no `#[ignore]` and no weakened assertion left behind.
- [ ] Tests cover the new behaviour in the existing `pins.rs` style (the in-module `ScratchDir`, driven through the directory-taking cores).
- [ ] Every store test stays isolated to its own scratch directory and the real `pins.json` is asserted UNTOUCHED before/after (the existing `real_pin_store_snapshot` helper), with no process-global `WERUST_SETTINGS_DIR` mutation.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`: purely to SERIALISE the two writers of `crates/werust-core/src/lib.rs` (8000+ lines, this repo's worst collision point). There is no logical dependency. That task owns the navigation/intent region; this one owns the pin-store location, the per-load store read and the bless path.

## Prompt

> Goal: make the trusted-name pin store distinguish "cannot determine trust" from "nothing trusted", on the READ side and the WRITE side, before anything depends on the file. The store is `werust_core::pins` (`TrustedNamePins`, `pins.json` beside `retrieval.json` under the `WERUST_SETTINGS_DIR` lever); the shell reads it once per load through the pin-store location abstraction in `werust-core`'s shell module and hands the chrome a plain value, so no presentation rule touches the filesystem. Keep that shape.
>
> What is wrong today, precisely: the load returns the DEFAULT store on any read or parse failure; the JSON parse FILTERS OUT an entry with no cid, no timestamp or an unknown posture spelling, and de-duplicates two entries for one key by keeping the first; and the next save persists the survivors, so the drop is permanent. Reading as "nothing trusted" and then WRITING into that apparent emptiness is a single transient failure destroying every record. Fix both halves in one change: a read outcome that carries WHY it cannot be determined, and a write that REFUSES while it holds. A MISSING file is still an empty store: a fresh install must behave exactly as it does now.
>
> Then carry it through the one reader: the shell's per-load read and the chrome's TOFU axis. Decide deliberately what the chrome shows while trust is undeterminable, and do not offer a bless the write rule would refuse. Do not fail a load over it: the store is advisory today and a load must never break because a file is corrupt.
>
> You are deliberately INVERTING an existing assertion. `a_missing_or_corrupt_store_degrades_to_no_pins_never_to_a_broken_load` and the module's "Fail-safe" doc paragraph both currently promise the degrade-to-no-pins behaviour this task removes. Rewrite them in THIS change (that is the point of doing it here rather than discovering it later), keeping the parts that remain true: no panic, no invented pin, missing-file-is-empty.
>
> Domain vocabulary: a **trusted name pin** is name to CID plus the timestamp and the `TrustPosture` at bless time; the verb is **bless**, never "pin" (see `CONTEXT.md` and the module docs; `docs/adr/0006` is the two-axis trust model). The posture wire vocabulary is shared with the chrome JSON and the debug view: do not mint a second spelling.
>
> Test at the seams the module already tests at: the directory-taking `load_from`/`save_to` cores against the in-module `ScratchDir`, plus the real-store-untouched snapshot assertion. No display, no network, pure Linux gate.
>
> Sequencing you must know about: `work/tasks/backlog/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns.md` is an UNPROMOTED task that edits the same two files and, among other things, proposes deleting or delegating `TrustedNamePins::load()` (which currently has no callers). If it has landed by the time you start, rebase and keep its shape; if it has not, do not do its job here, and leave that duplicate-loader question alone. Five sibling tasks from this spec follow this one in a strict chain (atomic write, canonical CID comparison, normalized-name keying and migration, the normalization corpus and stamp, the advisory lock): do NOT reach into their scope, and note that they will each rewrite parts of the file you touch.
>
> RECORD the fail-closed asymmetry (readers get a state, writers refuse) as an ADR in `docs/adr/` if it meets that gate, or as a module doc at the decision site plus a `## Decisions` block in the done record. It is exactly the kind of choice a reviewer would otherwise have to reverse-engineer.
