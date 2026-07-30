---
title: review-gate non-blocking nits for 'windows-ipfs-origin-probe-on-ci' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: windows-ipfs-origin-probe-on-ci
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-ipfs-origin-probe-on-ci' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the measurement route: the verdict was measured in a throwaway GitHub repo created under the user's account (wighawag/werust-windows-origin-probe-scratch), an external side effect on a shared account that the task never authorised. It is disclosed honestly, but only in work/notes/observations/leftover-scratch-repo-from-windows-origin-probe-2026-07-30.md (an append-only bucket nothing routes), not in the spike DECISIONS.md, and it needs a HUMAN to delete the repo since the worker's token lacks delete_repo. Ratify the route and delete the repo.
  (work/notes/observations/leftover-scratch-repo-from-windows-origin-probe-2026-07-30.md; docs/spikes/windows-ipfs-origin-probe-on-ci/DECISIONS.md lists 5 decisions, none of them this one)
- The evidence chain is not re-checkable from this repo: probe-report-2026-07-30.json and expected.json carry no Actions run URL and no commit SHA of the code that produced them, and the scratch repo is now private+archived. The .github/workflows/windows-origin-probe.yml file itself and the whole cfg(windows) half (crates/windows-origin-probe/src/win.rs) have never been built or run inside werust. Merging to main does trigger the workflow via its path filter, so this self-verifies on merge: if that first in-repo run is red, the recorded verdict must be re-decided before windows-webview2-backend-and-window claims it.
  (workflow push paths include crates/windows-origin-probe/** and docs/spikes/windows-ipfs-origin-probe-on-ci/**; the Ubuntu verify gate compiles only the host-independent half)
- Nothing in the Ubuntu gate asserts the two COMMITTED evidence files against each other: no test loads docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json plus probe-report-2026-07-30.json and asserts Expectations::diff is empty. The pinned verdict and the verbatim run can therefore drift in a later edit and only a Windows runner would notice, which is the one runner this repo does not have on every run. A single host-independent test would close it, in the same spirit as the 23 pure tests already shipped.
  (crates/windows-origin-probe/src/facts.rs tests build Expectations in-memory only)
- Ratify pinning case B and the control as HARD assertions in expected.json: a change in the fallback mechanism werust does NOT use, or a change in the control's exact strings (origin null, fetch reject:TypeError, push_state throw:SecurityError), reddens the workflow even though the chosen mechanism still works. That is deliberate as a falsification guard, but it is a brittleness choice with a CI cost and it is not recorded in DECISIONS.md.
  (docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json; facts.rs Expectations::diff checks all three cases)
