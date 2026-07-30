# Judgement calls made while building the Windows origin probe

Task: `windows-ipfs-origin-probe-on-ci`. Verdict + evidence: [`README.md`](README.md). Decision it closes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md) step 0.

These are the calls a reviewer or a human should be able to reverse without re-deriving them. One entry per decision: what was chosen, why, what was rejected, and what it touches.

## 1. A third case — a NEGATIVE CONTROL — was added, beyond the two the task named

**Chosen:** run a third case alongside A and B: the same registered `ipfs://` scheme, the same URL, the same canned bytes and the same page, with exactly one registration flag flipped (`HasAuthorityComponent = false`, and with it `TreatAsSecure`, which Microsoft documents as effective only alongside it). It measures no mechanism and can never decide the verdict, but it is ASSERTED to fail, and a run where it PASSES is failed as non-discriminating.

**Why:** case A passed every check on the first run. A probe in which every case passes is indistinguishable from a probe that cannot fail, and this repo's whole reason for putting a probe before the Windows shell is that it already paid once for a platform-origin belief that turned out to be wrong (`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`). Replacing "case A passed" with "case A passed and the one-flag-different control reproduced the Android bug verbatim, minutes apart, on the same runner" is the difference between a green light and evidence. The control did reproduce it, down to Blink's exact sentence.

**Rejected alternatives:** (a) run the control ONCE by hand and quote it in the README — cheaper, but it makes the falsification a dated anecdote rather than something every re-run re-establishes, and re-runs are the point (the runtime is evergreen); (b) no control at all, i.e. exactly the two cases the task named — which is what the task says, but it would leave the strongest single piece of evidence in the report unobtained for no saving worth having.

**What it touches:** the probe's `CaseId` (three variants, not two), `expected.json` (a third pinned block), and the report shape. Nothing outside `crates/windows-origin-probe` and this spike. "Control" is a new named concept in this repo; it is the ordinary experimental sense of the word, it sits inside the probe rather than in the product vocabulary, and it does not overlap or re-mean any term in `CONTEXT.md`.

## 2. The probe lives in `crates/`, not in this spike directory

**Chosen:** the probe is a normal workspace crate, `crates/windows-origin-probe`, with its WebView2 half behind `#[cfg(windows)]` and its bindings behind `[target.'cfg(windows)'.dependencies]`. This spike directory holds the VERDICT, the pinned expectations and the verbatim measurement.

**Why:** it follows the repo's own precedent rather than inventing one. The Android probe is committed in the platform's source tree (`crates/werust-android/app/src/androidTest/.../SpaClientNavOriginTest.kt`) while its verdict lives in `docs/spikes/`; the target-gated-dependency shape is copied verbatim from `crates/werust-android/rust`, which keeps `jni` Android-only for exactly the same reason. Being a workspace member also means the Ubuntu `verify` gate compiles the crate and runs the 23 unit tests over its host-independent half (the decision rule, the canned site, the CLI) on every ordinary run — the "source-shape tests in the gate plus a real platform job" pattern the research spike (section 7) prescribes for Windows code. A crate parked under `docs/` would have got none of that.

**Rejected alternatives:** (a) a standalone non-workspace crate under `docs/spikes/windows-ipfs-origin-probe-on-ci/probe/` — no gate coverage, no shared `Cargo.lock`, no `cargo fmt`/`clippy`; (b) naming it `werust-windows` — that squats the name the future shell wants, and this is explicitly NOT shell code.

**What it touches:** the workspace member list, `Cargo.lock` (which gains the pinned `webview2-com` / `windows` graph), and the future Windows shell task, which now has a working, compiling `webview2-com` call-site to copy from.

## 3. The workflow is on-demand and path-filtered; it is NOT scheduled

**Chosen:** `.github/workflows/windows-origin-probe.yml` triggers on `workflow_dispatch` (the on-demand entry point the acceptance criterion asks for) and on pushes to `main` that touch the probe or its recorded verdict. No `schedule:` cron.

**Why:** the acceptance criterion is "re-runnable on demand (a workflow entry point, not a one-off local transcript)", which `workflow_dispatch` satisfies exactly. A weekly cron would additionally WATCH the evergreen runtime, which is genuinely attractive given WebView2Feedback #5495 — but it is a standing CI cost and a standing red-alert channel on a repo that has neither today, and it would fire against `main` regardless of whether anyone is working on Windows. That is a decision for whoever funds the Windows shell, not a side effect of gate 0.

**Rejected alternative:** a weekly `schedule:` cron. Reversing this is a three-line change to the workflow, so it is deliberately left cheap.

**What it touches:** CI cost and the Windows shell task, which may well want the cron once a shell actually depends on the verdict.

## 4. The canned responses deliberately carry NO `Access-Control-Allow-Origin`

**Chosen:** the scheme handler answers with `Content-Type` and nothing else.

**Why:** an `Access-Control-Allow-Origin: *` would let an opaque origin's request through, which is precisely the failure the probe exists to detect — it would have turned the negative control green and made the whole run meaningless. Withholding it is what makes "the fetch resolved" mean "the fetch was same-origin".

**What it touches:** the Windows shell, which will face the real version of this question when it serves actual content. It is not a shell decision this probe makes; it is only a statement about what the probe measured.

## 5. Each case runs in its OWN PROCESS

**Chosen:** the normal invocation re-executes itself once per case (`--case a|b|control`), each with its own user-data folder, and aggregates the three JSON facts lines.

**Why:** WebView2 fixes the SET of custom scheme registrations at environment creation and makes it immutable for the browser-process lifetime; every environment sharing a browser process must register an IDENTICAL set or creation fails (ADR-0011 finding 5). The three cases register three different sets. In one process, the second and third cases' environment creation would have depended on how WebView2 happened to reuse a browser process — a confound sitting in the middle of the experiment. Separate processes make the isolation total and obvious.

**What it touches:** nothing outside the probe. The shell's version of this constraint is already answered by ADR-0011 (create the environment lazily); this is not it.
