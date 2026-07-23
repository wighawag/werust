---
title: "Gate-3 conductor review: retrieval-backend-user-setting (APPROVE, recovered from a Gate-2 iOS-parity block)"
date: 2026-07-23
status: open
reviewOf: retrieval-backend-user-setting
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 28fcf3a
---

## Verdict: APPROVE ✅ — merged to origin/main as 28fcf3a (drive-tasks --review --merge, isolated build)

## Recovery (the parity guard did its job, via the reviewer)

FIRST build BLOCKED by Gate-2: the `werust://settings` scheme was registered on desktop + Android but NOT on iOS (only `ipfs` had a `WKURLSchemeHandler`), so `werust://settings` was unreachable on iOS and the Rust-side `register_scheme_handler(WERUST_SCHEME,...)` was dead — yet the matrix claimed retrieval-backend iOS = `implemented`. This is exactly the silent-one-platform class the parity guard exists to forbid, caught here as a review finding. FIXABLE, so recovered in-loop: `dorfl requeue -m "<register the werust scheme on iOS like ipfs; add the FFI/Swift routing to apply_settings; else honestly stub the iOS cell>"` (kept the branch), then re-`do` continued and added the iOS registration. Gate-2 approved; merged.

## The fix is real (verified)

`crates/werust-ios/App/Sources/WKWebViewShellController.swift` now registers a `WKURLSchemeHandler` for the `werust` scheme (line ~108, mirroring the `ipfs` registration from the mobile-ipfs task), with the FFI/Swift routing so `werust://settings` reaches the core. The matrix's `retrieval-backend` cell is now honestly `implemented` on desktop + iOS + Android.

## Acceptance criteria — all met

- A user setting selects the active IPFS retrieval backend (default trustless gateway + custom gateway/local-node URL; delegated/embedded shown coming-soon), via the internal `werust://settings` page.
- Selecting a backend switches the actual `ContentRetriever` the `ipfs://` load path uses; a custom URL is validated + used.
- Persistence via `RetrievalSettings` (load/save with test-isolatable paths — shared-write rule honoured; the real store is untouched in tests).
- Privacy/trust trade-off legible on the settings page (a public gateway sees browsing; a custom/local endpoint is private).
- Present on desktop + iOS + Android (parity guard green truthfully). 
- The SEQUENCED default is honoured: Phase-1/dev default = labelled public gateway; the SHIPPED final-release default is deferred to the release-gate `retrieval-default-egress-before-final-release` + an ADR, NOT decided here.

## Gate-2 nits (non-blocking)

Two non-blocking nits in `review-nits-retrieval-backend-user-setting-2026-07-23.md`, left open for human triage.
