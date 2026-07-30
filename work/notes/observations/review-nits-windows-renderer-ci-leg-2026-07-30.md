---
title: review-gate non-blocking nits for 'windows-renderer-ci-leg' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: windows-renderer-ci-leg
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-renderer-ci-leg' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify D2: the WebView2 registry read was lifted into a shared composite action, which required editing a SECOND workflow (.github/workflows/windows-origin-probe.yml) that the task's scope line (one workflow file, one shape test) did not cover. The lift is verbatim and the shape guard now asserts the GUID lives in the action and in NEITHER workflow, so gate-0's leg is coupled to this task's test. Is the human happy with that scope widening and the cross-leg coupling?
  (docs/spikes/windows-renderer-ci-leg/README.md D2; .github/actions/webview2-runtime-version/action.yml; probe workflow diff replaces the inline pwsh block with uses: ./.github/actions/webview2-runtime-version; windows_renderer_leg_shape.rs::the_registry_read_exists_in_exactly_one_place)
- Ratify D1: the pull_request filter deliberately omits crates/werust-core, crates/fetcher and crates/renderer, so a core change that breaks the Windows build is found only after it merges (push on main) or on demand. The test pins both halves so broadening is a deliberate edit. Accept the stated trade (no cross-platform gating of core PRs, minutes saved) or mirror the macOS leg?
  (.github/workflows/windows-renderer.yml on.pull_request.paths vs on.push.paths; windows_renderer_leg_shape.rs::the_pull_request_filter_stays_narrow_and_push_carries_the_rest)
- Ratify D3: the leg runs git config --global core.autocrlf false before checkout instead of adding a repo-wide .gitattributes or making the *_shape.rs tests line-ending agnostic. It is one runner-local step and the run proves it is sufficient (all 61 source-parsing shape tests passed on Windows), but the underlying CRLF fragility of those tests remains unowned for any future Windows job that forgets the step. Leave as is, or open a follow-up?
  (windows-renderer.yml first step; README D3, alternatives (a) .gitattributes and (c) CRLF-tolerant tests both rejected as out of mandate)
- Cross-task interaction worth naming for the next Windows task: windows_renderer_leg_shape.rs hard-codes GREEN_ON_WINDOWS and asserts that EVERY crate the leg builds also appears in the push path filter. So windows-webview2-renderer-backend cannot just add its crate to the workflow; it must update the constant and the push filter in the same change or the Ubuntu gate goes red. Intended (extension must be deliberate), but it is not called out in the task file or the README decisions.
  (crates/werust-core/tests/windows_renderer_leg_shape.rs GREEN_ON_WINDOWS and the_pull_request_filter_stays_narrow_and_push_carries_the_rest; task text: the backend task EXTENDS this leg)
- Housekeeping only: the task file lands in work/tasks/done/ still carrying needsAnswers: true, and work/questions/task-windows-renderer-ci-leg.md keeps five unanswered stuck entries quoting the now-closed Gate-2 block. Precedented (five other done tasks carry needsAnswers: true), and routing is the caller's job, but a stale sidecar can keep this item looking like it needs attention after merge.
  (work/tasks/done/windows-renderer-ci-leg.md frontmatter; work/questions/task-windows-renderer-ci-leg.md Q1-Q5)
