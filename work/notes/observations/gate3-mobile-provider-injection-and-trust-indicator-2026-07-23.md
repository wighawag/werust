---
title: "Gate-3 conductor review: mobile-provider-injection-and-trust-indicator (APPROVE) — mobile parity complete, zero stubbed matrix cells"
date: 2026-07-23
status: open
reviewOf: mobile-provider-injection-and-trust-indicator
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: b48cb08
---

## Verdict: APPROVE ✅ — merged to origin/main as b48cb08 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green; no recovery needed)

Closes the last two desktop-only silent no-ops the parity guard surfaced (the guard earned its keep: it named these gaps on its first run).

## Acceptance criteria — all met

- EIP-1193 provider is injected on iOS (WKWebView) and Android (System WebView) via the OS-edge script bridge, routed through the SAME `werust-core` provider path desktop uses. The mobile backends' `register_script_message_handler` / `inject_script` are no longer empty no-ops.
- The mobile chrome renders the real trust posture: `trust_posture` reads the shared `LoadLifecycle` (the same source the desktop chrome reads), not the inherited seam default. The two-axis model is wired on mobile: `mark_ens_origin` -> `NameViaTrustedRpc`, `mark_mutable_name` -> `MutableName`.
- The parity matrix's `eip1193-provider` and `trust-indicator` cells are now `implemented` on desktop + iOS + Android. ZERO stubbed cells remain in the whole matrix — every capability is implemented on every shipped platform, and the guard is green truthfully (not by tracking, by completion).
- Tests prove the provider bridge + trust posture reach/derive-from the core on each mobile edge; the parity guard test was updated accordingly.

## Significance

The platform-capability parity guard has now driven werust to FULL parity: the three gaps it exposed (`ipfs-render`, `eip1193-provider`, `trust-indicator`, all originally desktop-only silent no-ops) are closed on iOS + Android. This is the mechanism working end to end: it caught the silent gaps, tracked them as stubbed cells, and the cells are now all `implemented`. A future silently-one-platform feature will red the gate.

## Gate-2 nits (non-blocking)

Two non-blocking nits in `review-nits-mobile-provider-injection-and-trust-indicator-2026-07-23.md`, left open for human triage.
