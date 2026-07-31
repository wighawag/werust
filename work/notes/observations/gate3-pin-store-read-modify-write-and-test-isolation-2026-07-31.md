---
title: "Gate-3 verdict: pin-store-read-modify-write-and-test-isolation (APPROVE) — the store stops failing open on write, and still does on read"
date: 2026-07-31
status: open
reviewOf: pin-store-read-modify-write-and-test-isolation
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. The fix is correct and the comment explaining it is the kind this repo should keep writing: it states the failure it prevents, why two windows is not exotic, and why that direction of failure is the one a TOFU store cannot have.

## Criteria, ticked

1. **Blessing re-reads and merges, so a pin blessed in one shell survives a bless in another.** MET, covered by a test with two independently-constructed shells sharing a directory, and shaped exactly like the sibling `retrieval::apply_settings_request_in` the task pointed at. One edge case was handled better than I specified: with NO durable store to re-read, the in-memory pins ARE the truth (no file could have superseded them), so this session's earlier blesses are carried rather than dropped. I had not thought of that; a naive "always prefer disk" would have silently lost pins in the ephemeral case.
2. **Core tests no longer read the developer's real `pins.json`.** MET for `werust-core`. See residue 3.
3. **A test asserts the real store is untouched.** MET.
4. **The false GTK trust-surface history corrected in all three places.** MET (`DECISIONS.md` §8, the `macos-trust-surface-bless-affordance` task, and the matrix's trust-explanation cell).

## The residue I am cutting a task for

**The warning still reads a snapshot.** `self.pins` is populated at construction and refreshed only inside `bless_current_name`, so a long-lived window A never sees a pin window B blessed afterwards, and A's change-check stays SILENT for that name until A relaunches. That is the SAME direction of failure this task existed over — the user believes a name is blessed and no warning fires — merely narrower: the pin is no longer destroyed, just invisible to one window. Pins are per-USER, not per-window.

It is documented in a doc comment and nowhere else, which is how a known gap becomes an unknown one. Cut as `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`, with the fix prescribed as a re-read at NAVIGATION (not in the paint path, which was correctly rejected), plus two smaller residues folded in: `TrustedNamePins::load()` now has ZERO callers (unreferenced public surface duplicating the `Settings` arm — the same thing this drive just retired `CHROME_CSS_CLASS_SETS` for), and the mobile crates still read the real store.

## Raised to the human

**This introduced the repo's FIRST `cfg!(test)` branch in production code** (`PinStoreLocation::default()`). A grep finds no other. It works, `with_pins_dir` keeps its signature, and `Settings` behaves exactly as `None` did, so production behaviour is unchanged — but it is a new precedent of test-only divergence living in shipped code, and because `cfg!(test)` is per-CRATE it is the direct reason the mobile crates are still unisolated (honestly captured by the build in its own observation). The alternative is an explicit opt-in that production must call. I have told the follow-on task to prefer that if it is cleanly cheap and to retire the branch, but the precedent question is the human's.

## Ratified

- **`PinStoreLocation` as a private three-state type** (`Settings` | `Dir` | `Ephemeral`) owning load/save. Verified private; a clean home for a decision that was previously an `Option<PathBuf>` with implied meaning.
- **Read-modify-write is still not atomic across processes** (two writers can interleave read-read-write-write). This matches the sibling `retrieval` store exactly, so it is consistent rather than a regression; the follow-on records the expectation explicitly so nobody assumes more.
