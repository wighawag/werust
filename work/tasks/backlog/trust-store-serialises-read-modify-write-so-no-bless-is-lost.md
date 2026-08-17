---
title: "Two windows recording two names must both survive: serialise the trust store's read-modify-write under an advisory lock"
slug: trust-store-serialises-read-modify-write-so-no-bless-is-lost
spec: trust-store-hardening
blockedBy: [trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp]
covers: [9, 12]
---

## What to build

The bless path is a read-modify-write with NO mutual exclusion: it loads the store from disk, inserts, and writes the whole document back. Two windows are a supported configuration (a second launch opens a second window, and two versions are two processes) and the code says so in its own comments. So two of them recording two DIFFERENT names concurrently lose one update: B reads before A writes, then B writes its snapshot over A's record. It is silent, and it fails OPEN, which is the one direction a trust store must not fail in. Atomicity (landed earlier in this chain) makes the loss CLEAN instead of corrupt; it does not prevent it.

Hold an advisory lock across the READ and the SAVE, so a check-and-record is one critical section. The write rate is about to rise from "when the user acts" to potentially every first successful load of a name (spec `withhold-changed-content-until-trusted`), so the window stops being theoretical.

What the task must settle explicitly, because a lock is where cheap fixes go wrong:

- **The lock is on the store's directory/file, not a process-global mutex.** The contending writers are separate PROCESSES, so an in-process lock proves nothing. A same-process second shell must be serialised too, since the tests construct two shells in one process.
- **A lock that cannot be acquired is not a crash and not a silent skip.** Decide the behaviour (bounded wait then refuse, with the refusal surfaced through the save outcome that landed earlier in this chain) and state it. A browser must not hang on a stale lock file.
- **A stale lock must not brick the store forever.** Whatever mechanism is chosen, say what happens after a process dies holding it.
- **No new dependency without saying why.** If a small vetted advisory-locking crate is the right answer, say so and name it; if the platform primitive is enough, use it. The repo's rule is "never hand-roll crypto or TLS", not "never use the standard library", but a hand-rolled lock protocol IS the kind of thing that wants a paragraph of justification.

The test that matters is the one that FAILS TODAY: two independently-constructed shells (or two writers) sharing ONE scratch directory, recording two DIFFERENT names concurrently, and afterwards BOTH records are present. Make it genuinely concurrent (threads plus a rendezvous, or two processes) rather than sequential-and-hopeful, and make it deterministic enough not to flake: this repo already carries a flaky-test observation, and a flaky concurrency test in the acceptance gate is worse than none.

## Acceptance criteria

- [ ] Two concurrent writers recording two DIFFERENT names against one store both survive: after the run the store holds BOTH records, asserted by a test that fails without the lock.
- [ ] Two concurrent writers recording the SAME name leave the store with exactly one record for it and no corruption (the last writer wins, or a stated alternative).
- [ ] The read and the save of one bless are inside ONE critical section (the read cannot be from before another writer's committed write).
- [ ] The lock serialises across PROCESSES, not just within one, and the test drives it in a way that would not pass with a process-local mutex.
- [ ] Failure to acquire is a stated behaviour with a bound (no indefinite hang, no silent skip), surfaced through the store's save outcome; covered by a test.
- [ ] What happens to a lock left behind by a dead process is stated and tested (the store must not be permanently unwritable).
- [ ] The concurrency test is deterministic (no sleep-and-hope) and is not marked ignored.
- [ ] Tests stay isolated to their own scratch directory, and the real `pins.json` is asserted UNTOUCHED before/after; no lock artefact is left in the real settings directory.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`: same file (`crates/werust-core/src/pins.rs`), last link in this spec's chain. It also depends on the earlier atomic write and save-outcome work.

## Prompt

> Goal: make a concurrent bless impossible to lose. The trusted-name pin store (`werust_core::pins`) is read-modify-written per action by the shell's bless path: it re-reads the store from disk (deliberately, so it does not rewrite a stale whole-file snapshot) then writes the whole document back. That closed the destructive case but left the LOST UPDATE: B reads before A writes, B's write wins, A's record is gone, silently, failing open.
>
> Hold an advisory lock across the read AND the save so one check-and-record is one critical section. The contenders are PROCESSES (a second launch is a second window; two versions are two processes) so an in-process mutex is not a fix; it must also serialise two shells inside one process, because that is how the tests will drive it. Settle and RECORD: the acquire-failure behaviour with a bound (never an indefinite hang, never a silent skip, surfaced through the save outcome the earlier task in this chain made distinguishable), and what happens to a lock a dead process left behind. Justify any new dependency in one paragraph or use the platform primitive.
>
> The acceptance test is the one that FAILS TODAY: two independently-constructed shells sharing one scratch directory, blessing two DIFFERENT names concurrently, both records present afterwards. Make it deterministic (a rendezvous, not a sleep): this repo already carries a flaky-test observation and a flaky concurrency test in the acceptance gate is worse than no test. Note that `work/tasks/backlog/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns.md` (unpromoted) asked for the cross-process non-atomicity to be RECORDED as a known limit; this task RESOLVES it, so if that task landed first, update what it recorded instead of leaving two contradictory statements in the tree.
>
> Do not extend the lock to the sibling `retrieval.json` settings store: that store's own mutation path is a different spec's territory (`settings-mutations-require-user-intent`) and widening the blast radius here would collide with it. If you find the same lost-update shape there, capture it as an observation under `work/notes/observations/` instead of fixing it here.
>
> You own `crates/werust-core/src/pins.rs` (plus, if the acquire-failure outcome needs it, the bless region of `crates/werust-core/src/lib.rs`: stay in that region, the file is this repo's worst collision point). Pure Linux gate, no display, no network.
