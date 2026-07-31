---
title: review-gate non-blocking nits for 'mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the iOS slot choice: the explanation rides in accessibilityValue rather than accessibilityHint, so VoiceOver always reads it straight after the state name and it cannot be switched off in Settings. This is a user-visible default; recorded in the decision note, sound as argued.
  (WKWebViewShellController.swift:320-321,452-453; work/notes/observations/mobile-trust-badge-accessibility-slot-decisions-2026-07-31.md section 1)
- Ratify the Android coverage gap: stateDescription is API 30, minSdk is 21, so on Android 10 and older a TalkBack user hears the STATE only and the explanation is reachable solely via the tap dialog. Strictly better than what shipped and consistent with the task wording (available, never replacing), but should the pre-Android-11 case be cut as a named follow-up rather than living only in an observation note?
  (BrowserActivity.kt:399-408 (Build.VERSION_CODES.R guard); build.gradle.kts:186 minSdk = 21; decision note section 2 lists the composed-contentDescription alternative as the one to revisit)
- Unrecorded implementation decision with an inaccurate justification: the explanation is injected through an anonymous View.AccessibilityDelegate plus a cached trustDetailText field, justified by the claim that the secondary slot is write-only from the node the framework hands us. View.setStateDescription exists at the same API-30 floor, so a guarded direct property set on the badge would have avoided the delegate and the field. Two consequences worth a glance: the direct setter also emits the state-description-changed accessibility event (the delegate path does not, so a posture change while focus stays on the badge will not re-announce the explanation), and the extra field/delegate is machinery the guard now pins by name.
  (BrowserActivity.kt:222-239 doc comment and 399-411; decision note section 3)
- Stale heading in the superseded decision: D5's title and its Chosen paragraph still say the explanation is the badge's accessibility description (Android contentDescription, iOS accessibilityLabel). The supersede paragraph immediately follows and is explicit, so a careful reader is fine, but someone skimming headings gets the pre-fix wiring.
  (docs/spikes/mobile-chrome-presentation-from-one-derivation/DECISIONS.md:51-55)
- No gate in this repo compiles the changed Kotlin or Swift, and this diff adds the file's first anonymous AccessibilityDelegate and its first API-30 node-info property. Per the repo convention, obtaining that measurement is the conductor's job, but the iOS leg on main and the Android release dry run are the only proof the two edges still build.
  (.github/workflows has mobile-ios.yml and release.yml only; acceptance criterion 5 of the task)
