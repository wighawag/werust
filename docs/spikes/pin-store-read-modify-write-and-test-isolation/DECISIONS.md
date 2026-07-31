# Decisions: read-modify-write blessing + pin-store test isolation (`pin-store-read-modify-write-and-test-isolation`)

The three residues of `ipns-tofu-pin-and-warn-on-change` this task closed were each specified precisely enough to build without a judgement call, except for the two choices below. Each says what was chosen, why, what was rejected and what it touches, so a reviewer can ratify or reverse it.

Task: `work/tasks/*/pin-store-read-modify-write-and-test-isolation.md`. The corrected history it fixes: `docs/spikes/ipns-tofu-pin-and-warn-on-change/DECISIONS.md` section 8.

## 1. `PinStoreLocation` replaces `pins_dir: Option<PathBuf>`, with a third state

**Chosen.** The shell's `pins_dir: Option<std::path::PathBuf>` became a private three-state `PinStoreLocation` (`Settings` | `Dir(PathBuf)` | `Ephemeral`) owning both `load()` and `save()`. The doc comment at the type is the primary home of this reasoning; this entry exists so the choice is visible to a reviewer who is reading the task, not the code.

**Why.** Residue 2 needs a shell that reads NO store at all, which `Option<PathBuf>` cannot express: `None` already means "the real settings directory". Adding a parallel `bool` beside the `Option` would have made two fields that must agree, which is the same conflation with an extra invariant. Putting `load`/`save` on the location type also removes the `match` that residue 1 duplicated at every mutation site, so a future second mutation (a "forget this pin" action, say) cannot re-introduce the wholesale-rewrite bug by forgetting to re-read.

**Rejected.** Keeping `Option<PathBuf>` plus a flag; making the real-directory read an explicit opt-in that PRODUCTION calls (the task offered this alternative, and it was rejected because forgetting the call at one of the five edges would silently disable TOFU persistence there, which is the same security-relevant direction of failure residue 1 is about, just moved).

**Touches.** Nothing outside `werust-core`: the field and the type are private, `with_pins_dir` keeps its signature, and production behaviour is byte-for-byte unchanged (`Settings` does exactly what `None` did).

## 2. A test shell defaults to NO store via `cfg!(test)`, and this covers `werust-core`'s tests only

**Chosen.** `PinStoreLocation::default()` is `Ephemeral` under `cfg!(test)` and `Settings` otherwise, so every shell built inside `werust-core`'s own test binary starts empty; a test opts into a store with `with_pins_dir`. A bless on such a shell holds in memory and returns `false` (unpersisted), which is the contract `bless_current_name` already documented for "no settings directory".

**Why.** It is the smallest change that makes the default SAFE rather than making safety opt-in, and it needs no new public surface. The alternative (a public `with_real_pin_store()` that production must call) inverts the failure direction, per decision 1.

**Rejected.** Pointing `WERUST_SETTINGS_DIR` at a scratch directory from the tests (process-global env mutation is a data race with the parallel test threads, and the whole point of the directory-taking seam is that no test needs it); a `#[cfg(test)]`-only constructor (every existing test would have had to change, and a new test would default back to the unsafe path).

**Touches.** `cfg!(test)` is per-CRATE, so this closes the hole for `werust-core`'s unit tests (the ones the task names) and NOT for `werust-android` / `werust-ios`, whose tests build shells through the PRODUCTION `CoreSession::new()`. Neither of those suites blesses anything, so nothing writes the developer's store; they do READ it. Captured, unfixed and in scope for nobody yet, in `work/notes/observations/mobile-core-session-tests-read-the-real-pin-store-2026-07-31.md`.
