---
title: T0 server-web floor — authored subset fragment vs committed goldens
slug: t0-server-web-floor-golden-fixtures
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [native-renderer-t0-subset-path-behind-seam]
covers: [11]
---

## What to build

Establish the T0 server-web floor: commit authored static HTML fragments using only
the v0 element/property allowlist as golden fixtures, and assert the native T0 path
renders them correctly against those goldens. This is the objective regression guard
at T0 (there is no WPT bar at T0 — a fixed private subset has no meaningful public
pass-rate; the golden-image suite is the guard).

## Acceptance criteria

- [ ] A set of committed golden fixtures (authored v0-allowlist HTML fragments + their expected rendered reference) exists.
- [ ] The native T0 path renders each fixture and is asserted stable against its golden reference.
- [ ] A subset-doc-drift guard exists so the fixtures stay within the documented v0 allowlist.
- [ ] The golden suite runs under the `verify` gate and fails on a regression.

## Blocked by

- Blocked by `native-renderer-t0-subset-path-behind-seam`.

## Prompt

> Goal: pin the T0 server-web floor with golden fixtures (see
> `docs/conformance-tiers.md` T0 — the golden-image suite + subset-doc-drift guard is
> the T0 regression guard; WPT bars begin at T1).
>
> Commit authored v0-allowlist fragments + their golden references and assert the
> native T0 path renders them stably. Keep fixtures within the documented allowlist
> (a drift guard). This is the "server floor" half of T0; the content-addressed floor
> is `t0-content-addressed-floor-parity` — T0 is only "reached" when BOTH land.
>
> Done = the native T0 path renders the committed server-floor fixtures at golden
> parity, guarded under the gate.
