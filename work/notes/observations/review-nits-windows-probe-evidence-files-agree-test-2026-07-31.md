---
title: review-gate non-blocking nits for 'windows-probe-evidence-files-agree-test' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-probe-evidence-files-agree-test
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-probe-evidence-files-agree-test' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the provenance asymmetry with the macOS twin: the Windows test asserts the recorded line names the runner label and the WebView2 runtime version, but NOT an actions/runs/ URL, so a reader still cannot jump to the actual CI run the way the macOS guard guarantees. The drift update asked for symmetry and for provenance that 'names the CI run so a reader can go and look'. The agent chose to assert only the evidence that exists rather than hand-edit expected.json (recorded in the module doc). Accept as-is, or file a follow-up to capture the run URL the next time the probe is re-run and re-stamped?
  (crates/windows-origin-probe/tests/recorded_verdict.rs: asserts recorded.contains(windows-latest) + the runtime version + the report filename; macos twin asserts recorded.contains(actions/runs/). expected.json 'recorded' line has no run URL, and neither does the spike README.)
- Ratify the test-count / coverage decision: the task asked for ONE test; the agent shipped two (diff-agreement + provenance) and deliberately DROPPED the macOS twin's two extra tests (mechanism-derives and control-really-failed) on the stated grounds that both checks already live inside this probe's Expectations::diff. I verified that claim is true. The one residual macOS check with no Windows equivalent is the subresource-reach comparison (case_a.handler_uris.len() > control.handler_uris.len()), which expected.json does not pin. Ratify, or ask for that one extra assertion?
  (facts.rs Expectations::diff contains both the negative-control falsification guard (serves_a_client_side_navigation on case_control) and the mechanism_from cross-check; macos recorded_verdict.rs carries them as separate tests.)
- The PR/commit carried no '## Decisions' block; the two decisions above are recorded only in the new file's module doc. That placement is arguably better for durability, but the human ratification surface is the PR description. Worth confirming the convention for future work legs.
  (git log c0b4b6e: single-line conventional-commit subject, no body.)
