---
title: the macOS type-check harness's stand-in werust-core drifts from the real one with nothing watching
date: 2026-07-31
status: open
---

Spotted while repairing `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` (task `macos-spike-doc-accuracy-and-harness-guard`): besides the `desktop-paint` breakage that task owned, the script's hand-written stand-in `werust-core` had also silently fallen behind the real crate (it was missing `load_progress_tooltip` and `STOP_AFFORDANCE_LABEL`, which `crates/desktop-paint` imports), so the harness would have failed for a SECOND, unrelated reason once the first was fixed.

Nothing gates the stand-in against the real core's surface, and the harness is not run by CI, so each such drift is discovered by the next macOS agent as a confusing error rather than by the change that caused it. Both gaps were fixed by hand in that task; the underlying "who notices when the core moves" question is untouched.

## Occurrence log

Kept so the RECURRENCE is visible to whoever decides whether the stand-in should be generated, or symbol-checked against the real crate, rather than hand-maintained. Each entry is a separate change that broke the harness without anything going red.

1. **2026-07-31, the `desktop-paint` extraction** (repaired by `macos-spike-doc-accuracy-and-harness-guard`, item 0, titled "THE HARNESS IS NOW BROKEN"), plus the `load_progress_tooltip` / `STOP_AFFORDANCE_LABEL` gap above that the same repair uncovered behind it.
2. **2026-07-31, the `STUB_CHAIN_ID` -> `CHAIN_ID` rename** (`provider-refuses-honestly-instead-of-resolving-an-empty-account-list`). The stand-in's `provider` module still declared `STUB_CHAIN_ID` while the symlinked real `crates/macos-renderer/examples/trust_hooks_smoke.rs` had moved to `CHAIN_ID`. Caught by PR review, not by any gate: `crates/macos-renderer/tests/typecheck_harness_guard.rs` stubs `cargo` with `exit 0`, so it proves the scratch workspace ASSEMBLES but never that it COMPILES, and a rename walks straight through it. Repaired by hand (one line) in that task.
3. **2026-07-31, the TOFU trust-pin functions** (`ipns-tofu-pin-and-warn-on-change`, `e772025`). `crates/desktop-paint` imports `trust_pin_action_label`, `trust_pin_action_visible` and `trust_pin_detail` from `werust-core`; the stand-in never grew them, so the harness's window leg fails with `error[E0432]: unresolved imports`. Verified still failing identically on a clean `origin/main` checkout at `ae3cb6f`. NOT repaired — spotted in passing while fixing occurrence 2 and left out of scope for that task, so the harness is red today and the next macOS agent meets this error first.

Three hand-repairs (or non-repairs) in one day is the signal: the stand-in has no owner and no watcher. Occurrence 2 also shows the drift is not confined to `desktop-paint`'s imports — anything the harness symlinks can name a core symbol.
