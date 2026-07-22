---
title: Gate-3 verdicts — fail-closed-trust-hook-default + CI-webkitgtk-deps fix — both APPROVE
date: 2026-07-22
kind: observation
verdict: APPROVE
---

## fix-trust-hooks-fail-closed-default — APPROVE ✅ (commit on main 452ca7d area)

Human-ratified flip of the trust-hook qualification default.

- ✅ `Renderer::trust_hooks()` now defaults to `TrustHooks::none()` (fail-closed), with
  a doc-comment stating trust is never inherited by omission — a backend qualifies ONLY
  if it explicitly declares the hooks it wires.
- ✅ The real `WebViewRenderer` backend now EXPLICITLY overrides `trust_hooks()` to
  `TrustHooks::all()` (it genuinely wires provider injection + ipfs). Its two
  qualification tests were updated to assert it qualifies BECAUSE it declares — proving
  the flip did not silently disqualify the real backend.
- ✅ Native T0 backend unaffected (already declared `none()`); a default-relying backend
  is now disqualified (the inverse of the old fail-open behaviour).
- Nit: no Decisions block (the FakeBackend manual-Default + webview test rewrite are
  forced consequences of the flip, not free choices). Benign.

Resolves the fail-open design flag raised at the trust-hook-gate Gate-3.

## fix-ci-verify-missing-webkitgtk-system-deps — APPROVE ✅ (merged, main advanced)

Real CI bug: `verify.yml` + release.yml's `verify` job ran `cargo build` of the whole
workspace on a bare `ubuntu-latest` without the GTK/WebKitGTK system libs the
`webview-renderer` + `werust` crates link, so `cargo build` failed with `glib-2.0 not
found` (reported by the human from a live CI run).

- ✅ Both workflows now `sudo apt-get install -y --no-install-recommends pkg-config
  libwebkitgtk-6.0-dev` BEFORE the compile steps (`libwebkitgtk-6.0-dev` owns
  `webkitgtk-6.0.pc` and pulls in `libgtk-4-dev` + `libglib2.0-dev`).
- ✅ BONUS FIX (agent-initiated, correct): the goreleaser desktop leg previously
  installed `libwebkit2gtk-4.1-dev` — the WRONG ABI (the crate binds `webkit6` 0.5 /
  `gtk4` 0.10, which need `webkitgtk-6.0.pc`), so that leg would have failed to link at
  release time. Corrected to `libwebkitgtk-6.0-dev`. A real latent release bug fixed.
- Gate command unchanged (system-dep install is a runner concern, not part of the
  `verify` string). Nit: no Decisions block for the ABI correction. Benign.

## Architectural note for the human (NOT acted on — an ADR-level decision)

werust's core `verify` gate builds the WHOLE workspace including the webkit-linking
crate, which is WHY the gate needs these system libs on every runner. wezig deliberately
keeps WebKitGTK OFF its core `gate` and runs webkit work in a DEDICATED CI leg (its
ADR-0007), with `xvfb` + `WEBKIT_DISABLE_*` env. Whether werust should adopt that split
(feature-gate the webview backend out of the default gate build so the core gate stays
hermetic + fast) is a separate ADR-level decision, flagged for a future call.
