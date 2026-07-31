---
title: "Give the parity matrix a `service-workers` web-platform row — the one where a real cross-platform gap is already suspected"
slug: matrix-web-platform-row-service-workers
blockedBy: [android-enable-dom-storage-and-guard-web-platform-parity]
covers: []
---

## What to build

A `service-workers` WEB-PLATFORM row for `docs/platform-capability-matrix.toml`, in the category `android-enable-dom-storage-and-guard-web-platform-parity` opened with `web-storage`.

Unlike the sibling rows, this one is NOT expected to be a formality. The macOS origin probe already recorded `service_worker: reject:TypeError` on `ipfs://`, so on at least one edge service-worker registration fails on the origin werust's whole thesis is built around. That makes this the row most likely to expose a genuine capability gap, and the reason it is worth a task rather than a line in someone's notes.

Measure, on each edge, on BOTH origin shapes werust actually uses: a real `ipfs://<cid>` origin (four platforms) and the mapped `https://<cid>.ipfs.werust.invalid` origin (Android). A custom scheme is a plausible reason registration is refused, in which case Android's origin mapping might be the ONE edge where service workers work — the inverse of the storage bug, and worth knowing either way.

Then fill the row honestly. If service workers do not work on an edge, the cell is `stubbed` with a task that covers making them work, or `n-a` with a real reason — not `implemented` because the API object exists.

Consider, and record rather than decide silently, whether a service worker is even DESIRABLE on a content-addressed origin: a worker installed by one CID persists and can serve responses for that origin, which interacts with werust's trust model in ways an ordinary browser never has to think about. If that turns out to be load-bearing, it is an ADR, not a matrix cell.

## Acceptance criteria

- [ ] `docs/platform-capability-matrix.toml` gains a `service-workers` row with an explicit, honest cell for all five platforms, each stating its evidence class, and the parity guard passes with no weakening.
- [ ] Registration is MEASURED on each edge on both origin shapes (`ipfs://<cid>` and the Android internal `https://<cid>.ipfs.werust.invalid`), and the results are captured durably and referenced from the row.
- [ ] Any edge where registration fails is `stubbed` (with a task that covers it) or `n-a` (with a reason), never `implemented` because the API object is present.
- [ ] The trust interaction (a worker installed by one CID serving responses for that origin) is at least RECORDED, and raised to an ADR if it proves load-bearing.
- [ ] Tests mirror the repo's existing style, and any probe that does not run in CI says so plainly.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `android-enable-dom-storage-and-guard-web-platform-parity` (it establishes the web-platform row category and the honesty standard for the cells).

## Prompt

> Add a `service-workers` capability row to `docs/platform-capability-matrix.toml`, following `web-storage` as the worked example and `docs/adr/0005-platform-capability-parity-guard.md` for why the guard exists. Expect a REAL gap: the macOS origin probe already measured `service_worker: reject:TypeError` on `ipfs://`. Measure registration on every edge on BOTH origin shapes werust uses — the real `ipfs://<cid>` origin and Android's mapped `https://<cid>.ipfs.werust.invalid` — since a custom scheme is a plausible reason for refusal, which would make Android the only edge where they work. Fill cells from measurements only; `implemented` means registration succeeds, not that `navigator.serviceWorker` exists. Record (and raise to an ADR if load-bearing) the trust question of a worker installed by one CID serving responses for that content-addressed origin.
