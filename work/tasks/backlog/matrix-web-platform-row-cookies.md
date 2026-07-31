---
title: "Give the parity matrix a `cookies` web-platform row, and decide the third-party-cookie position for ALL edges"
slug: matrix-web-platform-row-cookies
blockedBy: [android-enable-dom-storage-and-guard-web-platform-parity]
covers: []
---

## What to build

A `cookies` WEB-PLATFORM row for `docs/platform-capability-matrix.toml`, in the category `android-enable-dom-storage-and-guard-web-platform-parity` opened with `web-storage`.

This row carries a POLICY question the storage row did not, and that is the real reason it is a separate task rather than a footnote.

Android is measured (`docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`): `CookieManager.acceptCookie()` is `true`, `document.cookie` round-trips, and `acceptThirdPartyCookies(webView)` is `false` — the `WebView` default. That third-party default happens to land exactly where a privacy-protecting browser would choose to land (`CONTEXT.md`, `docs/adr/0001`), so the storage task recorded it as a deliberate position rather than changing anything. But it was recorded for ANDROID ONLY, and it is not expressed anywhere in shared code: every edge silently inherits its engine's default.

So this task has two halves, and the second is the one that matters:

1. **Measure** cookie behaviour on all five edges — first-party round-trip, and whether third-party cookies are accepted — and fill the row honestly with an evidence class per cell.
2. **Decide, once, whether werust BLOCKS third-party cookies as a stated position**, and record it where a later reader will find it rather than leaving five engine defaults that happen to agree today. If the five edges DISAGREE, that is a parity gap of exactly the kind this matrix exists to surface. If the answer is "block everywhere, deliberately", it is a trust/privacy stance and plausibly meets the ADR bar (hard to reverse, surprising without context, a real trade-off: some legitimate flows break).

Do NOT quietly enable third-party cookies on any edge to make a cell uniform. Uniformity in the permissive direction is the wrong resolution for this project.

## Acceptance criteria

- [ ] `docs/platform-capability-matrix.toml` gains a `cookies` row with an explicit, honest cell for all five platforms, each stating its evidence class, and the parity guard passes with no weakening.
- [ ] Third-party cookie acceptance is MEASURED on every edge that can be measured, and any disagreement between edges is named in the row rather than smoothed over.
- [ ] The third-party-cookie POSITION is recorded durably (an ADR if it meets the ADR bar, else a linked decision note), and covers all five edges rather than Android alone.
- [ ] No edge is changed to be MORE permissive in order to make the row uniform.
- [ ] Tests mirror the repo's existing style, and any probe that does not run in CI says so plainly.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `android-enable-dom-storage-and-guard-web-platform-parity` (it establishes the row category and the Android cookie measurement).

## Prompt

> Add a `cookies` capability row to `docs/platform-capability-matrix.toml`, following the `web-storage` row as the worked example of the per-cell evidence-class honesty standard, and read `docs/adr/0005-platform-capability-parity-guard.md` first. Android is already measured (`docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`): first-party cookies work, third-party cookies are OFF by `WebView` default, and that was recorded as a deliberate privacy position for Android only. Measure the other four edges. The load-bearing half of this task is the POSITION: decide whether werust blocks third-party cookies deliberately on ALL edges and record that decision durably (ADR if it meets the bar in `work/protocol/ADR-FORMAT.md`), rather than leaving five engine defaults that coincidentally agree. Never resolve a disagreement by making an edge more permissive.
