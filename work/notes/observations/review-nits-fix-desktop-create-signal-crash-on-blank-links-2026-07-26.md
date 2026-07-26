---
title: review-gate non-blocking nits for 'fix-desktop-create-signal-crash-on-blank-links' (Gate 2 approve)
date: 2026-07-26
status: open
reviewOf: fix-desktop-create-signal-crash-on-blank-links
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'fix-desktop-create-signal-crash-on-blank-links' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The done record's TITLE still prescribes the superseded mechanism: the frontmatter title of work/tasks/done/fix-desktop-create-signal-crash-on-blank-links.md says route new-window via decide-policy instead, and the merge commit subject repeats it, but the requeue (and the code) chose the raw create-signal-returns-NULL route. Should the title be corrected so the durable done record does not misdescribe what shipped?
  (work/tasks/done/fix-desktop-create-signal-crash-on-blank-links.md frontmatter title vs the Requeue 2026-07-26 block and crates/webview-renderer/src/backend.rs:428)
- Bookkeeping: the task landed in tasks/done/ still carrying needsAnswers: true, and its stuck sidecar work/questions/task-fix-desktop-create-signal-crash-on-blank-links.md (15 duplicated entries, allAnswered=false) is still present on the branch. Per WORK-CONTRACT a resolved item clears needsAnswers and drains the sidecar. Is that the runner's integration step, or should it be cleaned here?
  (git diff main...HEAD shows a 100%-similarity rename into tasks/done/ and no change to work/questions/)
- The only red/green guard for the actual crash is #[ignore]d and, per the new observation note, cannot run alongside the other ignored tests (GTK init once per process), so it must be invoked filtered by name and CI never exercises it. Ratify that a display-bound, manually-run guard plus the display-free routing unit tests is the accepted coverage for this crash class.
  (crates/webview-renderer/src/lib.rs real_webview_new_window_requests_load_in_place_without_aborting; work/notes/observations/ignored-gtk-tests-cannot-share-one-test-process-2026-07-26.md)
- Silent fallthrough in the raw handler: if args.get(1) or the NavigationAction downcast ever fails, target is None, new_window_action returns Ignore, and the hook answers NULL with no load and no log, i.e. the dead-link behaviour of field finding C returns invisibly. Worth a debug log or a comment naming that failure mode?
  (crates/webview-renderer/src/backend.rs:429-440 (args.get(1).and_then(...).ok()))
- The in-place load calls view.load_uri directly, so it skips the seam's validate_url that Renderer::navigate applies; refusal of an unsupported/malformed target relies solely on WebKitGTK. This is pre-existing (unchanged by this fix) but the trust claim in the acceptance criteria reads as if the full navigate path is used.
  (backend.rs install_new_window_in_place vs Renderer::navigate at backend.rs:690 (validate_url))
