---
title: "Give the dev-path version guard its teeth back, and stop a stale Gradle daemon hard-failing a local build with advice that cannot help"
slug: android-version-guard-teeth-and-the-stale-daemon-trap
blockedBy: [android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1]
covers: []
---

## What to build

Four residues of `android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). Two matter; two are wording. All four live in two files.

**1. The dev-path guard no longer guards.** `crates/werust-core/tests/release_plumbing_shape.rs`'s `a_local_untagged_android_build_keeps_working_on_a_placeholder` asserts only that the `versionCode` binding CONTAINS the identifier `devPlaceholderVersionCode`. But that identifier now also appears interpolated inside the new `GradleException` message. So if a future change deleted the tolerant fallback and always threw, the test would still PASS — the single regression it exists to catch. A guard that cannot fail is worse than no guard, because it is believed.

Assert the ELSE-BRANCH SHAPE instead (the trailing fallback expression), not the bare identifier. Then prove it has teeth the way this repo proves its other guards: delete the fallback locally, watch the test go red, restore it, and say in the commit that you did.

**2. A stale Gradle daemon now hard-fails a local build, and the message sends the developer the wrong way.** The new failure is not confined to CI: ANY `WERUST_VERSION` in the Gradle daemon's environment triggers it. `build.gradle.kts` ALREADY documents (in `resolveWerustVersion`'s KDoc) that a reused daemon can carry the `WERUST_VERSION` of an EARLIER shell until `./gradlew --stop`. So a developer who once ran a tagged build can hit this hard failure in a fresh shell having set nothing at all — and the message only advises tagging a clean triple, which will not help them.

Add the stale-daemon path to the message (naming `./gradlew --stop`) and to the spike README's decision 8, whose second consequence bullet records the hand-set case but not this one. The failure itself is right and should stay loud; it just has to name the cause a developer will actually be in.

**3. The `versionCodeOf` KDoc is stale.** It still says a pre-release tag or an operator's named build "must take the dev placeholder instead of folding", which is precisely what no longer happens when `WERUST_VERSION` is injected. A reader of that function alone gets the pre-change rule. One-line amendment pointing at the `werustVersionCode` binding where the new distinction lives.

**4. A small message inaccuracy.** A triple that folds to 0 (`v0.0.0`) fails with text saying only a clean `major.minor.patch` triple folds — misleading, because `v0.0.0` IS one; it is rejected by the `takeIf { it > 0 }` guard. Nobody will tag `v0.0.0`, so this is noted only so the wording becomes a deliberate choice rather than an accident.

**Scope:** one assertion tightened and proven, one error message and one README bullet extended, two doc corrections. No change to the mapping, the fail-loud behaviour, or the dev-build path.

## Acceptance criteria

- [ ] `a_local_untagged_android_build_keeps_working_on_a_placeholder` FAILS if the tolerant fallback is removed (proven once during development, and stated).
- [ ] The `GradleException` message names the stale-daemon cause and `./gradlew --stop`, not only "tag a clean triple".
- [ ] The spike README's decision 8 records the stale-daemon path beside the hand-set case.
- [ ] `versionCodeOf`'s KDoc no longer states the pre-change placeholder rule.
- [ ] The fold-to-zero message says what is actually rejected.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: four residues in `crates/werust-android/app/build.gradle.kts` and `crates/werust-core/tests/release_plumbing_shape.rs`. (1) `a_local_untagged_android_build_keeps_working_on_a_placeholder` asserts only that the binding CONTAINS `devPlaceholderVersionCode`, but that identifier now also appears inside the new `GradleException` message — so deleting the tolerant fallback entirely would leave the test GREEN, which is the one regression it exists to catch. Assert the else-branch shape, and prove the teeth (delete the fallback, watch it red, restore, say so). (2) The new hard failure fires on ANY `WERUST_VERSION` in the Gradle daemon's environment, and this same file documents that a reused daemon carries an earlier shell's value until `./gradlew --stop` — so a developer can hit it in a clean shell having set nothing, and the message only tells them to tag a clean triple. Name the stale-daemon cause and `--stop` in the message, and add it to the spike README's decision 8 beside the hand-set case; keep the failure loud. (3) `versionCodeOf`'s KDoc still states the pre-change rule (a pre-release tag "must take the dev placeholder"); amend it to point at the `werustVersionCode` binding. (4) A `v0.0.0` fold-to-zero fails with text claiming only a clean triple folds, which `v0.0.0` is; say what is actually rejected.
