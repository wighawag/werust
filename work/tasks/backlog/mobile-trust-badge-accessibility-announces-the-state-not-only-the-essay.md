---
title: "A screen-reader user should still hear WHICH trust state the badge is in, not only the 240-character explanation of it"
slug: mobile-trust-badge-accessibility-announces-the-state-not-only-the-essay
blockedBy: [mobile-chrome-presentation-from-one-derivation]
covers: []
---

## What to build

A user-visible accessibility regression introduced by `mobile-chrome-presentation-from-one-derivation`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). Small fix, real consequence.

**What changed.** That task finally gave the mobile edges the trust EXPLANATION they had been missing for months, which was the right and overdue thing. But it wired the explanation in by REPLACING the badge's accessibility text on both edges: `trust.contentDescription = chrome.trustIndicatorDetail` (Android, `BrowserActivity.kt` `refreshChrome`) and `trustLabel.accessibilityLabel = chrome.trustIndicatorDetail` (iOS, `WKWebViewShellController.swift`).

**The consequence.** A screen-reader user now hears a ~240-character sentence on every focus of that badge, and never hears the badge's own label. So the one piece of information a sighted user gets instantly and repeatedly — WHICH state this is, "verified" or otherwise — is the one piece a blind user no longer gets at all. On a browser whose entire thesis is an honest, legible trust posture (`docs/adr/0001`, `docs/adr/0006`), that is the wrong half to drop. Sighted users are unaffected, which is exactly why this would go unnoticed.

**The fix, which is the platform-idiomatic split the decision did not weigh.** The badge's LABEL should stay the badge's own text (the short state name), and the explanation belongs in the secondary slot each platform already provides for exactly this: `stateDescription` or a `contentDescription` that leads with the state, plus the explanation, on Android; `accessibilityValue` / `accessibilityHint` on iOS. The concrete requirement, however you spell it per platform: **the state name is announced FIRST and always; the explanation follows or is available, never replaces it.**

Keep both strings coming from the ONE shared derivation this task just built (`trust_indicator` and `trust_indicator_detail` in the core). Do NOT re-derive or re-word either on an edge, and do not mint a third "accessibility string" in core unless both edges genuinely need the same composed form — in which case compose it in core, once, and say so.

**Also, while in the same spike (doc accuracy, no code):** `MEASUREMENT.md` and the module doc of `crates/werust-core/examples/chrome_json_cost.rs` both describe `baseline_facts_only_json` as "the pre-change encoder". It is not: it re-encodes the eleven original facts with `serde_json`, whereas the deleted `ffi_json` twins used a hand-rolled `format!` + escape. So the reported delta measures the cost of the ten EXTRA FIELDS, not the real before/after of that commit, which also swapped the encoder. The CONCLUSION is unaffected (microseconds, event-driven cadence, not observable), so do not re-run anything: just correct the wording to "facts-only, same encoder" so the number is not read as something it never measured.

**Scope:** the two accessibility wirings, whatever shape test pins them, and one wording correction. No change to the derivation, no change to what either edge displays visually.

## Acceptance criteria

- [ ] On BOTH mobile edges, a screen reader announces the trust STATE (the badge's own label) first; the explanation is available in the platform's secondary slot rather than replacing the label.
- [ ] Both strings still come from the one shared core derivation; neither edge re-derives, re-words or truncates either.
- [ ] The wiring is pinned by the existing mobile shape-guard style, so a later refactor cannot silently swap the label back.
- [ ] `MEASUREMENT.md` and the example's module doc describe the baseline as facts-only with the SAME encoder, not as the pre-change encoder; the measured numbers and the conclusion are unchanged.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green, and both mobile edges still build (the iOS leg on push to `main`, the Android APK via the release dry run).

## Prompt

> Goal: `mobile-chrome-presentation-from-one-derivation` gave the mobile edges the trust explanation they had been missing, but wired it in by REPLACING the badge's accessibility text (`trust.contentDescription` on Android, `trustLabel.accessibilityLabel` on iOS) with the ~240-character detail. A screen-reader user therefore hears the essay on every focus and never hears WHICH trust state the badge is in — the one fact a sighted user gets instantly, dropped for the users who most depend on it, in a browser whose thesis is a legible trust posture. Restore the platform-idiomatic split: the LABEL stays the badge's short state name, and the explanation moves to the secondary slot (`stateDescription` or a state-first `contentDescription` on Android, `accessibilityValue`/`accessibilityHint` on iOS), so the state is announced FIRST and always. Both strings must keep coming from the one shared core derivation (`trust_indicator`, `trust_indicator_detail`) — do not re-derive, re-word or truncate on an edge, and only compose a combined string if you do it once, in core. Pin the wiring with the existing mobile shape-guard style. While in that spike, correct `MEASUREMENT.md` and the `chrome_json_cost.rs` module doc: `baseline_facts_only_json` is facts-only with the SAME encoder, not "the pre-change encoder", so the delta measures the extra fields rather than the commit's real before/after; the conclusion stands, only the wording is wrong.
