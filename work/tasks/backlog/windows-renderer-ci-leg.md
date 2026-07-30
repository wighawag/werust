---
title: "Land the `windows-renderer` CI leg FIRST, so the Windows shell can be measured on a real runner instead of predicted"
slug: windows-renderer-ci-leg
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

Sub-task 1 of the Windows shell split (`windows-webview2-renderer-backend`, `windows-win32-window-and-chrome`), and it goes FIRST for a mechanical reason, not a technical one.

**Why this is its own task, ahead of the code.** Both macOS tasks were blocked at Gate 2 for shipping a PREDICTION where an acceptance criterion demanded a MEASUREMENT, and neither could have done better from inside the build: a build agent's work branch only reaches the arbiter when its run surfaces, so it cannot dispatch a workflow against its own code mid-build. `gh workflow run <wf> --ref <branch>` is only legal once `<wf>` exists on the DEFAULT branch. Landing this leg on `main` first is therefore what makes the two Windows code tasks measurable at all: their branches can be dispatched against a `windows-latest` runner by whoever drives them, exactly as `macos-renderer.yml` made the AppKit window provable (run 30572253620).

**What the leg does on the day it lands** (this must be GREEN on `main` from the first commit; do not land a workflow that only becomes correct later). There is no Windows shell code yet, so the leg proves the thing that IS provable today and is genuinely worth proving: that the repo's toolkit-free half compiles and passes its tests on `x86_64-pc-windows-msvc`. Concretely, build and test the crates that carry no GTK/WebKitGTK dependency on a `windows-latest` runner — at minimum `webview-shared` (which holds `LoadLifecycle`/`SharedLifecycle`, the `navigate` URL rule and the ADR-0008 off-thread `ipfs://` boundary that the Windows backend will reuse verbatim), plus `renderer`, `werust-core`, `fetcher` and `windows-origin-probe`'s host-independent half. DETERMINE the exact set empirically rather than trusting this list: if one of them does not build on Windows today, that is a FINDING worth recording (a `work/notes/observations/` note) and the leg ships with the set that is honestly green, naming what it excluded and why. A leg that is red on arrival teaches nothing.

Also record the runner's WebView2 Runtime version in a step, the way `windows-origin-probe.yml` already does: a platform result without its platform version is not a result. Reuse that workflow's registry-read step rather than inventing a second one.

**Shape, not scope.** Model it on `.github/workflows/macos-renderer.yml`: `workflow_dispatch` (the on-demand entry point, and the whole point of this task) plus a path-filtered `push` on `main` and a path-filtered `pull_request`. It is a SEPARATE leg from the pure-Rust Ubuntu `verify` gate, like `mobile-ios.yml`, `windows-origin-probe.yml` and `macos-renderer.yml`.

**Choose the `pull_request` path filter deliberately, and say why.** A live question the human raised about the macOS sibling applies here before the ink is dry: `macos-renderer.yml` triggers on PRs touching `crates/werust-core/**`, so most core work now spends `macos-14` minutes and can be gated by a red macOS leg. Do NOT copy that filter reflexively. Prefer the NARROWEST `pull_request` filter that still catches a real Windows-affecting change (the Windows crates, `crates/webview-shared/**`, the workflow itself), and record the trade-off in the workflow's own header comment: broad filters buy early detection at the cost of minutes and cross-platform gating; `workflow_dispatch` covers the deliberate case. If you disagree with narrow, say so in the comment with a reason.

**Pin the shape in the Ubuntu gate.** This repo already parses workflow files inside the pure-Rust `verify` gate (`crates/werust-core/tests/release_plumbing_shape.rs`). Add a test in that style asserting the leg exists, runs on `windows-latest`, and carries `workflow_dispatch` — the last one is load-bearing, because dispatch-by-ref is the entire reason this task exists, and a future edit that drops it would silently re-open the prediction trap. Do not add a dependency for YAML if the existing tests parse workflows some other way; follow whatever that file already does.

**Scope discipline:** one workflow file, one shape test, no shell code, no new crate. The Windows backend task EXTENDS this leg (adds its crate to the build/test steps and its trust-hooks smoke); it does not replace it.

## Acceptance criteria

- [ ] `.github/workflows/windows-renderer.yml` exists, runs on `windows-latest`, and has `workflow_dispatch` as an entry point.
- [ ] The leg is GREEN as landed: every crate it builds and tests really does compile and pass on `x86_64-pc-windows-msvc`, and any crate excluded from the set is named with the reason it was excluded.
- [ ] The run records the runner's WebView2 Runtime version, reusing the existing probe workflow's step rather than a second implementation.
- [ ] The `pull_request` path filter is the narrowest one that still catches a Windows-affecting change, and the workflow header states the trade-off that was chosen and why.
- [ ] A test in the Ubuntu `verify` gate (in the `release_plumbing_shape.rs` style) asserts the leg's existence, its `windows-latest` runner and its `workflow_dispatch` trigger, with no new dependency.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: land `.github/workflows/windows-renderer.yml` on `main` BEFORE any Windows shell code, so the two Windows code tasks can be dispatched against a real `windows-latest` runner by ref (`gh workflow run … --ref <branch>`), which is only legal once the workflow is on the default branch. That mechanic is exactly what blocked both macOS tasks at Gate 2 for shipping predictions instead of measurements. The leg must be GREEN THE DAY IT LANDS: there is no Windows shell yet, so build and test the toolkit-free crates that really do compile on `x86_64-pc-windows-msvc` (`webview-shared` above all, plus `renderer`, `werust-core`, `fetcher`, the probe's host-independent half) — determine that set empirically and name whatever you excluded. Record the runner's WebView2 Runtime version reusing `windows-origin-probe.yml`'s registry step. Model the file on `macos-renderer.yml`, but choose the NARROWEST `pull_request` path filter that still catches a Windows-affecting change and justify it in the header comment (the macOS leg's `crates/werust-core/**` PR trigger is under review for exactly this cost). Pin the shape — existence, `windows-latest`, `workflow_dispatch` — with a test in the `crates/werust-core/tests/release_plumbing_shape.rs` style, no new dependency. One workflow, one test, no shell code, no new crate.

## Requeue 2026-07-30

CONDUCTOR HANDOFF (2026-07-30, drive-tasks). Gate 2 blocked this correctly: acceptance criterion 2 demanded the leg be GREEN AS LANDED and the recorded evidence was a `cargo xwin check --tests` cross-sweep, which type-checks but does not link and runs ZERO tests. You could not have closed that from inside the build (`gh workflow run --ref` is refused while the workflow is absent from the default branch, and a PR could not run it either: GitHub cannot compute a merge ref, because the runner's own stuck-surface commits on main touch the very task file this branch moves backlog->done, PR #3). The conductor did it for you.

THE LEG IS GREEN, MEASURED TWICE, ON A REAL `windows-latest` RUNNER:

- main's tree, push-triggered by landing the workflow: run 30581522002 — SUCCESS.
- THIS BRANCH's tree, `workflow_dispatch --ref work/task-windows-renderer-ci-leg`: https://github.com/wighawag/werust/actions/runs/30581549437 — SUCCESS, all steps.

Verbatim from the branch run:

    WebView2 Runtime (registry HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}): 150.0.4078.65

Every crate in the leg's set BUILT and its tests RAN and PASSED on x86_64-pc-windows-msvc — 448 tests, zero failures, zero ignored:

    fetcher                              36 passed
    renderer                             20 passed
    webview-shared                        5 passed
    werust-core (lib)                   276 passed
    werust-core browser_menu_edge_wiring_shape        6 passed
    werust-core chrome_css_class_set_edge_wiring_shape 4 passed
    werust-core debug_capture_edge_wiring_shape        9 passed
    werust-core debug_view_desktop_wiring_shape        5 passed
    werust-core debug_view_mobile_wiring_shape         5 passed
    werust-core ipfs_redirects_fixture                18 passed
    werust-core platform_capability_parity             9 passed
    werust-core redirect_navigation_edge_shape         5 passed
    werust-core release_plumbing_shape                20 passed
    werust-core windows_renderer_leg_shape             7 passed
    windows-origin-probe (lib)           23 passed

Note what that settles beyond the criterion: the CRLF workaround works (every `*_shape.rs` test parses committed source and passed), the loopback-TCP and sleep-based tests in `fetcher`/`werust-core` pass on Windows, the temp-dir scratch handling in `retrieval.rs` passes, and `windows-origin-probe`'s 23 host-independent tests have now RUN on Windows for the first time. Those were exactly the runtime-only risks your README listed as unprovable by the sweep.

WHAT TO DO WITH THIS — re-stamp, do NOT re-derive, and do NOT relabel the sweep as a measurement:

1. `docs/spikes/windows-renderer-ci-leg/README.md`: the "What this measurement does NOT prove" section is now largely OBSOLETE. Replace the prediction framing with the RECORDING: both run URLs, the WebView2 Runtime version, the per-crate test counts above, and the date. Keep the `cargo xwin` sweep in the README as the METHOD that chose the crate set (it is still the honest provenance of the exclusions), but stop presenting it as the proof of green.
2. Keep the named exclusions and their reasons exactly as they are — `werust`/`webview-renderer` red on pkg-config, the cfg-gated-away platform crates, `native-renderer`/`script-engine` left out on cost. That reasoning was not disturbed by the run.
3. Anything the run genuinely did NOT exercise stays honestly listed (nothing in this leg drives WebView2 itself yet; that is the backend task's job).
4. HOUSEKEEPING, so your rebase is clean: the conductor landed `.github/workflows/windows-renderer.yml` and `.github/actions/webview2-runtime-version/action.yml` on `main` as commit c9e7430, as BYTE-IDENTICAL copies of your own files, purely to make the dispatch legal. They will merge without conflict. Do not revert them, do not duplicate them, and do not treat their presence on main as someone else's competing design — they are yours. Everything else (the shape test, the spike README, the done-move) still lands with this branch.
