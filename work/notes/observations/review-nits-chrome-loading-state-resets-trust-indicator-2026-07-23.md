---
title: review-gate non-blocking nits for 'chrome-loading-state-resets-trust-indicator' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: chrome-loading-state-resets-trust-indicator
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'chrome-loading-state-resets-trust-indicator' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the neutral-loading DEFAULT: label is the glyph+text '... loading...' with a grey #5c5c5c 'trust-loading' CSS class (a fourth colour), applied identically on desktop and both mobile shells. It makes no trust claim (never says verified). Load-bearing user-visible default but easily reversible.
  (Decision 2 in the decisions note; crates/werust/src/main.rs trust_indicator/_detail/_css_class + TRUST_INDICATOR_CSS; Kotlin/Swift trustIndicator().)
- Ratify the adjacent latent-bug fix: the desktop CSS class-toggle set previously listed only trust-verified/trust-name-trusted-rpc/trust-unverified, omitting trust-mutable-name, so the purple class could linger after a transition. The fix now lists all five (adding trust-loading and trust-mutable-name). Correct and in-scope (same edit site), but a self-directed fix beyond the task text.
  (Decision 5; verified pre-image at crates/werust/src/main.rs lines 104-107 omitted trust-mutable-name while trust_indicator_css_class can return it.)
- Ratify the parity gap: the mobile Kotlin/Swift loading-wins branches have NO Rust unit test (the mobile display mapping lives edge-side; only the shared 'loading' fact over FFI JSON is Rust-pinned). Honestly disclosed and matches repo convention, but the mobile assertion is not gate-enforced.
  (Decision 6; WerustCore.kt/.swift read getBoolean('loading') then branch loading -> '... loading...'; no Rust test for those branches.)
