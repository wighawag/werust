---
title: "Sign and notarize the macOS `.app` so a downloaded release opens without a Gatekeeper fight (the macOS analogue of `android-apk-signing`)"
slug: macos-app-signing-and-notarization
blockedBy: [macos-release-packaging-leg]
covers: []
needsAnswers: true
---

<!-- open-questions -->

## Open questions

1. **Is there an Apple Developer account?** Developer ID signing and notarization both require a paid Apple Developer Program membership ($99/yr) and a Developer ID Application certificate. Without one this task cannot be built at all, and the honest alternative is to keep shipping unsigned and improve the open-it instructions instead. Does the account exist, or is it wanted?
2. **Where do the secrets live, and who puts them there?** The Android precedent needs a keystore, its password and an alias in repository secrets, provided by the human. This needs the exported Developer ID certificate (`.p12`) plus its password, and an App Store Connect API key (issuer id, key id, `.p8`) for `notarytool`. Same question as `android-apk-signing`: the human provides them, but confirm which storage (repository secrets vs an environment) and who rotates them.

<!-- /open-questions -->

## What to build

The follow-on `macos-release-packaging-leg` deliberately named and deferred. That leg attaches a universal but UNSIGNED `Werust-macos-universal-unsigned.app.zip`; this task makes a downloaded release open normally.

**Why it needs its own task at all** (origin: the observation `work/notes/observations/macos-signing-notarization-follow-on-has-no-task-2026-07-31.md`, raised again as a Gate-2 nit and cut by the conductor at Gate-3 on 2026-07-31): the follow-on was already named in FOUR places (the packaging task body, `.github/workflows/release.yml`'s header, the repo README and `windows-release-packaging-leg`) and existed in none of them as a work item. The Android side did it the other way round, and better: `android-apk-signing` was a real task before anything pointed at it. A deferral that is referenced everywhere and tracked nowhere is how a release ships unsigned forever.

**Copy the Android pattern rather than inventing a second one.** `android-apk-signing` is the precedent, and the packaging leg's spike README (decision 3) already prescribes the shape: gate on a secrets-PRESENCE env flag with a graceful no-op when absent (never on the `secrets` context, which a step `if:` cannot read), keep the unsigned artifact's honest name, and attach the signed artifact under a name that says what it is, exactly as the Android leg attaches `app-release.apk` beside the debug one. A fork of the repo with no secrets must still get a green release run.

**The pieces, in order:** `codesign` the bundle with the Developer ID Application identity and the hardened runtime enabled, zip it, submit with `xcrun notarytool submit --wait`, then `xcrun stapler staple` the ticket onto the `.app` and re-zip. Staple the APP, not the zip. Verify with `spctl -a -vvv -t install` and `codesign --verify --deep --strict` in the job, so a broken signature fails the leg rather than the user.

**Two traps this task inherits, both already recorded:**

- **`CFBundleVersion` is `git describe`-shaped on non-tag builds** (`0.2.9-3-gabc1234`), by a deliberate decision (packaging spike, decision 4) that chose correspondence-with-the-reported-version over numeric normalisation. Notarization may reject a non-numeric `CFBundleVersion`. If it does, THIS task owns the normalisation, and it should then also set `CFBundleShortVersionString` — the packaging leg deliberately did not.
- **The `deliberately unsigned` guard is currently over-broad, and narrowing it is in scope here.** `crates/werust-core/tests/release_plumbing_shape.rs::macos_desktop_leg_is_deliberately_unsigned` asserts that `codesign` appears NOWHERE in the job or in `bundle-app.sh`. But ad-hoc signing (`codesign -s -`) is not Developer-ID signing, and on Apple Silicon an arm64 Mach-O must carry at least an ad-hoc signature to execute at all. `ld64` ad-hoc-signs the arm64 slice at link time; whether that signature survives `lipo -create` intact is asserted nowhere and has never been run. So the unsigned bundle may not launch on Apple Silicon, and the one-line fix (`codesign -s - Werust.app`) trips a test. Narrow the absence assertion to the Developer-ID/notarization TOOLS (`notarytool`, `altool`, `stapler`, `codesign` with a real identity) and permit ad-hoc explicitly, with a comment saying why the distinction matters.

**Scope:** the signing + notarization path, its secrets gating, the verification steps, the guard narrowing, and the README correction that follows (a signed+notarized artifact needs none of the Gatekeeper-bypass instructions). Not in scope: an app icon, `CFBundleShortVersionString` beyond what notarization forces, a `.dmg`, Sparkle-style auto-update, or the Windows analogue.

## Acceptance criteria

- [ ] A tagged release attaches a SIGNED and NOTARIZED universal `Werust.app` zip, named so it is distinguishable from the unsigned artifact.
- [ ] Signing and notarization are gated on a secrets-PRESENCE env flag: with no secrets the leg still runs green and still produces the unsigned artifact (a fork must not go red).
- [ ] The job VERIFIES its own output (`codesign --verify --deep --strict` and `spctl -a -vvv -t install` on the stapled bundle) so a broken signature fails CI, not the user.
- [ ] The notarization ticket is STAPLED to the `.app` (not to the zip) and the stapled bundle is what ships.
- [ ] `macos_desktop_leg_is_deliberately_unsigned` is narrowed to forbid Developer-ID/notarization tooling only, with ad-hoc signing explicitly permitted and the reason recorded.
- [ ] If notarization rejects the `git describe`-shaped `CFBundleVersion`, this task normalises it AND sets `CFBundleShortVersionString`; if it accepts it, that is RECORDED as a measured fact rather than assumed.
- [ ] The README's macOS section stops telling users to bypass Gatekeeper for the signed artifact, while keeping accurate instructions for anyone using the unsigned one.

## Prompt

> Goal: sign and notarize the macOS `.app` that `macos-release-packaging-leg` currently ships unsigned, following the `android-apk-signing` precedent exactly rather than inventing a second pattern: gate on a secrets-PRESENCE env flag (never the `secrets` context in a step `if:`), no-op gracefully without secrets so a fork stays green, and name the signed artifact honestly beside the unsigned one. `codesign` with the Developer ID Application identity + hardened runtime, `xcrun notarytool submit --wait`, `xcrun stapler staple` the APP (not the zip), re-zip, and VERIFY in the job with `codesign --verify --deep --strict` + `spctl -a -vvv -t install` so a broken signature fails CI rather than the user. Two inherited traps, both in scope: (1) `release_plumbing_shape.rs::macos_desktop_leg_is_deliberately_unsigned` forbids `codesign` ANYWHERE, but ad-hoc signing is not Developer-ID signing and an arm64 Mach-O needs at least an ad-hoc signature to run — narrow the assertion to the real Developer-ID/notarization tools and permit ad-hoc, saying why; (2) `CFBundleVersion` is `git describe`-shaped on non-tag builds by deliberate decision, so if notarization rejects it, normalise it here and set `CFBundleShortVersionString` too, and if it accepts it, record that as measured. Finally correct the README so the Gatekeeper-bypass instructions apply only to the unsigned artifact. This repo has no Mac, so state plainly what CI proved versus what awaits hardware.
