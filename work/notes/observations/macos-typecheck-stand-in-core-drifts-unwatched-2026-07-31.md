---
title: the macOS type-check harness's stand-in werust-core drifts from the real one with nothing watching
date: 2026-07-31
status: open
---

Spotted while repairing `docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` (task `macos-spike-doc-accuracy-and-harness-guard`): besides the `desktop-paint` breakage that task owned, the script's hand-written stand-in `werust-core` had also silently fallen behind the real crate (it was missing `load_progress_tooltip` and `STOP_AFFORDANCE_LABEL`, which `crates/desktop-paint` imports), so the harness would have failed for a SECOND, unrelated reason once the first was fixed.

Nothing gates the stand-in against the real core's surface, and the harness is not run by CI, so each such drift is discovered by the next macOS agent as a confusing error rather than by the change that caused it. Both gaps were fixed by hand in that task; the underlying "who notices when the core moves" question is untouched.
