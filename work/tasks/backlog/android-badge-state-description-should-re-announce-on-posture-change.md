---
title: "Set `stateDescription` directly so a posture change RE-ANNOUNCES, and decide the pre-Android-11 explanation gap"
slug: android-badge-state-description-should-re-announce-on-posture-change
blockedBy: [mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay]
covers: []
---

## What to build

Three residues of `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). The first is a behavioural gap for exactly the users that task was written to serve.

**1. The delegate path never re-announces, and the reason given for choosing it is not accurate.** `BrowserActivity.kt` injects the explanation through an anonymous `View.AccessibilityDelegate` plus a cached `trustDetailText` field, justified in a doc comment (and in decision note §3) by the claim that Android's secondary slot is "write-only from the node the framework hands the delegate". But `View.setStateDescription` exists at the SAME API-30 floor, so a guarded direct property set on the badge would have avoided both the delegate and the field.

That is not merely tidier. **The direct setter emits the state-description-changed accessibility event; the delegate path does not.** So today, if the trust posture changes while a TalkBack user's focus stays on the badge — precisely the moment the explanation matters most, and precisely what the changed-pin warning produces — the new explanation is NOT re-announced. Switch to the guarded direct setter, drop the delegate and the cached field if nothing else needs them, and correct the doc comment and decision note §3, which currently state a constraint that does not exist. Keep the API-30 guard and keep the shape guard passing (it pins the mechanism by name, so it will need updating with the change).

**2. Decide the pre-Android-11 gap deliberately, rather than leaving it in a note.** `stateDescription` is API 30 and `minSdk` is 21, so on Android 10 and older a TalkBack user hears the STATE only and the explanation is reachable solely by activating the badge (the existing `AlertDialog`, which TalkBack advertises as activatable). The task's own bar was "available, never replacing", and one tap away IS available — it is the same deal desktop gives a mouse user, whose explanation is a hover away — so this may well be fine as it stands. What is not fine is that it is a user-facing coverage decision living only in an observation note.

Record it as a decision with a number: either accept the gap explicitly (and say what would change the answer), or implement the alternative the note already names as the one to revisit — a state-first COMPOSED `contentDescription`, which works at API 21. If you take the composed route, compose it ONCE in the core, not on the edge, and mind what the note flags: an eleventh derived field that only one of the two edges paints forks the shape guard's "both edges decode and paint every derived field" symmetry, so decide what that guard should then assert.

**3. A stale heading in the superseded decision.** `docs/spikes/mobile-chrome-presentation-from-one-derivation/DECISIONS.md` D5's TITLE and its "Chosen" paragraph still say the explanation is the badge's accessibility description (Android `contentDescription`, iOS `accessibilityLabel`). The supersede paragraph follows immediately and is explicit, so a careful reader is fine — but someone skimming headings gets the pre-fix wiring, which is the wiring this task's parent existed to remove. Fix the heading and the Chosen line so the correction is visible without reading on.

**Scope:** one Android slot mechanism swapped with its guard, one recorded decision (with or without the composed fallback), one heading correction. No change to the iOS edge, no change to the derivation, no new core field unless item 2 deliberately takes the composed route.

## Acceptance criteria

- [ ] The Android badge's explanation is set via the guarded direct `stateDescription` property, so a posture change while focus remains on the badge RE-ANNOUNCES.
- [ ] The delegate and cached field are gone unless something else needs them, and the doc comment plus decision note §3 no longer claim a write-only constraint that does not exist.
- [ ] The pre-Android-11 gap is a numbered decision: accepted with what would change the answer, or closed with a core-composed state-first fallback.
- [ ] If the composed route is taken, the string is composed ONCE in core and the shape guard's both-edges-paint-every-field symmetry is deliberately resolved.
- [ ] `mobile-chrome-presentation-from-one-derivation`'s D5 heading and Chosen line describe the superseded wiring as superseded.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green, and the Android edge still builds (release dry run).

## Prompt

> Goal: three residues of the mobile trust-badge accessibility split. (1) `BrowserActivity.kt` sets the explanation through an anonymous `View.AccessibilityDelegate` + a cached `trustDetailText` field, justified by a claim that the secondary slot is write-only from the framework's node — but `View.setStateDescription` exists at the same API-30 floor, and crucially the DIRECT setter emits the state-description-changed accessibility event while the delegate path does NOT. So a posture change while a TalkBack user's focus stays on the badge (exactly what a changed-pin warning produces) is never re-announced. Swap to the guarded direct setter, drop the delegate and field if unneeded, correct the doc comment and decision note §3, and update the shape guard that pins the mechanism by name. (2) `stateDescription` is API 30 against `minSdk = 21`, so Android 10 and older hear the state only, with the explanation one tap away in the existing dialog — possibly fine (desktop's is a hover away), but it is a user-facing coverage decision living only in an observation note: make it a NUMBERED decision, either accepting the gap and saying what would change the answer, or taking the note's named alternative (a state-first composed `contentDescription`, composed ONCE in core, resolving what the both-edges-paint-every-field guard should then assert). (3) `mobile-chrome-presentation-from-one-derivation`'s D5 heading and Chosen paragraph still describe the pre-fix wiring; fix them so a heading-skimmer is not misled.
