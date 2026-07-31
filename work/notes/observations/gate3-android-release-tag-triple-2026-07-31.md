---
title: "Gate-3 verdict: android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1 (APPROVE) — the front door is shut"
date: 2026-07-31
status: open
reviewOf: android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. The latent release defect is closed: a `v0.3.0-rc1`-shaped tag can no longer ship a signed APK with `versionCode = 1`.

## Criteria, ticked

1. **A non-triple with `WERUST_VERSION` set FAILS, naming the version, the reason and the accepted shape.** MET.
2. **With `WERUST_VERSION` unset, an untagged local build still succeeds on the placeholder.** MET — the dev/release split is the right axis, and it keeps `./gradlew :app:assembleDebug` working for someone with no tags at all.
3. **Covered by a test in the ordinary gate, not only prose.** MET, though see residue 1: the dev-path half of that coverage is currently toothless.
4. **The pre-release SEQUENCING mapping is deliberately NOT designed**, and the README records what would have to be decided if it is ever wanted. MET, and this was the important restraint. Inventing an `rc` fold here would have quietly committed the project to a release policy nobody has chosen.

## Residues, cut as `android-version-guard-teeth-and-the-stale-daemon-trap`

- **The dev-path guard cannot fail.** `a_local_untagged_android_build_keeps_working_on_a_placeholder` asserts the binding CONTAINS `devPlaceholderVersionCode` — but that identifier now also appears interpolated inside the new `GradleException` message, so deleting the tolerant fallback entirely would leave the test GREEN. That is the single regression it exists to catch. A guard that cannot fail is worse than no guard, because it is believed, and this drive has already seen two other "prove the teeth" moments; this one skipped it.
- **A stale Gradle daemon now hard-fails a local build.** The new failure fires on ANY `WERUST_VERSION` in the daemon's environment, and `build.gradle.kts` ALREADY documents that a reused daemon carries an earlier shell's value until `./gradlew --stop`. So a developer who once ran a tagged build can hit a hard failure in a fresh shell having set nothing, and the message tells them to tag a clean triple, which will not help. The failure should stay loud; it just has to name the cause they are actually in.
- Two wording items: `versionCodeOf`'s KDoc still states the pre-change placeholder rule, and the fold-to-zero message claims only a clean triple folds, which `v0.0.0` is (it is rejected by `takeIf { it > 0 }`).

## Note on scope

I did not fire a release dry run for this one. The previous dry run already proved the Gradle configuration path executes end to end, and this change's LOAD-BEARING behaviour is the tag path, which a `workflow_dispatch` run cannot exercise (it has no tag). Running one would have re-measured the fallback path and told us nothing new, so the honest position is: the tag path lands unexercised until the next real `v*` tag, and the guard plus the shape test are what stand behind it until then.
