---
title: "Land the `windows-renderer` CI leg FIRST, so the Windows shell can be measured on a real runner instead of predicted"
slug: windows-renderer-ci-leg
blockedBy: []
covers: []
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
