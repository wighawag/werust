---
title: "Give the web-platform rows the SAME evidence class on all five edges: measure `localStorage` in the probes that already run in CI"
slug: matrix-web-platform-rows-are-measured-on-every-edge
blockedBy: [android-enable-dom-storage-and-guard-web-platform-parity]
covers: []
---

## What to build

The `web-storage` row landed with `android-enable-dom-storage-and-guard-web-platform-parity`, and its cells rest on deliberately UNEQUAL evidence, which the row states per cell rather than hiding:

- **Android** — `implemented`. Measured on-device, before and after, by an instrumented probe that reads `window.localStorage` back and round-trips it (API 36 emulator, System WebView 142.0.7444.174).
- **desktop** — `implemented`. Rests on a FIELD REPORT: the human ran `mandalas.eth`, a site that uses `localStorage`, and it worked on the GTK desktop while it was `null` on Android. Behaviour on the real origin, but a site-level observation, not a property read-back.
- **macOS, Windows, iOS** — `stubbed`, pointing at THIS task. No probe on those edges has ever read `window.localStorage` back. They are not "known broken"; they are NOT ESTABLISHED.

Why the three are stubbed rather than `implemented`-with-a-caveat, because this is the whole point of the task: the tempting inference is "the engine enables DOM storage by default and no edge disables it". This repo has MEASURED that this inference is unsafe on exactly these origins. On a REGISTERED `ipfs://` origin with `HasAuthorityComponent` + `TreatAsSecure` — a real, secure tuple origin where `fetch` and `pushState` both work — Blink still rejects `navigator.serviceWorker.register('/sw.js')` with `InvalidStateError` (`docs/spikes/windows-ipfs-origin-probe-on-ci/probe-report-2026-07-30.json`, WebView2 150.0.4078.65; write-up in `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`). Engine capabilities on CUSTOM-SCHEME origins are SCHEME-GATED, and `ipfs://` is precisely the origin those three edges serve. So an engine default is not evidence, and the Android bug is the proof that "the engine surely does the right thing" is the assumption that fails on the one edge where somebody actually looked.

This task closes the gap by MEASUREMENT and flips those cells honestly.

## The prescribed route: the probes that already exist, already in CI

This is much cheaper than it looks, and it needs no hardware and no human. `crates/macos-origin-probe` and `crates/windows-origin-probe` ALREADY run on real `macos-14` and `windows-latest` runners (`.github/workflows/macos-renderer.yml`, `.github/workflows/windows-origin-probe.yml`), ALREADY load a real `ipfs://` origin served by the platform's scheme handler, and ALREADY measure per-capability facts on it: `service_worker` is literally a field of `CaseFacts` in both probes, reported from the canned page's JSON (`src/page.rs` -> `apply_report` in `src/mac.rs` / `src/win.rs`).

So:

- Add a `local_storage` field to both probes' `CaseFacts`, measured by the canned page the same way `serviceWorker` is: read `window.localStorage` back, report what it IS (a `Storage` object, `null`, or the `SecurityError` throw the platform allows on an opaque origin) and whether a set/get round-trips. `sessionStorage` alongside it, for the same cost.
- **Pin it, do not merely report it.** `expected.json` currently pins a SUBSET of the reported fields (`origin`, `secure_context`, `fetch`, `fetch_handler_fired`, `push_state`) — `service_worker` is reported but unpinned, so nothing goes red if it moves. A field that decides a matrix cell must be in the pinned block, so a future regression reds the probe naming the field. Re-record `expected.json` FROM a committed run with the reason, per the contract `crates/macos-origin-probe/tests/recorded_verdict.rs` enforces on the Ubuntu gate.
- **iOS comes free, by PORT EQUIVALENCE.** `WKWebView` on macOS and iOS are two ports of one WebKit, so what the macOS runner measures about `localStorage` on a `WKURLSchemeHandler`-served origin is a property of the port, not of AppKit — the identical argument ADR-0011 Amendment 3 made for the origin verdict, and it should be cited (and its residual risk, something iOS-specific in the shell's own wiring, named) rather than re-argued. If a hand-run iOS instrumented probe is cheap when this lands, better still.
- **desktop** is the one edge with no probe of this kind. Upgrading its cell from the field report to a read-back is the smallest remaining piece; do it if there is a route, and if there is not, say so in the cell rather than leaving the reader to guess.

Standing convention that applies: a CI-measurable criterion needs its CI LEG on `main` FIRST (`CONTEXT.md`). Both legs are already there, which is why this route was chosen; if any NEW leg turns out to be needed, it lands in its own change first. And a PREDICTION must never be committed where a MEASUREMENT belongs.

Where an edge genuinely cannot be measured today, the outcome is a clearly named limit in the row, not a quiet upgrade of the claim.

## Acceptance criteria

- [ ] `crates/macos-origin-probe` and `crates/windows-origin-probe` measure `window.localStorage` (and `sessionStorage`) on the `ipfs://` origin they already serve, and report it per case.
- [ ] The new field(s) are PINNED in each probe's `expected.json`, re-recorded from a committed verbatim run with the reason, so a future regression reds the probe naming the field.
- [ ] The `macos` and `windows` cells of `web-storage` are decided BY THAT MEASUREMENT: `implemented` if it round-trips, or a filed bug with its own task if it does not — never flipped on inference.
- [ ] The `ios` cell is decided by the macOS result via port equivalence, with the argument and its residual risk stated in the cell, or measured directly if a route exists.
- [ ] Each cell's `EVIDENCE (<platform>):` line in the row is updated to its new evidence class (the source-shape guard `crates/werust-core/tests/web_storage_edge_wiring_shape.rs` requires one per cell and will red if a cell is flipped without one).
- [ ] The `desktop` cell is either upgraded to a read-back measurement or its field-report evidence class is restated deliberately.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `android-enable-dom-storage-and-guard-web-platform-parity` (it lands the row whose evidence gap this closes).

## Prompt

> The `web-storage` row in `docs/platform-capability-matrix.toml` is `implemented` on android (measured on-device) and desktop (a field report on `mandalas.eth`), and `stubbed` against THIS task on macos/windows/ios, because no probe there has ever read `window.localStorage` back and this repo has MEASURED that engine defaults do not carry to custom-scheme origins (Blink rejects `serviceWorker.register` with `InvalidStateError` on a registered, secure `ipfs://` origin: `docs/spikes/windows-ipfs-origin-probe-on-ci/probe-report-2026-07-30.json`). Close it by MEASUREMENT using the mechanism that already exists: `crates/macos-origin-probe` and `crates/windows-origin-probe` already run on `macos-14` and `windows-latest`, already load a real `ipfs://` origin and already measure per-capability facts on it (`service_worker` is a field). Add `local_storage` (and `sessionStorage`) to both, PIN them in each `expected.json` re-recorded from a committed run with the reason (today only origin/secure_context/fetch/fetch_handler_fired/push_state are pinned, so an unpinned field can move silently), and flip the cells on what the runs say — filing a bug task if an edge does not conform. iOS follows from the macOS result by WebKit port equivalence, exactly as ADR-0011 Amendment 3 argued for the origin verdict; state the argument and its residual risk in the cell. Update each cell's `EVIDENCE (<platform>):` line; `crates/werust-core/tests/web_storage_edge_wiring_shape.rs` requires one per cell.
