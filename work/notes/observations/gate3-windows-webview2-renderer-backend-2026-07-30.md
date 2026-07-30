---
title: "Gate-3 verdict: windows-webview2-renderer-backend (APPROVE) — the third shell, measured on a real runner by the agent itself"
date: 2026-07-30
status: open
reviewOf: windows-webview2-renderer-backend
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main` as `5fff7fc`. Conductor's own diff-vs-criteria pass over 3,414 added lines: `crates/windows-renderer` (the `Renderer` impl over WebView2), its 723-line source-shape guard, the extended CI leg, and the spike.

**The headline, and it is a first for this repo: the build agent obtained its OWN runtime evidence.** Two `windows-latest` runs, dispatched by the agent with `gh workflow run windows-renderer.yml --ref <branch>` against a branch it pushed for the purpose. That is exactly what `windows-renderer-ci-leg` was landed first to enable, and it worked on the first try. The three-task pattern (agent predicts → Gate 2 blocks → conductor measures → requeue → re-dispatch) did not repeat. One round trip bought back, permanently, for every future Windows task.

## Independent verification of the evidence (do not take a run URL on trust)

A recorded run URL is a claim like any other, so I checked it rather than reading it:

- Runs [30584851232](https://github.com/wighawag/werust/actions/runs/30584851232) and [30585224388](https://github.com/wighawag/werust/actions/runs/30585224388) EXIST, both `workflow_dispatch`, both `conclusion: success`, both on `windows-latest`.
- Their head SHAs (`307694a`, `95f1188`) resolve. Diffing the MEASURED tree `95f1188` against what actually merged, restricted to `crates/` and the workflow, yields exactly ONE hunk: two comment lines added to `crates/windows-renderer/Cargo.toml` explaining why the `Win32_Graphics_Gdi` feature is needed. The measurement therefore applies to the code that landed, not to a since-edited ancestor.
- The temporary `ci/windows-webview2-renderer-backend` branch was DELETED after the runs, leaving no orphan on the remote. Good hygiene, and worth naming because the alternative (a stray branch nobody remembers) is a known failure mode.
- Post-merge, `main`'s own push-triggered run [30586166766](https://github.com/wighawag/werust/actions/runs/30586166766) is SUCCESS. The leg is green on `main` with the backend in it.

## Criteria, ticked against the merged diff

1. **`Renderer` over WebView2 on `x86_64-pc-windows-msvc`, NO trait widening.** MET. Compiled and LINKED on the runner (not merely parsed), and `the_renderer_seam_was_not_widened_for_windows` pins it on the Ubuntu gate.
2. **Own crate, no gtk4/webkit6, CONSUMES `webview-shared` rather than copying it.** MET, and guarded by `the_windows_backend_crate_depends_on_no_toolkit_and_consumes_the_shared_half`. This is the third consumer of the toolkit-free half, which is the outcome ADR-0011 finding 5 predicted; `webview-shared`'s own module doc was updated to say so, which is the right place for that fact to live.
3. **Lazy environment for the fixed scheme-name set, eager container HWND.** MET, exactly as prescribed, and guarded by `the_environment_is_created_lazily_so_schemes_can_be_registered_first`. A late registration is reported loudly on stderr rather than swallowed, because the seam returns unit and must not widen. The right call: state the contract rather than encode it by widening a trait.
4. **Both trust hooks on a real WebView2, with a negative control that FAILS.** MET, and this is the criterion that matters. From the run, verbatim: the page reported `origin: ipfs://bafkreih2auw…`, `secureContext: true`, `provider: object`, `chainId: 0x1`; posture `ContentVerified`; the control (bytes that do not hash to their CID) ended `Failed` / `UnverifiedOrigin`. A smoke where everything passes has measured nothing; this one can fail and the control did.
5. **Everything through the seam; SPA URL change via `add_SourceChanged` (`IsNewDocument == FALSE`) not an inference.** MET structurally (`the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired`), and the README is honest that a real `pushState` was WIRED but not DRIVEN. Correctly deferred to the window task, which has the ronan.eth-shaped test for it.
6. **Runtime-missing failure is honest, named, not a crash, tested where possible.** MET as far as it can be: the `windows-latest` image HAS the runtime, so the path is covered by a pure unit test on the Ubuntu gate and the README says plainly that it is unmeasured at runtime. See nit 1 below, which is a real (small) defect inside this otherwise-met criterion.
7. **The leg is extended and a real run against this branch is recorded, no prediction dressed as a measurement.** MET, verified above.
8. **CI-versus-hardware stated explicitly (ADR-0011 Amendment 1).** MET, and this is the best version of that section the repo has produced: it distinguishes "a real Windows with a real evergreen runtime" from "a desktop with a display, a GPU and a user in front of it", and lists seven specific unproven things rather than one hand-wave. `the_verification_honesty_is_recorded` even guards that the section keeps existing.
9. **Ubuntu `verify` green, cfg-gated half covered by source-shape tests.** MET.

**The planted forward-note WAS honoured.** I planted the `GREEN_ON_WINDOWS` coupling in the task body at Gate-3 of the CI leg; the diff updates the constant, the build steps, the test steps and the push filter together, and extends the constant's doc comment to explain WHY extension is deliberately not a one-line edit. That note cost a minute to plant and saved a red gate.

## Review-nit triage (4 raised at Gate 2, all non-blocking)

- **Nit 1, `missing_runtime_error` is used for EVERY environment-creation refusal.** REAL, KEEP, and folded into a follow-on task. The constructor has already proved the runtime is present via `GetAvailableCoreWebView2BrowserVersionString`, so a corrupt user-data folder, a policy block or a version refusal all tell the user to "install the Evergreen Runtime" — advice that cannot help, with the real HRESULT surviving only in a trailing parenthetical. It is a small, contained fix (map the post-presence-check failure to a plain `RendererError::Backend`), and it matters because honest failure is a product value here, not a nicety.
- **Nit 2, the widened `pull_request` filter with a now-STALE header.** This is the one that bites. The widening itself is CORRECT and I ratify it: `crates/windows-renderer/**` is genuinely Windows-shaped, and gating its PRs on the Windows leg is exactly the discrimination the narrow-filter decision was making. But the header still reads "WHY THE `pull_request` FILTER IS NARROW" and still calls `windows-origin-probe` "the only Windows-specific code in the tree", which is now false in the same file that disproves it. My forward-note asked explicitly that a widening be justified in the header, so this is a planted-note MISS — the only one in the diff. It merged (there is no PR to block in `--merge` mode), so it is recorded here and carried into the follow-on task. Also unrecorded: `docs/spikes/windows-webview2-renderer-backend/**` joined the PUSH filter, so a docs-only edit now burns a `windows-latest` run.
- **Nit 3, `os_color_scheme()` + an HKCU read living in the ENGINE crate for the sibling's benefit.** RATIFIED, keep. The engine follows the OS itself via `PREFERRED_COLOR_SCHEME_AUTO` and does not need the reader, so it is built-for-the-sibling — but it is 30 lines, it is pure-tested on the Ubuntu gate, and the alternative (the chrome task adding a registry read to a window crate) is worse: the platform-detail read belongs with the platform bindings. Recorded here because it was NOT in `DECISIONS.md` and should have been.
- **Nit 4, the `%TEMP%\werust-webview2` default profile with an unenforced hand-off.** ACTED ON. A temp WebView2 profile silently inherited by the shipping shell would lose cookies, storage and cache on every reboot, which is a user-visible bug that would be found late and diagnosed slowly. I added an explicit acceptance criterion to `work/tasks/backlog/windows-win32-window-and-chrome.md` requiring a durable `%LOCALAPPDATA%` path, so it is now enforced by that task's own gate rather than by a sentence in a `DECISIONS.md` nobody re-reads.

## What this unlocks

`windows-win32-window-and-chrome` is unblocked and, crucially, MEASURABLE the same way. Windows is now the fourth edge with a qualifying `Renderer` (WebKitGTK, WKWebView on iOS and macOS, WebView2), and the third consumer of `webview-shared` — the extraction has paid for itself. Still open, deliberately: the `windows` parity-matrix column and a Windows packaging leg, both to be cut after the window lands so their cells describe what really shipped.
