---
title: Native renderer T0 — fixed v0 subset path behind the Renderer seam
slug: native-renderer-t0-subset-path-behind-seam
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [renderer-seam-trait-and-webview-backend-navigate]
covers: [10]
---

> **FORWARD-POINTER (planted by drive-tasks after `renderer-seam-trust-hook-qualification-gate` landed).** The seam now enforces trust-hook qualification via `Renderer::trust_hooks()` + the `qualify()` gate: a backend qualifies ONLY if it declares BOTH trust hooks (EIP-1193 provider injection AND `ipfs://` custom-scheme resolution). IMPORTANT: `trust_hooks()` DEFAULTS to `TrustHooks::all()` (fail-OPEN) — so if you leave it defaulted, this T0 backend will silently "qualify" even though a fixed-subset renderer does NOT actually wire provider-injection or ipfs resolution. Declare `trust_hooks()` HONESTLY for this backend: only claim a hook it genuinely satisfies. If the T0 subset path cannot yet satisfy the trust hooks (likely — it renders an allowlist subset, it is not yet a full browser backend with a live provider / scheme resolver), have it declare only the hooks it truly wires (possibly `TrustHooks::none()`), and let `qualify()` legitimately report it as not-yet-qualifying. Do NOT fail-open-declare `all()` to make the gate pass; that would defeat the thesis the gate encodes. (Criterion 3 already requires this backend to be "subject to the trust-hook qualification gate" — this note pins HOW: honest declaration, not a rubber-stamp.)

## What to build

Build a T0 native render path as a SECOND `Renderer` backend (beside the webview):
a naive subset tokenizer + allowlist tree builder, a real cascade over a small
property set, block/inline flow layout, and software text — assembled from the
pure-Rust stack where possible. This matches the fixed v0 subset the wezig Zig arm
already reached, now in Rust, and is the anchor the higher tiers extend. It renders
the v0 element/property allowlist (see `docs/conformance-tiers.md` T0), not real
arbitrary documents.

## Acceptance criteria

- [ ] A native (non-webview) `Renderer` backend renders the T0 v0 subset (allowlist elements + restricted CSS property/selector set) to a window via the seam.
- [ ] Tokenize → allowlist tree → cascade (small property set) → block/inline flow → software text is implemented end-to-end for the subset.
- [ ] The native path plugs into the SAME `Renderer` seam the webview uses (hot-swappable), and is subject to the trust-hook qualification gate.
- [ ] Tests cover the subset render path (unit + a small render assertion), mirroring the repo's style.

## Blocked by

- Blocked by `renderer-seam-trait-and-webview-backend-navigate`.

## Prompt

> Goal: the native renderer's T0 floor — a fixed v0 subset behind the `Renderer`
> seam (see `docs/conformance-tiers.md` T0, `CONTEXT.md`).
>
> Assemble from the pure-Rust stack where it helps, but T0 is deliberately a small,
> fixed subset (naive tokenizer, allowlist tree, small cascade, block/inline flow,
> software text) — NOT a real parser (that's T1, `t1-whatwg-parser-html5ever-behind-tokenizer-seam`).
> It is a second backend on the SAME seam as the webview, so it must also pass the
> trust-hook qualification gate. This is the anchor T1 extends; structure the
> tokenizer/tree-builder behind a `Tokenizer | TreeBuilder` seam so T1 can swap in
> html5ever.
>
> Done = a native backend renders the v0 subset allowlist via the seam, matching the
> wezig T0 floor.
