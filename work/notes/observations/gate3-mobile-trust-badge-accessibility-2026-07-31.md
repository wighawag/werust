---
title: "Gate-3 verdict: mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay (APPROVE) — the state is heard first again"
date: 2026-07-31
status: open
reviewOf: mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. The regression I cut this task for is closed: a screen-reader user hears WHICH trust state the badge is in, and the explanation follows rather than replacing it.

## Measured, per the convention ratified today

No gate in this repo compiles a line of the changed Kotlin or Swift, and this diff adds the file's first anonymous `AccessibilityDelegate` and its first API-30 node-info property. The Gate-2 reviewer said so explicitly and noted that obtaining the measurement is the conductor's job. Both edges build:

- **iOS: [run 30611656244](https://github.com/wighawag/werust/actions/runs/30611656244)** (`mobile-ios.yml`, push to `main`) — SUCCESS.
- **Android: [run 30611679642](https://github.com/wighawag/werust/actions/runs/30611679642)**, `android-apk` job — SUCCESS.

## Criteria, ticked

1. **The STATE is announced first and always; the explanation is available, never replacing it.** MET on both edges.
2. **Both strings still come from the one shared derivation; no edge composes, re-words or truncates.** MET, and explicitly recorded as decision §3.
3. **The wiring is pinned by the mobile shape guard**, so a later refactor cannot silently swap the label back. MET.
4. **The `MEASUREMENT.md` wording corrected** ("facts-only, same encoder", not "the pre-change encoder"). MET.

The two per-platform slot choices the task deliberately left open were both decided well, and written down where a reviewer can reverse them:

- **iOS uses `accessibilityValue`, not `accessibilityHint`.** The right call for a reason worth keeping: hints are announced after a pause and can be switched OFF entirely in Settings, so the trust EXPLANATION would sit behind a preference most people never see. "Available unless the user disabled hints" is the wrong guarantee for this particular string.
- **Android uses `stateDescription` only, with no pre-API-30 fallback.** Argued honestly, with two alternatives named and costed rather than waved away.

## Residues, cut as `android-badge-state-description-should-re-announce-on-posture-change`

- **The Android delegate path never RE-ANNOUNCES, and its stated justification is factually wrong.** The doc comment and decision §3 say the secondary slot is "write-only from the node the framework hands the delegate", but `View.setStateDescription` exists at the same API-30 floor. The consequence is not cosmetic: the direct setter emits the state-description-changed event and the delegate path does not, so a posture change while a TalkBack user's focus stays on the badge — exactly what a changed-pin warning produces, and exactly the moment this task exists for — is never announced. Good catch by the reviewer; it needed knowing the platform, not just reading the diff.
- **The pre-Android-11 gap** (API 30 slot, `minSdk = 21`) is defensible and possibly final, but it is a user-facing coverage decision living only in an observation note. Promoted to a numbered decision either way.
- **A stale HEADING** in the superseded D5 of the parent spike, which still describes the pre-fix wiring to anyone skimming.
