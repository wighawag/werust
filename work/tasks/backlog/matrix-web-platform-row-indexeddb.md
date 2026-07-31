---
title: "Give the parity matrix an `indexeddb` web-platform row, measured on every edge"
slug: matrix-web-platform-row-indexeddb
blockedBy: [android-enable-dom-storage-and-guard-web-platform-parity]
covers: []
---

## What to build

The second WEB-PLATFORM row for `docs/platform-capability-matrix.toml`. The first (`web-storage`) landed with `android-enable-dom-storage-and-guard-web-platform-parity`, which also recorded WHY the category exists: the matrix's other rows are all werust FEATURES, so nothing in it ever asked whether the web platform itself behaves the same on all five edges — and that is exactly why the parity guard could not fire on the `window.localStorage` bug (ADR-0005's ceiling).

IndexedDB earns its own row rather than riding on `web-storage` because it is a different API with a different failure mode, and because wallets and dapps depend on it far more heavily than on `localStorage`: a wallet that cannot open an IndexedDB database does not degrade, it fails.

One edge is already measured. `crates/werust-android/app/src/androidTest/.../WebStorageTest.kt` opens a database, puts and gets a record on the internal per-CID origin, and found that IndexedDB works on Android with `domStorageEnabled` OFF as well as ON — so the historical claim that it depends on that switch does NOT hold on the API levels this app supports (`docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`). The other four edges are unmeasured.

MEASURE before filling a cell. A predicted cell in the slot a measurement belongs in is the failure this repo already guards against elsewhere; if an edge cannot be measured today, say what the evidence class actually is in the row's prose (the `web-storage` row is the worked example) rather than inventing a stronger claim.

Note the origin dimension while measuring: on four platforms an `ipfs://<cid>` origin is the real thing, and on Android it is the mapped `https://<cid>.ipfs.werust.invalid`. IndexedDB on a genuinely opaque origin throws, so an edge that serves content on an opaque origin would fail this row for a REASON, and the reason belongs in the cell.

## Acceptance criteria

- [ ] `docs/platform-capability-matrix.toml` gains an `indexeddb` row with an explicit, honest cell for all five platforms, and the parity guard passes with no weakening.
- [ ] Each cell's prose states its EVIDENCE CLASS (measured on-device / measured in CI / engine default with no edge-side disabling), never a stronger claim than what was actually observed.
- [ ] Where an edge is measured, the measurement is captured durably (a spike doc or an extension of the existing measurements doc) and referenced from the row.
- [ ] Tests mirror the repo's existing style: a source-shape test on the Ubuntu gate for anything an edge must keep doing, and any on-device/CI probe marked plainly with whether it runs in CI.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `android-enable-dom-storage-and-guard-web-platform-parity` (it establishes the web-platform row category and the Android measurement this row starts from).

## Prompt

> Add an `indexeddb` capability row to `docs/platform-capability-matrix.toml`, the second WEB-PLATFORM row after `web-storage`. Read `docs/adr/0005-platform-capability-parity-guard.md` first, then the `web-storage` row as the worked example of the honesty standard (it states an evidence class per cell rather than one flat `implemented`). Android is already measured on-device by `WebStorageTest.kt` and recorded in `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md`: IndexedDB works there and does NOT depend on `domStorageEnabled`. MEASURE the other four edges rather than predicting them, and where you cannot measure, say so in the cell instead of overclaiming. Watch the origin dimension: IndexedDB throws on an opaque origin, so an edge serving content opaquely fails this row for a specific reason that belongs in the cell.
