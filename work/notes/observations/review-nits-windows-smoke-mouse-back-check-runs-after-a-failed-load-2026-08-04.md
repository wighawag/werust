---
title: review-gate non-blocking nits for 'windows-smoke-mouse-back-check-runs-after-a-failed-load' (Gate 2 approve)
date: 2026-08-04
status: open
reviewOf: windows-smoke-mouse-back-check-runs-after-a-failed-load
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-smoke-mouse-back-check-runs-after-a-failed-load' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The observation note that tracked this RED leg was not closed. work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md still carries status: open, severity: red-ci-on-main and a Status section saying main currently has a RED windows-renderer leg. The immediately-preceding sibling task (macos-smoke-blur-url-bar-does-not-end-the-field-editor, commit 42a6657) closed its own note with status: closed plus closedBy. Should this note get the same closure so the next conductor is not told main is still red?
  (work/notes/observations/windows-smoke-mouse-back-check-is-sequenced-after-a-failed-load-2026-08-04.md frontmatter, unchanged in this diff)
- Ratify the timeout budget. Criterion 5 asked that the 30-second wait on the failure path be shortened; the back wait is now 10s, but the section added two NEW setup waits of 30s each (load_and_settle(..., 30)), so a failure in this section can now cost up to 40s of CI rather than 30s. The named regression class (the back move correctly refusing) is 10s, and the choice is recorded in the spike Decisions block. Is the 30s setup budget the intended trade?
  (window_smoke.rs, two_loads uses 30s per load; went_back uses 10s; docs/spikes/windows-smoke-mouse-back-check-runs-after-a-failed-load/README.md Decisions, entry 3)
- Ratify keeping the mouse section AFTER the negative control rather than moving it before, as the task allowed either. The section now self-establishes history (good), but it also leaves the window on a different page (second_cid, with a forward entry) for the COLLAPSE section that follows, so the smoke's sequence-coupling is only de-risked for this one section. The following watch_a_load_and_cancel_it still navigates to honest_cid, so it stays a cross-URL load and should be unaffected. Confirm no follow-up is wanted for the remaining sequence-coupled sections.
  (window_smoke.rs, the mouse section precedes the COLLAPSE section whose in_flight_url is ipfs://honest_cid/)
