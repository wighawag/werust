---
title: "Gate-3 verdict: android-apk-version-from-the-release-tag (APPROVE) — one version source, and one door left open"
date: 2026-07-31
status: open
reviewOf: android-apk-version-from-the-release-tag
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. 194 lines of new Gradle version resolution, 246 lines of shape assertions, a README section on the debug-to-signed transition, and a decisions block.

## What I measured

The new code is 194 lines of Kotlin that execute at Gradle CONFIGURATION time and never run on the Ubuntu `verify` gate, so a shape test reading the file is weak evidence that it WORKS. I fired the release dry run: **[30598391420](https://github.com/wighawag/werust/actions/runs/30598391420), all five jobs SUCCESS, `android-apk` included.** Since the new resolution runs on every Gradle invocation, a green `android-apk` job proves the whole chain configures, resolves and builds without throwing on the no-tag path.

**What that does NOT prove, stated plainly:** a `workflow_dispatch` run has no tag, so `WERUST_VERSION` is empty and the run exercised the FALLBACK chain (`git describe`, then the workspace Cargo version), not the tag path. The tag path is pinned by the shape test and by the decisions, and it will first really execute on the next `v*` tag. That is inherent to a release-tag feature and worth knowing rather than glossing.

## Criteria, ticked

1. **A tagged release produces an APK whose `versionCode` derives from the tag and increases; `versionName` is the same string the Rust core reports.** MET by construction and by the shape guard, measured only on the fallback path (above). The `major * 10000 + minor * 100 + patch` fold is implemented as prescribed, with the CI-run-number alternative named and rejected for the right reason (it destroys the correspondence between the APK's version and the release it came from).
2. **The mapping reads the EXISTING version source; no second source is introduced.** MET in the sense that matters: the same INPUTS in the same precedence (`WERUST_VERSION`, else `git describe --tags --always`, else the workspace Cargo version) that `crates/werust-core/build.rs` uses. It is a second IMPLEMENTATION (Kotlin) of one SOURCE, and the task's decision 6 says so honestly rather than claiming more.
3. **The dev-build path keeps working.** MET, and treated as a first-class requirement rather than an afterthought: every lookup is failure-tolerant, and the comment states the principle ("a dev APK with a placeholder version is a far better outcome than a dev build that fails").
4. **The debug-to-signed uninstall transition is documented.** MET, in `crates/werust-android/README.md`.

## The one finding worth a task, not a shrug

**A pre-release tag reintroduces the exact bug this task removed.** `release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is an acceptable release tag. On it, `versionName` resolves correctly to `0.3.0-rc1`, but `versionCodeOf()` returns null for a non-triple and the code falls back to `devPlaceholderVersionCode = 1`. A SIGNED release APK would ship `versionCode = 1`: unsequenceable, un-updatable — the precise condition the task existed to remove, reachable through the front door.

It is also internally inconsistent, which is what convinced me it is a defect rather than a policy: decision 5 argues that a placeholder on a RELEASE APK is the worst outcome, and already fails the build loudly when a component exceeds 99. A non-triple tag deserves the same and gets the opposite.

Cut as `android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1`, with the fix prescribed as dev-versus-release (fail loudly when `WERUST_VERSION` is set and does not fold; keep today's tolerant placeholder when it is not) and with the genuine policy question — does this project ever want pre-release tags, and what mapping would sequence them? — recorded for the human instead of invented. No impact today: every existing tag is a clean triple.

## Review-nit triage (6 raised, all non-blocking)

**Acted on:** the pre-release tag defect (above), and the over-broad criterion-10 guard, which forbids `versionCode`/`versionName`/`git describe` ANYWHERE in the `android-apk` job — including in a future explanatory COMMENT. That would red the gate for a non-defect, in a repo whose whole habit is a comment explaining WHY next to the thing. Both folded into the new task.

**Ratified:**

- **A configuration-time `GradleException` for a component > 99** breaks every invocation of the module, not just a release build. Ratified: it is unreachable at any version this project will plausibly cut, and failing at configuration time is the loud behaviour the decision wanted.
- **The resolution is MIRRORED in Kotlin rather than read back from the compiled core** (unlike the macOS leg's `print_version` readout). Ratified with the caveat recorded: the inputs are shared so no second SOURCE exists, but nothing FAILS if `build.rs`'s precedence later changes — the shape test asserts the Gradle text mentions the same inputs, not that the two chains agree. A drift here would be silent. Worth knowing; not worth a task while the two are ten lines apart in intent.
- **`cargoBuildRustCore` now takes the resolved version as an `@Input`**, so a changed version re-runs the cross-compiles that were previously up-to-date. Found on a real build where the manifest said `0.3.0` while the packaged `.so` still reported `0.2.9-91-g…`. That is a genuine correctness fix the task discovered while working, and it is the kind of thing only a real build finds.
- **A no-git source tree resolves the workspace Cargo version rather than the placeholder**, so an untagged tarball build gets `versionCode 209` — the same code the real `v0.2.9` release will carry. A minor deviation from the acceptance wording, recorded in the spike's local-verification table, and low impact because such builds are debug-signed (an uninstall is needed anyway).
