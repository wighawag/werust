# Decisions: one writer at a time (`trust-store-serialises-read-modify-write-so-no-bless-is-lost`)

The last task of spec `trust-store-hardening`. Blessing a name was already a READ-MODIFY-WRITE against the file (`pin-store-read-modify-write-and-test-isolation`), which stopped a stale whole-file snapshot ERASING another window's records, and the write was already atomic (`trust-store-writes-atomically-and-keeps-fields-it-does-not-know`), which stopped a crash truncating the document. Neither closes the LOST UPDATE: B reads before A writes, then B's document lands on top of A's and A's record is gone, silently, failing OPEN. This task holds an advisory lock across the read AND the save, so a check-and-record is one critical section.

Task: `work/tasks/*/trust-store-serialises-read-modify-write-so-no-bless-is-lost.md`. Prior decisions this builds on: `docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md` (decision 1 already noted that "the advisory-lock task will want the lock held AROUND this whole read-modify-write, not inside it" — it is), `docs/spikes/trust-store-fails-closed-instead-of-reading-as-nothing-trusted/DECISIONS.md` (the refuse-while-unreadable write, kept whole), and `docs/spikes/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns/DECISIONS.md` (which deliberately did NOT record the cross-process non-atomicity as a standing limit, because this task fixes it — so there is no contradictory statement in the tree to retract).

None of this met the ADR bar on its own: a file lock is the ordinary mechanism rather than a surprising one, and every choice below is cheap to reverse. It is recorded here plus as a module doc at the site (`crates/werust-core/src/pins.rs`, the "One writer at a time" section), which is where a reader of the code will look first.

## 1. The lock is the PLATFORM primitive from `std`, on a SIBLING `pins.lock` — no new dependency

**Chosen.** `std::fs::File::try_lock` (stable since Rust 1.89; this repo pins 1.97 in `rust-toolchain.toml`) on a `pins.lock` file in the same directory as `pins.json`. That is `flock(2)` on Unix and `LockFileEx` on Windows, so the lock lives on the open file DESCRIPTION.

**Why.** The task's rule is "justify any new dependency in one paragraph or use the platform primitive", and the platform primitive is now IN the standard library, so there is no paragraph to write: no `fs2`/`fs4`/`fd-lock` in the tree, no new lineage in `Cargo.lock`, nothing to keep up to date, and no hand-rolled lock protocol either (which is the thing that actually would have needed justifying). Being an OS lock is not just convenience: it is what makes decision 4 true for free.

A SIBLING file, never `pins.json` itself, for two independent reasons. The save RENAMES a new file over the store, so a lock taken on the store would be held on an inode the next writer never opens. And creating `pins.json` merely to have something to lock would turn a fresh install into an EMPTY file, which this store reads as `UndeterminableTrust` and then refuses to write over — a self-inflicted brick. As a bonus, `LockFileEx` is MANDATORY on Windows (it blocks other processes' reads of the locked file), which would be a real hazard on the document and is harmless on a file nobody reads.

**Rejected.** `fd-lock` or `fs4` (a dependency for what `std` now does, even though both would have reused crates already in the lock); raw `libc::flock` plus `windows-sys::LockFileEx` behind `cfg`s (werust-core is platform-free today and this would make it the crate's first target-gated code, to reimplement `std`); an `O_EXCL` create-a-lock-file protocol (no dependency either, but a lock a dead process left behind then has to be broken by GUESSING at an age, and the guess is another race — see decision 4); a `Mutex` in the shell or a process-global `static` (proves nothing about the contenders that actually lose pins: a second launch is a second window, and two versions are two processes).

**Touches.** Every settings directory that has ever been written gains a permanent empty `pins.lock` beside `pins.json`. Nothing sweeps it: deleting a lock file is a race, not tidiness. Anything that lists the settings directory must expect it (`settings-location-on-every-edge` owns that directory; nothing enumerates it in production today). The store's own tests assert the file listing, so the expectation is checked rather than assumed.

## 2. The critical section is a CLOSURE (`TrustedNamePins::update_in`), and the settings-directory `save` is gone

**Chosen.** `update_in(dir, change)` takes the lock, RE-READS the document, runs `change` on it, saves while still holding the lock, and releases. It answers a `PinStoreUpdate { pins, outcome }`. `save_to(dir)` still exists for writing a document a caller already has, and takes the same lock for its write; the private write core takes a `&WriteLock` it cannot fabricate, so "hold the lock while writing" is something the compiler asks for. `TrustedNamePins::save()` (the settings-directory one) is DELETED, replaced by `TrustedNamePins::update()`.

**Why.** The alternative shape — hand callers a lock guard and trust them to read, modify and save inside it — puts the whole property in each caller's memory, and this module has an explicit rule against that ("it lives HERE rather than at each caller so no writer can forget it", the refuse-while-unreadable read). A closure makes the section un-openable in the wrong order. Deleting `save()` follows the precedent this store already set for `load()` under `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`: do not leave two ways to write one store, especially when the second way is "write this snapshot", which is the exact bug the shape exists to prevent.

The closure is also what makes the lost-update test DETERMINISTIC (decision 5) without a single test-only branch in production, exactly as the injected write step does for the atomicity test.

**Rejected.** A public `PinStoreLock` guard plus the existing `load_from`/`save_to` (forgettable, and the forgetting is silent); putting the lock inside `load_from` (readers would queue behind writers on every navigation, buying nothing, since an atomic write is already whole-document); locking inside `save_to` only (that is what shipping today's code with a lock bolted on would look like: the READ would still be outside, so the lost update would survive).

**Touches.** `crates/werust-core/src/lib.rs`'s `PinStoreLocation` loses `save` and gains `update`, and `BrowserShell::bless_current_name` goes through it. That is the shell's only mutation of the store, and a future "forget this pin" now has nowhere else to go.

## 3. Failing to acquire is `CouldNotPersist`, after a bounded 2-second wait, and nothing is applied

**Chosen.** A writer polls `try_lock` for `pins::WRITE_LOCK_WAIT` (2 s, 2 ms between attempts) and then gives up with `PinSaveOutcome::CouldNotPersist("another werust window was recording a trusted name and did not finish within 2s, so nothing was written and nothing was lost (the bless can simply be repeated)")`. `PinStoreUpdate::pins` is `None`, so the shell's cache and the chrome are left exactly as they were.

**Why.** The three properties the task asks for are the bound (never an indefinite wait: a browser must not freeze because another window is wedged), the audibility (never a silent skip), and the route (through the save outcome the earlier task made distinguishable). 2 s is far longer than the section it guards — one small read plus one small write, microseconds when nothing else is running — so an honest contender always gets in; and it is short enough that the pathological case ends in an answer rather than a frozen window. The wait is a bounded POLL rather than a blocking lock because neither platform offers a lock-with-timeout and a blocking `flock` cannot be cancelled once entered, which is the very hang the bound forbids.

Contention is deliberately NOT a new `PinSaveOutcome` variant. To everyone who can read the outcome it says exactly what a full disk says — "werust would have written it and could not" — which is what `CouldNotPersist` means and what distinguishes it from the one POLICY refusal (`Refused`, a store werust cannot read). A fifth variant would ripple through every exhaustive match, including the two mobile FFI boundaries that flatten the whole taxonomy to `is_recorded()`, for a difference nothing branches on: there is no trust-management surface, and no automatic retry. **This is the one choice here another task might be surprised by**, so it is written down: if a surface ever wants to auto-retry contention (and only contention), it needs a variant, and this is the paragraph to reverse.

The bless does NOT hold in memory for the contended case, unlike every other `CouldNotPersist`. That is not an inconsistency but the honest reading: the store was never READ, so werust never computed what the record would be, and showing a bless it did not perform is the failing-open direction this whole spec is about. The user repeats the action. The variant's own doc says so.

**Rejected.** Blocking indefinitely (a frozen window on a stale holder); giving up on the first `WouldBlock` (turns a 200 µs overlap into a lost bless); a new `Contended` variant (above); retrying automatically after the bound (the same wait, spelled twice).

## 4. A lock a dead process left behind releases itself; the FILE stays

**Chosen.** Nothing detects, breaks or ages out a stale lock. The lock is the operating system's, held on a descriptor, so it is released when the holder's process ends however it ends: an orderly exit, a panic, an OOM kill, a `SIGKILL`, a power-off. The empty `pins.lock` file remains and holds nothing.

**Why.** This is the whole reason decision 1 binds an OS lock instead of hand-rolling an `O_EXCL` lock FILE. A lock-file protocol has no way to tell "a live window is mid-save" from "a window died three days ago", so it has to guess an age, and a wrong guess either bricks the store for the timeout or breaks a live writer's section. There is nothing to state here beyond the mechanism, which is exactly the point: the store cannot become permanently unwritable, because the state that would make it so cannot exist.

The residue is asserted rather than argued: `a_write_lock_a_dead_process_left_behind_never_bricks_the_store` SIGKILLs a real child process mid-hold and then records two pins through the file it left behind.

**Rejected.** Deleting the lock file after a save (a race: another process may have it open and locked right then); an age-based stale-lock break (guessing, above); writing the holder's PID into the file and checking liveness (PID reuse, and it re-implements what the kernel already guarantees).

## 5. The concurrency tests are deterministic in BOTH directions, and one of them is a second PROCESS

**Chosen.** Three tests carry the property:

- `pins::tests::two_writers_recording_two_different_names_both_survive` forces the losing interleaving from INSIDE the critical section: each writer announces (through a condvar) that it has read and then waits up to 200 ms for the other to announce the same. Without the lock both writers read, both announce, both waits return at once and one record is erased. With the lock the second writer is still waiting for the LOCK, so it cannot announce, the first writer's window simply elapses, and both records land. The assertion never depends on which writer was quicker.
- `pins::tests::a_write_lock_another_process_holds_bounds_the_wait_and_then_says_so` and `..._a_dead_process_left_behind_never_bricks_the_store` re-run the test binary as a CHILD PROCESS (one env var, set per-child, never on this process) that takes the lock and holds it until killed. A process-local mutex passes neither.
- `tests::two_windows_blessing_two_names_at_the_same_moment_both_survive` (the shell level) builds two INDEPENDENT `BrowserShell`s in two threads over one scratch directory, settles both on their page, and blesses two different names either side of a `Barrier`.

**Why.** The repo already carries a flaky-test observation, and a flaky concurrency test in the acceptance gate is worse than none — so "deterministic" was taken to mean deterministic in both directions: never red when the code is right, and reliably red when it is not (a test that only *usually* catches the bug is a test that will one day stop catching it silently). The bounded wait inside the section is a CEILING on a condition that provably cannot arrive while the lock holds, not a sleep hoping for a race.

**Measured**, by disabling the lock (`try_lock` short-circuited to `Ok`) and re-running:

| test | with the lock | without it |
| --- | --- | --- |
| `two_writers_recording_two_different_names_both_survive` | 12/12 pass | 10/10 FAIL (one record on disk instead of two) |
| `two_windows_blessing_two_names_at_the_same_moment_both_survive` | 20/20 pass | 20/20 FAIL |
| `a_write_lock_another_process_holds_bounds_the_wait_and_then_says_so` | 6/6 pass | FAIL (the change runs, so the parent writes straight through the holder) |

**Rejected.** A bare "spawn two threads and hope they collide" (probabilistic in the direction that matters, which is the red one); marking any of them `#[ignore]`; a `#[cfg(test)]` hook in the bless path to force the interleaving (this repo retired its only `cfg!(test)` branch and a shape guard reds the gate if one returns — the closure of decision 2 is ordinary production shape and needs no hook); `flock(1)` from a shell for the cross-process half (util-linux only, so it would not run on the macOS leg).

**Touches.** The suite gains one test that is not a test — `pins::tests::a_child_process_that_holds_the_write_lock` does nothing at all unless its env var is set — plus about 2 s of wall clock for the bound test, and 4 s of a real second process. Tests stay in their own scratch directories, and the isolation test now snapshots the real settings directory's LISTING (not just `pins.json`'s bytes), so a lock artefact appearing in the developer's own settings directory would red the gate.

## 6. What was left alone

`retrieval.json`, the sibling settings store, has the SAME lost-update shape (`retrieval::apply_settings_request_in` is load → mutate → save with no mutual exclusion). It is deliberately untouched: its mutation path belongs to spec `settings-mutations-require-user-intent`, and widening the blast radius here would collide with it. Captured instead as `work/notes/observations/retrieval-settings-store-has-the-same-lost-update-shape-2026-08-18.md`.
