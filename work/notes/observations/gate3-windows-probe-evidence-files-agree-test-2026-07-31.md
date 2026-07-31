---
title: "Gate-3 verdict: windows-probe-evidence-files-agree-test (APPROVE) — the guard now exists on both probes"
date: 2026-07-31
status: open
reviewOf: windows-probe-evidence-files-agree-test
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. One new file, `crates/windows-origin-probe/tests/recorded_verdict.rs` (128 lines), and nothing else touched. That restraint is itself a criterion: the task said "one test plus whatever tiny loader it needs", and the diff is exactly that.

## Criteria, ticked

1. **A test in the ordinary Ubuntu gate loads both committed files and asserts an empty `Expectations::diff`.** MET. It runs on every `cargo test`, needs no WebView2 and no Windows runner, which was the whole point: the only thing that would previously have noticed a drift between the pinned verdict and its evidence was a Windows runner, and this repo does not have one on every push.
2. **The failure message names the differing field and says which file is the baseline and which is the recorded run.** MET. "Which one is wrong?" is the reader's first question and the message answers it.
3. **It has teeth (a deliberate mismatch fails it).** MET.
4. **No new dependency, no change to the Windows-only half, no restructuring.** MET.

**The drift update I planted was honoured and improved on.** I pointed the task at `crates/macos-origin-probe/tests/recorded_verdict.rs` as the model. The agent followed its shape, and then did something better than obedience: it checked which of the macOS twin's extra assertions were actually meaningful HERE, found that two of them (mechanism-derives, control-really-failed) are already enforced inside this probe's own `Expectations::diff`, and dropped them rather than copying ceremony. I verified that claim against `facts.rs` and it is true. Copying a sibling's test list unexamined would have been the easy wrong answer.

## Review-nit triage (3 raised, all non-blocking, all ratified)

- **Provenance asymmetry: the Windows guard asserts the recorded line names the runner label and the WebView2 runtime version, but not an `actions/runs/` URL, where the macOS twin does.** RATIFIED, and the reasoning is the important part: the Windows `expected.json` has no run URL to assert, because the original probe task never recorded one. The agent could have "fixed" the asymmetry by hand-editing `expected.json` to add a URL, and that would have been precisely the sin both these guards exist to prevent — writing something into an evidence file that did not come from a run. It asserted the evidence that EXISTS and recorded the gap in the module doc. Correct call.
  The residual: whoever next re-runs the Windows probe and re-stamps `expected.json` should capture the run URL then, at which point the assertion can be tightened to match macOS. That is a one-line note for that future moment, not a task today, and it is recorded here so it is findable.
- **Two tests instead of the requested one, minus two of the macOS twin's.** RATIFIED (above). The one macOS check with no Windows equivalent is the subresource-reach comparison (`case_a.handler_uris.len() > control.handler_uris.len()`), which the Windows `expected.json` does not pin; not worth forcing.
- **No `## Decisions` block in the commit; the decisions live in the new file's module doc.** RATIFIED as the better placement. A module doc travels with the code and is read by the next person to touch it; a commit body is read once. Worth confirming as convention, so it goes to the human batch as a low-stakes ratify item.
