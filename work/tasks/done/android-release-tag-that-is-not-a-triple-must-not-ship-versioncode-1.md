---
title: "A release tag that is not a clean triple must FAIL the APK build, not silently ship `versionCode = 1` again"
slug: android-release-tag-that-is-not-a-triple-must-not-ship-versioncode-1
blockedBy: [android-apk-version-from-the-release-tag]
covers: []
---

## What to build

Two residues of `android-apk-version-from-the-release-tag`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). The first is a latent release defect, not a nit.

**1. A pre-release tag silently reintroduces the exact bug the parent task removed.** `.github/workflows/release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is a perfectly acceptable release tag today. On such a tag, `crates/werust-android/app/build.gradle.kts` resolves `versionName = "0.3.0-rc1"` (correct, it mirrors what the core reports) but `versionCodeOf()` returns null for a non-triple, so `werustVersionCode` falls back to `devPlaceholderVersionCode = 1`. A SIGNED release APK would ship with `versionCode = 1`: unsequenceable, un-updatable, indistinguishable from every other placeholder build. That is precisely the condition the parent task existed to remove, reachable through the front door.

It is also internally inconsistent: decision 5 of that task argues a placeholder on a RELEASE APK is the worst outcome, and it already fails the build loudly (a `GradleException`) when a minor or patch component exceeds 99. A non-triple tag deserves the same treatment and currently gets the opposite.

**Prescribed fix, with the distinction that matters:** the placeholder fallback is right for a DEV build and wrong for a RELEASE build, so make the behaviour depend on which one it is, not on the shape of the string alone. When `WERUST_VERSION` is set (i.e. CI resolved it from a tag) and it does not fold to a valid `versionCode`, FAIL the build with a message naming the tag, saying why it cannot be sequenced, and stating the accepted shape. When it is absent (a local build), keep today's tolerant behaviour exactly: a dev APK with a placeholder version must still build and install.

**The policy question this leaves open, and how to record it rather than decide it silently:** the alternative is a mapping that CAN sequence pre-release tags (for example, folding `rc1` into a lower code than the final release, the way many Android projects reserve a digit for it). That is a real product decision about whether this project ever cuts pre-release tags at all. Do NOT invent such a mapping here. Fail loudly, and record in the spike README that if pre-release tags are ever wanted, the mapping is the thing to design at that point.

**2. The workflow guard forbids substrings anywhere in the job, including in comments.** `crates/werust-core/tests/release_plumbing_shape.rs` (criterion 10, `the_android_leg_mints_no_second_version_source_in_the_workflow`) forbids `versionCode`, `versionName` and `git describe` ANYWHERE inside the `android-apk` job. The intent is right (the workflow must not mint a second version source), but the mechanism catches a future explanatory COMMENT in that job and reds the gate for a non-defect. Narrow it to the lines that could actually mint a source (`run:` / `with:` / `env:` values), so the guard keeps its teeth without punishing documentation. This repo's whole habit is comments that explain WHY next to the thing; a test that forbids them in one job is working against that.

**Scope:** one conditional failure path with its message, its test, and the recorded policy note; one narrowed assertion. No change to the mapping itself, no change to the dev-build path, no new dependency.

## Acceptance criteria

- [ ] With `WERUST_VERSION` set to a non-triple (e.g. `v0.3.0-rc1`), the APK build FAILS with a message naming the offending version, why it cannot be sequenced, and the accepted shape.
- [ ] With `WERUST_VERSION` unset, an untagged local build still succeeds on the placeholder exactly as it does today (a dev APK that installs beats a dev build that fails).
- [ ] The distinction is covered by a test in the ordinary gate, not only by prose.
- [ ] The spike README records that a sequencing mapping for pre-release tags is deliberately NOT designed here, and what would have to be decided if one is ever wanted.
- [ ] `the_android_leg_mints_no_second_version_source_in_the_workflow` no longer fails on an explanatory comment, while still catching a real second source in a `run`/`with`/`env` value.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: close a latent release defect in `android-apk-version-from-the-release-tag`. `release.yml` triggers on `tags: [v*]`, so `v0.3.0-rc1` is an acceptable tag; on it, `build.gradle.kts` resolves `versionName = 0.3.0-rc1` but `versionCodeOf()` returns null and falls back to `devPlaceholderVersionCode = 1`, so a SIGNED release APK would ship unsequenceable with `versionCode = 1` — exactly the bug that task removed, and the opposite of what its own decision 5 does for an out-of-range component (a loud `GradleException`). Make the behaviour depend on dev-versus-release rather than on the string shape: when `WERUST_VERSION` is SET and does not fold to a valid code, FAIL loudly naming the version, the reason and the accepted shape; when it is UNSET, keep today's tolerant placeholder so a local `assembleDebug` still builds. Cover the distinction with a test. Do NOT invent a pre-release sequencing mapping — record in the spike README that it is deliberately undesigned and what would have to be decided if it is ever wanted. Also narrow `release_plumbing_shape.rs`'s criterion-10 guard so it forbids `versionCode`/`versionName`/`git describe` in `run`/`with`/`env` VALUES rather than anywhere in the job, so a future explanatory comment cannot red the gate for a non-defect.
