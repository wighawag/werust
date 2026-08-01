---
title: "Apple's signing tiers decide what werust can ever TEST on Apple platforms: with no Mac and no paid membership, macOS and iOS are CI-only and unverifiable by any human"
date: 2026-08-01
status: verified
kind: finding
source:
  - https://developer.apple.com/support/compare-memberships/ (free Apple ID vs the paid Developer Program: what each tier may sign and distribute)
  - https://developer.apple.com/documentation/xcode/distributing-your-app-to-registered-devices (registered-device distribution needs a Developer Program membership; device UDIDs are registered in the portal)
  - https://developer.apple.com/documentation/xcode/running-your-app-in-simulator-or-on-a-device (free "Personal Team" provisioning is performed BY XCODE against an attached device; the 7-day profile lifetime)
  - https://developer.apple.com/testflight/ (TestFlight: internal testers install over the air, 90-day builds, requires App Store Connect i.e. a paid membership)
  - .github/workflows/release.yml (`ios-simulator-app` builds `aarch64-apple-ios-sim`; `macos-desktop-app` attaches an UNSIGNED universal `.app`)
  - measured in conversation 2026-08-01 (the maintainer has no Mac and no paid membership)
---

## Why this is written down

The question "can a GitHub macOS runner sign a free 7-day build for my iPhone?" is reasonable, gets asked repeatedly, and has a non-obvious answer (no, for three independent reasons). More importantly, the answer determines **which werust platforms can ever have a human in the loop**, which should shape prioritisation rather than being rediscovered each time someone proposes Apple-platform work.

## The external ground truth

**A Simulator build cannot run on a device.** The release leg builds `aarch64-apple-ios-sim`, which targets the Simulator runtime. Physical hardware needs `aarch64-apple-ios`. No signing question arises until that target exists; today's `WerustShell-Simulator.app.zip` could not run on an iPhone even if it were signed.

**iOS has no unsigned install path at all.** This is the hard asymmetry with Android. An APK installs under any signature, which is why Android field-testing works with the keystore we just configured. iOS refuses to execute any app without a valid signature AND a provisioning profile naming the target device. There is no "allow unknown sources".

**Free-tier ("Personal Team") signing cannot be automated, for three separate reasons:**

1. **No automatable credentials.** Xcode mints the free certificate and profile through an interactive, Apple-ID-authenticated flow. A free account has no access to the developer portal's Certificates/Identifiers/Profiles and no App Store Connect API key, so a CI job has nothing to authenticate with.
2. **The device must be registered, and the free tier registers it by having it physically attached.** A phone cannot be attached to a GitHub runner, and a free account cannot register a UDID through the portal (it has no portal).
3. **Runners are ephemeral.** Even signing in by hand over SSH would leave the certificate in a throwaway keychain. Reusing credentials on CI requires first exporting a cert + profile from a Mac you own, which is circular.

So free 7-day provisioning genuinely requires a Mac. A macOS CI runner is a batch host, not a substitute.

**The paid membership ($99/yr) is Mac-free, and this is the part that surprises people.** With it, the whole loop needs no Apple hardware: certificates come from the web portal (a CSR is `openssl` on Linux), the macOS runner builds and signs the device target and uploads with `xcrun altool`, and **TestFlight** delivers over the air to the phone. Internal testers need no UDID registration and no Beta App Review, and builds last **90 days** rather than 7.

## What it means for werust (the part that should change decisions)

**Two of the five shipped platforms have no human in the loop, ever.** iOS ships a Simulator build that needs a Mac to launch; macOS ships `Werust-macos-universal-unsigned.app.zip`, which nobody on this project can open. For both, CI is not the primary evidence, it is the ONLY evidence.

Three consequences follow, and they are the reason this finding exists rather than a note in one task:

1. **The Apple-platform CI smokes carry all the weight, and therefore deserve to be STRONGER than the ones for platforms that get used.** Every serious bug this project has caught in the field came from a human using it: the Android ANR, the SvelteKit client-nav death, `localStorage` being null, and (2026-08-01) the malformed-CID error path. macOS and iOS get none of that, so any behaviour CI does not assert simply ships.

2. **Apple-platform UX/polish work has no validation loop.** Anything whose acceptance is "a human sees the right thing on a Mac" cannot be accepted here at all. This is why `macos-app-signing-and-notarization` is cancelled (2026-08-01) and why a proposed successor task to "improve the Gatekeeper open-it instructions" was dropped rather than written: it would be confident prose about a dialog nobody here can see. Prefer CI-asserted behaviour over described behaviour on these two platforms.

3. **Signing helps OTHER USERS on macOS, and helps US on iOS.** They are not the same purchase. On macOS the blocker is hardware, not certificates: notarizing changes nothing about our ability to test. On iOS the paid membership would convert a dead platform into a testable one. If the membership is ever bought, **iOS device delivery should come before macOS notarization**, the reverse of what those two tasks' existence implies.

## Status of the decision (2026-08-01)

The maintainer declined the $99 membership **for now**. So the constraint is current, not permanent: buying the membership unlocks iOS field-testing (and only then macOS notarization becomes worth sequencing). Nothing else unlocks either: not a CI runner, not a free Apple ID, not a workaround.

One fallback is on record but deliberately not built: CI could emit an UNSIGNED device-target `.ipa` which a tool such as Sideloadly signs with a free Apple ID from a **Windows** machine (7-day expiry). The Linux equivalents are unofficial and unreliable, so no workflow should be built around it.
