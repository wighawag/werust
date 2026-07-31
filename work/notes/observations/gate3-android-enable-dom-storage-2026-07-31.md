---
title: "Gate-3 verdict: android-enable-dom-storage-and-guard-web-platform-parity (APPROVE after a requeue) — localStorage works, and the matrix has its first web-platform row"
date: 2026-07-31
status: open
reviewOf: android-enable-dom-storage-and-guard-web-platform-parity
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged after one requeue. `settings.domStorageEnabled = true` is the one-line fix; the rest of the diff is why this bug could happen at all.

## The measurement is the best thing in this diff, and it overturned a received belief

The agent ran a real emulator (`Medium_Phone_API_36.1`, Android 16 / API 36, System WebView 142.0.7444.174) with a new instrumented probe, varying exactly one setting. Verbatim, with `domStorageEnabled` at Android's default `false`:

```
window.localStorage: null
window.sessionStorage: [object Storage]
window.indexedDB: [object IDBFactory]
localStorage round-trip: throw:TypeError
sessionStorage round-trip: ok:werust-round-trip
indexedDB round-trip: ok:werust-round-trip
document.cookie round-trip: ok:werust-probe=werust-round-trip
```

Two things fall out of that, and neither was predictable from documentation:

1. **`domStorageEnabled` gates `localStorage` ONLY.** `sessionStorage` is a working `Storage` object with the setting off, and **IndexedDB round-trips fine** — which contradicts the widely-repeated claim that IndexedDB depends on this switch. I wrote that dependency into the task as something to check, expecting it to be true. It is not, on the API levels werust ships against. This is exactly why the task said "measure, do not assume", and it is a good reminder that the instruction has to apply to the instruction-writer too.
2. **The bug was narrower than the symptom suggested.** A dapp using IndexedDB or cookies was never broken on Android; only `localStorage` was. That matters for judging blast radius honestly rather than dramatically.

With the fix, all four round-trip, and a RELOAD of the same origin reads back what the previous load wrote — the persistence a dapp actually depends on, not just a property that stringifies nicely.

## The requeue: an over-claim in the very row added to prevent over-claims

Round 1 marked all five platforms `implemented` on the strength of "the engine enables DOM storage by default and no edge disables it", having measured exactly one of them. Gate 2 blocked it, and I verified the claim before acting: this repo has ALREADY MEASURED that engine capabilities on custom-scheme origins are **scheme-gated**. From `windows-ipfs-origin-probe-on-ci`'s committed report, on a registered `ipfs://` origin with `HasAuthorityComponent` + `TreatAsSecure` — a real, secure tuple origin where `fetch` and `pushState` both work — `serviceWorker.register` still rejects with `InvalidStateError`.

So "no toggle disables it" says nothing about whether `localStorage` works on an `ipfs://` origin in WKWebView or WebView2, which is exactly the origin those three platforms serve. A machine-readable `implemented` on three unmeasured edges would have defused the one row added to stop this class. ADR-0005 records the same over-claim being corrected once before.

The row now reads `android = implemented` (measured), `macos`/`windows`/`ios` = `stubbed` against a resolvable slug, `desktop = implemented` on the human's field report.

**And the follow-on got cheaper than anyone assumed.** I pointed it at machinery that already exists: `crates/macos-origin-probe` and `crates/windows-origin-probe` already run on real `macos-14` and `windows-latest` runners, already load a real `ipfs://` origin, and already report per-capability facts on it — `service_worker` is literally a field in both reports. Adding `local_storage` answers macOS and Windows **by measurement in CI, with no hardware and no human**, and the WebKit result carries to iOS by port equivalence exactly as ADR-0011 Amendment 3 argued for the origin verdict.

## The durable half

`docs/platform-capability-matrix.toml` had 24 rows and not one covered the web platform — every row was a werust FEATURE. That is why the guard built to stop a capability shipping on one platform could not see this. It now has a `web-storage` row, plus three authored follow-ons for IndexedDB, cookies and service workers, and a `WEBSETTINGS-AUDIT.md` listing the other browser-wrong Android defaults (pinch-zoom, wide viewport, media gesture, text scaling) **without changing any of them**, which was the right scope call.

## Nits — two worth the human, the rest ratified

- **`stubbed` is being stretched.** ADR-0005 means it as "a known gap, the matrix face of a no-op'd seam method". Here it means "not established". Three cells will read to a release reader as real capability GAPS when nothing suggests a defect. Either ratify the stretch or mint an `unmeasured` state (guard + ADR + glossary together). Worth a human call, and the follow-on probe work may make it moot within a task or two.
- **The desktop cell rests on a site-level field report** (`mandalas.eth` worked on GTK) rather than a property read-back, while three edges are stubbed. Stated in-cell, which is what I asked for; a human should still confirm the line is where they want it.
- Ratified: the audit guard now reds if a future UX task enables pinch-zoom without updating the audit (deliberate friction, but it constrains tasks this one does not own); and a per-cell evidence guard weaker than its prose, which is the third "guard that cannot fail" this drive — noted rather than chased, since the row itself is young.
