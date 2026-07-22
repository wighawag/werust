---
title: Gate-3 verdict — fix-gtk4-feature-pin-to-v4-14-for-ci — APPROVE; CI now GREEN
date: 2026-07-22
kind: observation
reviewOf: fix-gtk4-feature-pin-to-v4-14-for-ci
verdict: APPROVE
---

## Gate-3 verdict: APPROVE ✅ — and CI verify is now GREEN on main

Third and final CI fix in the chain (system-deps -> GTK pin). `do` ran Gate-1 + Gate-2
green; Gate-3 confirmed on the actual runner.

### Root cause (fully diagnosed from the runner logs)

`ubuntu-24.04` (`ubuntu-latest`) ships GTK **4.14.5** but WebKitGTK **2.52.3**. The
`webview-renderer` crate pinned GTK-4.18 features (`gtk4 v4_18`, `webkit6 gtk_v4_18`),
forcing a `gtk4 >= 4.18` system requirement the runner cannot meet. The dev laptop has
GTK 4.18.6 so it built locally; CI did not. WebKitGTK was never the problem (2.52.3
satisfies `v2_50`).

### The fix

- `gtk4` feature lowered `v4_18` -> `v4_14` (Ubuntu 24.04's GTK, widest install base).
- `webkit6` kept at `v2_50` but the `gtk_v4_18` feature DROPPED. Good catch by the
  agent: webkit6 0.5.0 has no `gtk_v4_14` forward, and `gtk_v4_18` would re-force
  gtk4 >= 4.18 — so omitting it keeps gtk4 at the v4_14 pin. Documented inline.
- No code change (the backend uses only long-stable GTK4/WebKitGTK APIs). This also
  WIDENS the distros werust builds on.

### Verified GREEN

`gh run watch` on the verify run for the merge: conclusion **success**, all steps ✓
(fmt/clippy/build/test) in ~5 min on ubuntu-24.04. The `glib-2.0 not found` and
`gtk4 >= 4.18` failures the human reported are both resolved.

### The full CI-fix chain (three landed fixes)

1. `fix-ci-verify-missing-webkitgtk-system-deps` — install `libwebkitgtk-6.0-dev` in
   both verify workflows (+ corrected the goreleaser desktop leg's wrong-ABI
   `libwebkit2gtk-4.1-dev` -> `libwebkitgtk-6.0-dev`).
2. `fix-gtk4-feature-pin-to-v4-14-for-ci` — this one.

### Standing architectural note (unchanged, for a future ADR call)

werust's core `verify` gate builds the WHOLE workspace incl. the webkit-linking crate,
so it needs GTK/WebKitGTK on every runner and is pinned to what the runner ships. wezig
keeps webkit OFF its core gate (dedicated leg, ADR-0007). Adopting that split (feature-
gate the webview backend out of the default build) remains a separate ADR-level option.
