---
title: "Collapse the Kotlin and Swift chrome-presentation twins onto the ONE shared derivation (and close the trust-explanation gap they revealed)"
slug: mobile-chrome-presentation-from-one-derivation
blockedBy: [desktop-chrome-presentation-into-core]
covers: []
---

## What to build

The follow-up `desktop-chrome-presentation-into-core` deliberately names and defers. That task moves the DESKTOP presentation rules into the shared core; this one makes the two mobile edges consume that same derivation instead of re-deriving it.

**The situation today.** The core hands each edge FACTS over the chrome JSON (`url`, `loading`, `loadStep`, `trustPosture`, `error`, `failureKind`, `retryable`, `invalidEntry`), and then each edge re-implements the DISPLAY RULES by hand: `WerustCore.kt` and `WerustCore.swift` each carry `statusLine()`, `trustIndicator()`, `errorBanner()`, `invalidEntryBadge()`, `loadProgress*()` twins of the Rust originals. Three copies of one rule set, in three languages, in three unit conventions (the load-progress fraction is `0.25` in Rust, `0.25` in Swift and `25` in Kotlin). They agree today only because each change has so far been hand-applied three times in one sitting.

**They have ALREADY drifted, and the drift is not cosmetic.** `trust_indicator_detail`, the text that explains what each posture MEANS (including "werust is loading this page and is not yet asserting a trust level for it"), exists only on desktop. Neither mobile edge has it in any form. For a browser whose thesis is an honest, legible trust posture (`docs/adr/0001`, `docs/adr/0006`), the explanation is missing on the two platforms most users are on. Closing that gap is part of this task, not a separate nicety.

**The fork to decide (prescribed, with the alternative named).** A non-Rust edge can consume the shared derivation two ways:

- **(a) Extend the chrome JSON with the DERIVED strings** (the status line, the trust badge + its detail, the banner text and its severity, the badge text, the progress fraction + hint), so each mobile edge reads a field instead of running a `when`/`switch`. PREFERRED: the mobile edges already decode this JSON every refresh, it needs no new FFI surface, it keeps the derivation in one place in Rust, and it shrinks both mobile files instead of growing them.
- **(b) Expose the derivation functions over FFI** and call them per field. Rejected as the default: it multiplies FFI entry points for values the edge needs on every refresh anyway, and the JSON already crosses that boundary on the same cadence.

Take (a) unless something concrete blocks it, and record the decision. Watch the payload cost: these are short strings computed once per chrome refresh, and the refresh is already a JSON serialise, so the honest expectation is "no measurable change". Say so with a number rather than assuming.

**Keep the wire vocabulary shared.** The derived fields must speak the SAME vocabulary the rest of the system speaks (`werust_core::debug::trust_posture_wire_name`, the `LoadStep` hints), never a second spelling minted for mobile.

**Do not change behaviour.** Every string a mobile edge shows today should be the same string afterwards, EXCEPT the newly-added trust explanation, which is new surface. Where a mobile twin turns out to disagree with the Rust original, that is a BUG the collapse fixes: record each such divergence explicitly rather than silently normalising it, because each one is evidence for why the duplication had to go.

## Acceptance criteria

- [ ] The Kotlin and Swift chrome-presentation functions no longer re-derive: each reads the shared derivation produced in `werust-core` (via the chosen mechanism, default (a)).
- [ ] Every visible string on iOS and Android is unchanged, except the newly-added trust-posture EXPLANATION, which now exists on both mobile platforms (surfaced in a platform-appropriate way: a tap/long-press affordance, an accessibility label, or an info row, not a hover tooltip).
- [ ] Any divergence discovered between a mobile twin and the Rust original is recorded (an observation or a spike DECISIONS entry) and then resolved in favour of the shared derivation.
- [ ] The chosen consumption mechanism is recorded with its rejected alternative, and the chrome-refresh cost is reported with an actual measurement, not an assurance.
- [ ] The derived fields reuse the existing wire vocabulary; no new spelling is minted.
- [ ] `docs/platform-capability-matrix.toml` reflects the trust-explanation capability honestly across platforms (a new row, or an amended existing one, rather than a silent improvement).
- [ ] The repo `verify` gate is green; the mobile edges still build.

## Prompt

> Goal: make the iOS and Android chrome read the ONE shared presentation derivation (moved into `werust-core` by `desktop-chrome-presentation-into-core`) instead of re-implementing it in Swift and Kotlin. Today `statusLine()`, `trustIndicator()`, `errorBanner()`, `invalidEntryBadge()` and `loadProgress*()` are hand-written twins of the Rust originals, in three languages and three unit conventions, and they have already drifted: `trust_indicator_detail` (the text explaining what each trust posture MEANS) exists only on desktop, so the two platforms most users are on show a trust badge with no explanation. Default mechanism: extend the chrome JSON with the DERIVED strings, since both edges already decode it every refresh and it needs no new FFI surface; the alternative (per-field FFI calls) is rejected unless something concrete blocks (a). Behaviour-preserving except the newly-added trust explanation, which must now exist on both mobile platforms in a platform-appropriate way. Any twin found disagreeing with the Rust original is a BUG: record it, then resolve it toward the shared derivation. Reuse the existing wire vocabulary; mint no second spelling. Report the chrome-refresh cost with a measurement.
