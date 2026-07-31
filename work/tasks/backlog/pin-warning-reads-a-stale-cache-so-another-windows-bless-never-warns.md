---
title: "The change-warning still reads a snapshot, so a name blessed in another window is never warned about in this one"
slug: pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns
blockedBy: [pin-store-read-modify-write-and-test-isolation]
covers: []
---

## What to build

The read-side twin of the defect `pin-store-read-modify-write-and-test-isolation` fixed on the write side, plus two small residues from the same review. Cut by the conductor at Gate-3 (2026-07-31).

**1. The warning check reads a cache that is only ever refreshed by a bless.** That task made blessing read-modify-write, so a pin now SURVIVES on disk when two windows bless. But `self.pins` remains a read-through cache populated at shell construction and refreshed only inside `bless_current_name`. So a long-lived window A never sees a pin that window B blessed after A launched, and A's change-check (`self.pins.check`, around `lib.rs:2891`) stays SILENT for that name until A relaunches or happens to bless something itself.

That is the same direction of failure the parent task existed over — the user believes a name is blessed, and no warning fires — merely narrower: the pin is no longer destroyed, it is just invisible to one window. Pins are per-USER, not per-window, so a user who blessed a name in one window reasonably expects the warning everywhere.

**The prescribed fix, and the thing NOT to do.** Do not re-read the store in the paint path: that was explicitly and correctly rejected (a file read on every chrome refresh). Re-read at NAVIGATION instead — once per navigation to a mutable name, which is exactly when the answer is about to be used and is already an I/O-bearing moment. Keep the cache for painting. If you find a cheaper trigger that is equally correct (for example, a modification-time check before the read), that is fine, but state why it is equally correct. Cover it with a test in the parent's style: two independently-constructed shells sharing one directory, bless in one, navigate the other to that name at a DIFFERENT CID, and assert the warning fires.

**2. `TrustedNamePins::load()` now has zero callers.** The shell goes through `PinStoreLocation::Settings`, which re-implements `settings_dir().map(load_from)` so it can distinguish "no directory" from "empty". `load()` is `pub`, so no dead-code lint fires, but it is unreferenced surface duplicating the `Settings` arm — and an unconsumed public helper is exactly what the same drive just retired `CHROME_CSS_CLASS_SETS` for. Either delete it or have `Settings` delegate to it; do not leave two ways to load one store.

**3. The mobile crates still read the developer's real `pins.json`.** Recorded in `work/notes/observations/mobile-core-session-tests-read-the-real-pin-store-2026-07-31.md`: the new empty-store default keys off `cfg!(test)`, which is per-CRATE, so it covers `werust-core`'s own tests but not `werust-android` / `werust-ios`, whose tests build shells through the production `CoreSession::new()`. Neither suite blesses anything, so nothing WRITES the real store — this is the same one-machine-only hermeticity hole, just narrower. Close it for the mobile crates too, preferably by a mechanism that is not another `cfg!(test)` branch (see the note below), and add the same assertion the parent added: the real store is untouched by the suite.

> **Read first, and decide deliberately:** the parent introduced `cfg!(test)` branching in PRODUCTION code (`PinStoreLocation::default()`), which the review flagged as the FIRST such precedent in this repo — a grep finds no other. It is also the reason residue 3 exists at all, since `cfg!(test)` cannot see across crate boundaries. If, while closing residue 3, an explicit opt-in (production constructs the shell with a location; tests pass an ephemeral one) turns out to be both cleaner and cheap, prefer it and retire the `cfg!(test)` branch. If it is not cheap, keep the branch and say why, but do NOT add a second `cfg!(test)` branch to the mobile crates without weighing that.

**Also worth recording (no code):** read-modify-write is still not atomic across processes — two writers can interleave read-read-write-write and lose a pin. That matches the sibling `retrieval` store exactly, so it is consistent rather than a regression, but if the pin store is ever expected to be multi-process safe it needs a real lock or an atomic rename. Note it in the spike so the expectation is explicit.

**Scope:** one refresh trigger with its test, one duplicate loader removed, the mobile test isolation, and two recorded notes. No change to the TOFU model, the trust posture, or the banner rules.

## Acceptance criteria

- [ ] A name blessed in one shell is warned about in ANOTHER long-lived shell sharing the same store, on navigation to that name at a different CID; covered by a test with two independently-constructed shells.
- [ ] The store is NOT re-read in the paint path; the refresh happens at navigation (or at a trigger argued to be equally correct).
- [ ] There is exactly one way to load the store (`TrustedNamePins::load()` is deleted, or `PinStoreLocation::Settings` delegates to it).
- [ ] `werust-android` and `werust-ios` unit tests no longer read the developer's real `pins.json`, and a test asserts the real store is untouched.
- [ ] Whether the `cfg!(test)` branch survives is a stated decision, not an accident.
- [ ] The cross-process non-atomicity of read-modify-write is recorded in the spike.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: close the READ side of the pin-store defect. `pin-store-read-modify-write-and-test-isolation` made blessing read-modify-write, so a concurrent bless no longer ERASES a pin — but `self.pins` is still a snapshot taken at shell construction and refreshed only inside `bless_current_name`, so a long-lived window never sees a pin another window blessed, and its change-check (`lib.rs` ~2891) stays SILENT for that name. Same missed-warning direction, narrower. Do NOT re-read in the paint path (correctly rejected: a file read per chrome refresh); re-read at NAVIGATION to a mutable name, which is when the answer is used and is already I/O-bearing, and prove it with two independently-constructed shells sharing a directory. Also: `TrustedNamePins::load()` now has ZERO callers because the shell went through `PinStoreLocation::Settings`, so delete it or have `Settings` delegate — do not leave two ways to load one store. And close the mobile half of the test-isolation hole (`werust-android`/`werust-ios` tests build shells through the production `CoreSession::new()` and still read the real `pins.json`), adding the same real-store-untouched assertion. Before you do: the parent introduced this repo's FIRST `cfg!(test)` branch in production code, and its per-crate nature is precisely why the mobile hole exists — if an explicit opt-in is cleanly cheap, prefer it and retire the branch; if not, keep it and say why, but do not add a SECOND one. Finally, record in the spike that read-modify-write is still not atomic across processes (same as the sibling `retrieval` store).
