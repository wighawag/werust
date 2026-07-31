# Decisions taken while splitting the mobile trust badge's accessibility text (2026-07-31)

Task `mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay`. The task named the shape ("the state name is announced FIRST and always; the explanation follows or is available, never replaces it") but left two per-platform calls open. Both are also documented at the choice site (`BrowserActivity.kt` around the badge's `accessibilityDelegate`, `WKWebViewShellController.swift` around `trustLabel.accessibilityValue`); this note is where they sit together so a reviewer can ratify or reverse them.

## 1. iOS secondary slot: `accessibilityValue`, not `accessibilityHint`

The task offered either. `accessibilityValue` is announced by VoiceOver immediately after the label, always; `accessibilityHint` is announced after a pause and can be switched OFF entirely in Settings, which would put the trust EXPLANATION behind a user preference most people never see. For a badge whose whole job is a legible trust posture (`docs/adr/0001`, `docs/adr/0006`), "available unless the user disabled hints" is the wrong guarantee, so the explanation is the badge's VALUE. What it touches: the tap alert reads the same slot (`showTrustExplanation()` now guards on `trustLabel.accessibilityValue`), and the shape guard `both_mobile_edges_surface_the_trust_explanation` pins the slot by name.

## 2. Android secondary slot: `stateDescription` only, with no pre-API-30 fallback

`AccessibilityNodeInfo.setStateDescription` arrived in API 30 and this app's floor is `minSdk = 21`, so on Android 10 and older the badge announces its STATE only and the explanation stays one tap away (the existing `AlertDialog` affordance, which TalkBack advertises as activatable). That is the same deal desktop gives a mouse user, whose explanation is a HOVER away, and it is strictly better than what shipped, which announced the ~240-character explanation and never the state.

Alternatives considered: (a) a state-first COMPOSED `contentDescription`, which works at API 21 but would have to be composed once in the core and carried as an eleventh derived field that only one of the two edges paints, forking the shape guard's "both edges decode and paint every derived field" symmetry; (b) an extra `AccessibilityNodeInfo.hintText` branch for API 26-29, which narrows the uncovered window to Android 7.1 and older at the cost of a second untestable branch mixing two slot semantics. Both were rejected as more machinery than the remaining gap is worth, but (a) is the one to revisit if pre-Android-11 screen-reader coverage is ever asserted as a requirement. What it touches: `werust_core::chrome_json`'s field set (unchanged by this task) and the guard's `DERIVED_FIELDS` list, which stays one list for both edges.

## 3. Neither edge composes anything

Both strings stay verbatim from `trust_indicator` / `trust_indicator_detail` on the one carrier; no edge concatenates, re-words or truncates, so nothing new was minted in the core either. The Kotlin edge now caches the explanation in a `trustDetailText` field instead of parking it on the widget, because Android's secondary slot is write-only from the node the framework hands the delegate; the field is still the core's derivation held only between a paint and a tap, exactly like the TOFU strings beside it.
