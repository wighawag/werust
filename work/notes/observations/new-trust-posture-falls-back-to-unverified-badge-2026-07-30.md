---
title: "A new TrustPosture variant paints as `trust-unverified` until someone adds a branch (no compile error)"
date: 2026-07-30
status: open
spec: trustless-ens-to-ipfs-phase2-3-helios-and-hardening
kind: observation
---

Noticed while closing `export-the-chrome-css-class-set-from-core`: `trust_indicator_css_class` (and its siblings `trust_indicator` / `trust_indicator_detail`, `crates/werust-core/src/lib.rs`) are `if`/`else if` chains ending in an `else` fallback, not `match`es over `TrustPosture`. So adding a fifth posture (the anticipated Phase-2 name-verified one) compiles fine everywhere in the chrome derivation and paints the new posture with the plain `trust-unverified` badge and wording. That is FAIL-CLOSED and therefore honest (a posture werust cannot describe is never called "verified"), but it is silent: nothing reds, so the Phase-2 task must remember to add each branch itself.

Not fixed here (out of this task's scope; converting the chains to exhaustive matches touches several rules at once). Verified by mutation while checking the CSS-class-set teeth: with a `TrustPosture::NameVerified` added and no rule touched, `cargo test` stayed green and the badge fell through to `trust-unverified`. See `docs/spikes/export-the-chrome-css-class-set-from-core/DECISIONS.md` (D4, "what it does NOT catch").
