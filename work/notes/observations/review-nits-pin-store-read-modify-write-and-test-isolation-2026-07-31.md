---
title: review-gate non-blocking nits for 'pin-store-read-modify-write-and-test-isolation' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: pin-store-read-modify-write-and-test-isolation
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'pin-store-read-modify-write-and-test-isolation' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify recorded decision 1: the shell's pins_dir Option<PathBuf> became a private three-state PinStoreLocation (Settings | Dir | Ephemeral) that owns load/save. Verified private, with_pins_dir keeps its signature, and Settings behaves exactly as None did, so production behaviour is unchanged. Confirm you want the third state rather than an explicit opt-in that production must call.
  (crates/werust-core/src/lib.rs:1652-1735; docs/spikes/pin-store-read-modify-write-and-test-isolation/DECISIONS.md section 1)
- Ratify recorded decision 2: PinStoreLocation::default() branches on cfg!(test), so a shell built inside werust-core's test binary reads no store. This is the FIRST cfg!(test) behaviour branch in the repo (grep finds no other), i.e. a new precedent of test-only divergence living in production code. It is also per-crate, so werust-android / werust-ios unit tests still read the developer's real pins.json; that residue is honestly captured in work/notes/observations/mobile-core-session-tests-read-the-real-pin-store-2026-07-31.md.
  (crates/werust-core/src/lib.rs:1696-1703; only cfg!(test) occurrence in crates/)
- Un-recorded residue: self.pins stays a read-through cache that is only refreshed on a bless, so a long-lived window A never sees a pin window B blessed after A launched, and A's change-warning check (lib.rs:2891 self.pins.check) would stay silent for that name until A relaunches or blesses. Strictly better than before (the pin now survives on disk), and refreshing in the paint path is explicitly rejected, but the missed-warning direction is the same class the task exists over and it is documented only in a doc comment, not captured as a note or follow-up task.
  (crates/werust-core/src/lib.rs:1639-1646 (cache doc), 2349-2359 (bless re-read), 2891 (warning reads the cache))
- Read-modify-write is still not atomic across processes: two writers can interleave read-read-write-write and lose a pin. This matches the sibling retrieval store the task pointed at, so it is in-scope-consistent, but worth an explicit note if the pin store is ever expected to be multi-process safe.
  (PinStoreLocation::save -> TrustedNamePins::save_to, plain fs::write with no lock (crates/werust-core/src/pins.rs:261))
- Nit: TrustedNamePins::load() now has zero callers anywhere in crates/ (the shell went through PinStoreLocation::Settings, which re-implements settings_dir().map(load_from) so it can distinguish no-directory from empty). Public, so no dead-code lint, but it is now unreferenced surface duplicating the Settings arm; consider deleting it or having Settings delegate.
  (crates/werust-core/src/pins.rs:227; crates/werust-core/src/lib.rs:1716-1720)
