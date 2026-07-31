---
title: review-gate non-blocking nits for 'windows-backend-error-mapping-and-leg-header-accuracy' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-backend-error-mapping-and-leg-header-accuracy
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-backend-error-mapping-and-leg-header-accuracy' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The new error message trails its detail after a colon, but three places claim it LEADS with the platform detail (the rustdoc on environment_creation_error, DECISIONS.md item 1, and the assertion message in the new pure test). Should the wording be corrected to 'carries' / 'keeps', or should the message actually be reordered to put the HRESULT first?
  (crates/windows-renderer/src/pure.rs: message is 'the ... Runtime is installed on this machine, but it refused to create the browser environment werust renders in: {detail}'; the rustdoc says 'names the operation and LEADS with detail'. Same doc-overclaims-the-tool class this task exists to close. Behaviour itself is correct and the test only asserts contains().)
- Two guard comments overclaim what the pins enforce. Should the macOS pin become an exact-set pin like the Windows one, or should the two comments be softened to say removal-pinned only?
  (macos_backend_shape.rs new comment says pinning desktop-paint means 'the next widening of either filter is an edit to a test rather than an accretion', but that test only asserts pull_request.contains(...) plus a two-entry deny list, so a NEW macOS PR-filter path still lands silently. Separately windows-renderer.yml says the list and the header 'neither can move without the other going red', yet no test holds the header prose. Only the Windows YAML list is truly exact-pinned.)
- Ratify: the Windows leg guard was converted from a must-have/must-not-have pair to an EXACT-set pin of the whole pull_request filter, which is broader than the criterion's ask to pin desktop-paint. Every future Windows task that adds a PR-filter path (e.g. windows-release-packaging-leg) now goes red until it edits PULL_REQUEST_FILTER. Keep?
  (crates/werust-core/tests/windows_renderer_leg_shape.rs PULL_REQUEST_FILTER + the rewritten the_pull_request_filter_stays_narrow_and_push_carries_the_rest; recorded in the new DECISIONS.md item 3. Intended effect, cheap to reverse, but it is a cross-task interaction.)
- Ratify: typecheck-windows-from-linux.sh was strengthened from cargo xwin check to cargo xwin clippy, so a developer's local inner loop can now fail on lints CI never ran on the cfg(windows) halves. The task allowed either this or softening the README; the agent did both. Keep the stronger harness?
  (docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh; recorded in DECISIONS.md item 4, re-run clean 2026-07-31. No CI leg runs the script, so the gate is unaffected.)
