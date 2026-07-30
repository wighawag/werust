---
title: "Gate-3 conductor review: versioned-gtk-app-id-and-stale-process-detection (APPROVE)"
date: 2026-07-30
status: open
reviewOf: versioned-gtk-app-id-and-stale-process-detection
verdict: approve
---

## Verdict: APPROVE

Merged as `f62a060` on `origin/main` (drive-tasks, `--allow-backlog --review --merge`, `etherplay/opus-5`). Gate-1 (the repo `verify`) and Gate-2 (the review gate, 5 non-blocking nits) both green. 21 `werust` desktop tests re-run locally green, including the two new ones.

## Acceptance criteria, ticked against the merged tree

- [x] **`com.github.wighawag.werust.v0_2_9` on a v0.2.9 release, dots to underscores.** `app_id()` in `crates/werust/src/main.rs`; asserted by `the_application_id_carries_the_version_so_two_releases_never_share_a_process`.
- [x] **The test's own `com.github.wighawag.werust.test` id is unchanged.** Still hard-coded in the display-backed test; it never read `APP_ID`.
- [~] **Launching v0.2.9 while v0.2.8 runs opens a NEW window.** Verified BY PROXY, not end to end: a headless `Gio.Application` probe (`docs/spikes/.../app-id-uniqueness.py`) shows the new version registering as primary rather than being handed off, and two real binaries were built and confirmed to bake distinct versions via their banners. The two-window launch was deliberately not run (it would open windows on the operator desktop). Disclosed honestly in the spike README. Accepted: the probe isolates the single rule (bus-name uniqueness) that decides the hand-off.
- [x] **Intra-version single-instance preserved.** `app_id` is a pure function of the version, so a second copy of the same release still activates the running window; asserted.
- [x] **The id uses the SAME `werust_core::version()` the menu and banner read.** Asserted by checking the banner contains the same version the running id was built from, so no second version source can drift.
- [x] **A test asserts the constructed id.** Two, in fact: the exact-shape test and a validity test that runs every id through `gio::Application::id_is_valid`, including a negative case proving the naive splice (`…werust.0.2.9`) is rejected.
- [x] **No new IPC, no auto-kill, no cache-dir explosion.** No D-Bus property, no process killing. The cache-dir premise turned out to be wrong in an interesting way, see below.

## Nit triage (5 non-blocking findings)

Two need a human decision, one is a worthwhile follow-up, two are cosmetic.

**Needs your ratification: the sanitisation is WIDER than the task prescribed.** The task said "replace dots with underscores"; `app_id()` folds every character outside `[A-Za-z0-9-]` to `_` and prefixes the element with `v`. The rationale is sound and recorded in the spike README: the build-time version is not always a release triple (a dev build is `git describe` output, and an operator can inject an arbitrary `WERUST_VERSION`), and an invalid application id fails SILENTLY by dropping uniqueness, which is the exact failure this task exists to remove. I accept it; it needs your nod because it exceeds the prescription.

**Needs your decision: a NEW consequence the task did not anticipate.** The task assumed per-version cache dirs would accumulate under `~/.cache/com.github.wighawag.werust.v0_2_*`. That premise is wrong: nothing configures a WebKit data dir (`WebContext::new()` with the default session), so WebKit/GTK storage is keyed on `prgname` (`werust`), not the application id. Good: no per-version profile fork, no cookie loss on upgrade. New: two DIFFERENT versions can now run CONCURRENTLY against the SAME cookie/localStorage/cache store, which was impossible before (the old process always took the session). Concurrent shared-store access across versions is a real behaviour change; it wants either an explicit "acceptable" or a follow-up task.

**Worth a follow-up: nothing pins the production call site.** Both new tests exercise the pure `app_id()` function; none asserts that `main()` passes `werust_core::version()` into it. A future edit back to a constant id would restore the exact stale-process trap with a green suite. The repo already has the cheap mechanism (source-reading shape tests such as `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs`). Criterion 5 allowed "a unit test OR a shape test", so this is not a miss against the gate, but the guard is one line and the trap is the whole point of the task.

**Cosmetic:** the `app_id` doc comment says the id is valid "by construction", but only the character class is guaranteed, not the 255-character application-id limit (reachable only by an absurd injected `WERUST_VERSION`). And criterion 2's proxy verification, already covered above.

## Context

The follow-up named in the previous conductor note (`gate3-loading-banner-with-phase-and-cancel`, "widen the visibility predicate so the ENS-resolution window is covered") has since landed in `5ed6fb6`, which also moved load progress off the displacing banner and into the URL bar.
