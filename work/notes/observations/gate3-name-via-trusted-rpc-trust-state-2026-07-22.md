---
title: "Gate-3 conductor review: name-via-trusted-rpc-trust-state (APPROVE)"
date: 2026-07-22
status: open
reviewOf: name-via-trusted-rpc-trust-state
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
gate: gate-3-conductor-diff-review
verdict: approve
mergedAs: 619a101
---

## Verdict: APPROVE ✅ — merged to origin/main as 619a101 (drive-tasks --review --merge, isolated build, Gate-1 + Gate-2 green)

Conductor's own diff-vs-acceptance pass over the landed diff on origin/main. Threads the new posture through all four layers (renderer enum -> webview-renderer -> werust-core BrowserShell/ChromeState -> desktop chrome in the werust binary), exactly like the existing content-verified posture precedent.

## Acceptance criteria — all met

- `TrustPosture` gains `NameViaTrustedRpc`, a distinct variant separate from `ContentVerified` and `UnverifiedOrigin`.
- `ChromeState` exposes `is_name_via_trusted_rpc()`; the desktop chrome renders a legible, visually-distinct indicator: badge `"◈ name via trusted RPC"` with its own tooltip detail and CSS class (three visually distinct trust states).
- Driven by the ACTUAL load path, not the URL: the seam test uses a fake backend `serve_via_ens_trusted_rpc()` that marks the posture on a real resolution; a name-looking URL alone does NOT trigger it (test asserts "the URL looking like a name is NOT enough").
- Seam tests assert the chrome reflects the new state (`the_chrome_shows_the_name_via_trusted_rpc_posture_for_an_ens_resolved_load`) AND that it does not leak onto a later served load (`the_name_via_trusted_rpc_posture_does_not_leak_into_a_later_served_load`: a fresh navigation resets to untrusted).

## HONESTY REQUIREMENT — CONFIRMED (the non-negotiable Phase-1 label rule)

The new posture is NEVER surfaced as "verified" / "name-verified":
- `TrustPosture::NameViaTrustedRpc::is_content_verified()` returns FALSE (it is its own predicate, not folded into content-verified).
- The doc-comment on the variant explicitly states it MUST never be surfaced as "verified"/"name-verified" (name-verification is a Phase-2 addition once a light client exists).
- The user-visible chrome badge is `"◈ name via trusted RPC"` — a distinct middle badge, deliberately NOT "verified". A grep for `name.?verified` / `"verified"` found only the doc-comments FORBIDDING the mislabel, no actual mislabel.

## Scope fence honoured

This task adds + plumbs the STATE and proves it with a fake backend, as scoped. It does NOT touch `install_ipfs`/`mark_content_verified` in the webview backend — the REAL wiring that makes an ENS-originated `ipfs://` load report this posture (and resolves the clash with the scheme handler's unconditional `mark_content_verified`) is correctly left to the front-door task `bare-eth-urlbar-front-door-end-to-end`. Keeping the `TrustPosture`/`ChromeState` edit here avoids a merge collision with that task, exactly as the task body directed.

## Gate-2 nits (non-blocking, already recorded)

Three non-blocking nits in `review-nits-name-via-trusted-rpc-trust-state-2026-07-22.md`, left open for human triage. None block integration; none require a re-task.
