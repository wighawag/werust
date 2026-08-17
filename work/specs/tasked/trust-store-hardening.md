---
title: "Harden the trust store BEFORE anything depends on it: fail closed, write atomically, key stably, and never lose a concurrent write"
slug: trust-store-hardening
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.
> Tasked 2026-08-16 (`to-task`): the Implementation / Testing detail this spec carried moved INTO the tasks it emitted (`work/tasks/`, carrying `spec: trust-store-hardening`), which are the current truth for what to build. Durable rationale is recorded as ADRs by the tasks that decide it, not predicted here.

## Problem Statement

`pins.json` records "the user trusted name X at content-hash Y, on this date, at this trust posture". Today it is **advisory**: nothing reads it to decide what loads, so its rough edges cost at most a missing warning. Four of them are real, and all four are in the code now:

1. **An unreadable or corrupt store reads as "nothing trusted."** `load_from` returns an empty store for any read or parse failure, indistinguishable from a fresh install.
2. **Malformed entries are dropped, and the loss is then persisted.** `from_json` filters out any entry missing a cid or timestamp, or carrying a posture spelling it does not know; the next save writes the survivors, so the dropped ones are gone for good. A future fifth trust posture would silently un-trust every name recorded under it.
3. **The write is a whole-file overwrite with no atomicity.** `save_to` is a bare `fs::write` into the live path: a crash, an OOM kill, or a full disk truncates it. There is no temp-file-plus-rename.
4. **The lookup key is not the resolved identity.** `pin_key` is `trim().to_lowercase()` — ASCII case folding — while name resolution normalizes through ENSIP-15 and throws the normalized string away. Any name whose normalization is not plain case folding (emoji with and without the U+FE0F variation selector, fullwidth or circled Latin, other confusables) produces TWO keys for ONE resolved identity. The module's own doc calls a casing split "the one failure mode a TOFU store cannot have"; its guard covers ASCII only.

Plus two that only bite once something depends on the file:

5. **A read-modify-write with no mutual exclusion.** Two windows are a supported configuration and the code says so. Two of them recording two different names concurrently lose one update: B reads before A writes, then B writes its whole-file snapshot over A's. Silent, and it fails open.
6. **Duplicate keys silently pick a winner.** `from_json` sorts then de-duplicates by name, keeping the first. A hand-edited or partially-merged file yields one of two entries with no signal.

**Why now, separately, and first.** `withhold-changed-content-until-trusted` makes this file DECIDE WHAT LOADS. Every item above then converts from a missing warning into a vulnerability: an unreadable file becomes a silent browser-wide re-trust of every site at whatever it points to now, a lost write becomes an unprotected name, a key mismatch becomes a second record written with no warning shown. These are questions about behaviour under corruption and concurrency, and they are answered by building and testing the file's semantics — not by specifying them alongside a five-edge UI change.

## Solution

The store gains the semantics a load-bearing record needs, while it is still advisory and nothing can be broken by getting them wrong.

### Unreadable is a THIRD state, with defined behaviour on BOTH sides

"Cannot determine trust" becomes distinguishable from "no records". Naming the state is not enough — both paths need a rule, and both wrong answers are exploitable:

- **Readers** get the state and must handle it explicitly rather than seeing an empty store.
- **Writers must NOT write** while the store is unreadable. Inserting into what looks like an empty store is how one transient read failure permanently replaces every record with a single fresh one — the very failure item 1 describes, reintroduced from the write side.

### Atomic writes, and preserved unknown fields

Temp-file-plus-rename so a partial write can never be observed. Fields the running version does not recognise survive a rewrite, so an older build cannot silently strip a newer one's data — the two-versions-as-two-processes case the code already contemplates. A malformed entry is no longer silently dropped-and-persisted: the read reports the problem instead of quietly shrinking the file.

### One key per identity, migrated, and stable across dependency bumps

The key becomes the ENSIP-15-normalized name, obtained from the resolution path rather than re-derived — which means the resolution path must start returning it instead of discarding it.

Two consequences the naive version misses:

- **Existing records must be re-keyed.** A non-ASCII record written under the old ASCII fold would MISS after the change, and the caller would record a second one. Migration is part of this work, not an afterthought.
- **Stability must be pinned.** The lookup key is derived from a normalization library, so a version bump that changes any mapping re-keys every affected record. A fixed non-ASCII corpus test makes that a red gate rather than a silent re-trust, and the store records which normalization version wrote it so a change is detectable rather than inferred.

Names that do not normalize, and non-ENS names (a bare IPNS key is not an ENS label), need a stated rule rather than an accidental fallback into a second key space.

### One writer at a time, and one canonical CID form

Read-modify-write happens under an advisory lock held across the read and the save, so a check-and-record is one critical section. Atomicity makes a lost update clean instead of corrupt; it does not prevent it.

CID comparison becomes form-insensitive. The ENSIP-7 decoder emits whatever form the contenthash carried, so a CIDv0 `Qm...` and the CIDv1 `bafy...` of the SAME content are different strings today, and Android canonicalizes CIDs to lowercase base32 for its internal origin. Comparing raw strings therefore reports "changed" for a republish of identical content — a false positive, and false positives are what train a user to click through the warning that matters.

## User Stories

1. As a user whose trust store is corrupt or unreadable, I want werust to treat that as unknown rather than as nothing-trusted, so that one bad file is not a silent re-trust of every site I visit.
2. As a user whose store is unreadable, I want werust not to WRITE to it, so that a transient read failure cannot permanently destroy the records I still have.
3. As a user on a device that lost power mid-write, I want my store intact, so that a crash cannot truncate it into an empty one.
4. As a user running two werust versions, I want the older one to preserve fields it does not understand, so that using an old build does not strip data the new one wrote.
5. As a user whose store contains an entry werust cannot parse, I want to be told rather than have it silently dropped and the loss saved, so that a future format or posture cannot quietly un-trust a name.
6. As a user of a name spelled with non-ASCII characters, I want ONE record per name however it was typed or linked, so that an invisible spelling variant cannot make a trusted name look brand new.
7. As an existing user with a non-ASCII name already recorded, I want it re-keyed on upgrade, so that the fix does not itself lose my trust for exactly the names it protects.
8. As a user, I want a normalization-library upgrade to red the build rather than silently re-key my store, so that a dependency bump is not a trust reset.
9. As a user running two windows, I want two records written at the same time to both survive, so that being protected on one site does not silently cost me another.
10. As a user of a site that republished identical content under a different CID form, I want no spurious change warning, so that the warning stays rare enough to mean something.
11. As a user whose store holds two entries for one name, I want that reported rather than silently resolved to one of them, so that a tampered or half-merged file cannot quietly choose for me.
12. As a developer running the test suite, I want every store test isolated to a scratch directory, so that tests never read or write my real trusted names.

## Out of Scope

- **Anything that READS the store to make a decision** — that is `withhold-changed-content-until-trusted`, which is why this lands first: it inherits answers rather than assertions.
- **Where the store lives** — `settings-location-on-every-edge` owns the directory and the location migration. This spec owns the file's contents and semantics, and the two migrations must not be run as one undifferentiated step.
- **Retaining content** — `blessed-version-content-retention`.
- **A trust-management UI.** Nothing here is user-facing except the absence of silent loss.
