---
title: "Give the web-platform rows the SAME evidence class on all five edges: probe the four that were never measured"
slug: matrix-web-platform-rows-are-measured-on-every-edge
blockedBy: [android-enable-dom-storage-and-guard-web-platform-parity]
covers: []
---

## What to build

The `web-storage` row landed with `android-enable-dom-storage-and-guard-web-platform-parity` marked `implemented` on all five platforms, but its cells do NOT rest on equal evidence, and the row says so rather than hiding it:

- **Android** is measured on-device, before and after, by an instrumented probe that reads `window.localStorage` back and round-trips it.
- **desktop** rests on a field report (the human ran `mandalas.eth`, which uses `localStorage`, and it worked) — not a probe that read the property back.
- **macOS, Windows, iOS** rest on the engine default plus the absence of any edge-side disabling. No probe on those platforms has ever read `window.localStorage`.

That asymmetry is honest but it is not a guard. The whole lesson of the Android bug is that "the engine surely does the right thing" is precisely the assumption that failed, and it failed on the one edge where someone actually looked. This task closes the gap: give the four unmeasured edges a real probe, so the web-platform rows are MEASURED everywhere rather than reasoned about on four platforms out of five.

Use each edge's existing measurement route rather than inventing a new harness. This repo already has the pattern: the macOS and Windows origin probes run as CI legs and record their verdicts (with a recorded-verdict guard so a PREDICTION can never be committed where a MEASUREMENT belongs), and the mobile edges have hand-run instrumented probes. Note the standing convention: a CI-measurable criterion needs its CI LEG on `main` FIRST — so if this task needs a new workflow leg, that leg lands in its own change before the measuring work is dispatched.

Where an edge genuinely cannot be measured today, the outcome is a clearly named limit in the row, not a quiet upgrade of the claim.

## Acceptance criteria

- [ ] `window.localStorage` and `window.sessionStorage` are read back and round-tripped by a real probe on desktop, macOS, Windows and iOS, on the origin each edge actually serves content on.
- [ ] The measurements are captured durably and referenced from the `web-storage` row, and each cell's prose is updated to state its now-equal (or still-unequal, honestly named) evidence class.
- [ ] Any edge that turns out NOT to conform is filed as a bug with its own task, and its cell stops claiming `implemented`.
- [ ] Any new CI leg this needs is on the default branch and GREEN before it is relied on, per the repo's standing convention.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `android-enable-dom-storage-and-guard-web-platform-parity` (it lands the row whose evidence gap this closes).

## Prompt

> The `web-storage` row in `docs/platform-capability-matrix.toml` is `implemented` on all five platforms but rests on unequal evidence: Android is measured on-device, desktop rests on a field report, and macOS/Windows/iOS rest on "the engine enables it by default and no edge code disables it". The Android bug is the proof that this reasoning is not a guard. Probe `window.localStorage` and `window.sessionStorage` for real on the four unmeasured edges, on the origin each one actually serves content on, using each edge's EXISTING measurement route (the macOS and Windows origin-probe CI legs; the iOS instrumented probe) rather than a new harness. Read the repo conventions in `CONTEXT.md` first: a CI-measurable criterion needs its CI leg on `main` before the work is dispatched, and a prediction must never be committed where a measurement belongs. Update each cell's prose to its real evidence class; if an edge does not conform, file it as a bug and stop claiming `implemented` for it.
