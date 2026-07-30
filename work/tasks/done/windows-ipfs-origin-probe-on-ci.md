---
title: "Windows origin probe (gate 0): does a registered `ipfs://` scheme in WebView2 give a real same-origin `fetch` + `pushState`, or must Windows use the internal-https origin map?"
slug: windows-ipfs-origin-probe-on-ci
blockedBy: []
covers: []
---

## What to build

Gate 0 of the Windows work, prescribed by `docs/adr/0011-webview2-for-windows.md` (step 0 of its breakdown, kept by Amendment 1 when the DEFER was overturned). **No Windows shell code is written until this answers.** It needs no Windows hardware: it runs on a `windows-latest` GitHub runner.

**The question.** werust does not need "a document renders"; it needs a page served from an intercepted `ipfs://` URL to do a same-origin `fetch('/blog/__data.json')` and a `history.pushState` without throwing. On Android that failed: an intercepted document gets an OPAQUE origin, so Blink rejects the fetch before the network stack and `pushState` throws `SecurityError`, killing every SvelteKit client-side navigation with no signal werust could see. That cost a field bug and the `crates/werust-android/rust/src/origin_map.rs` internal-`https://<cid>.ipfs.werust.invalid` workaround. WebView2 is the same Blink engine, but unlike Android's interception hook it has real scheme REGISTRATION (`ICoreWebView2CustomSchemeRegistration` with `HasAuthorityComponent = TRUE` + `TreatAsSecure`), which Microsoft documents as giving an http-like tuple origin. Whether that documented behaviour actually holds for `fetch` is an OPEN WebView2 bug ([WebView2Feedback #4328](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4328), open since 2024-01-28), and the neighbouring behaviour regressed in stable runtime 144 in January 2026 ([#5495](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5495)). The runtime is evergreen and cannot be pinned. So this is measured, not read.

**The shape** is the analogue of the committed Android probe `crates/werust-android/app/src/androidTest/.../SpaClientNavOriginTest.kt`, and the design is already written down in `docs/spikes/windows-platform-research/README.md` section 4: canned bytes, no werust core, no IPFS, no network. Two cases:

- **Case A — the real registered scheme:** `ipfs://` registered with `HasAuthorityComponent` + `TreatAsSecure`.
- **Case B — the internal origin:** `https://<cid>.ipfs.werust.invalid`, the mechanism `origin_map.rs` already implements for Android.

For each case, assert three things: the document's `origin` string, whether a same-origin `fetch` RESOLVES **and** fires the `WebResourceRequested` handler (both, not either), and whether `pushState` throws.

**Bindings:** `webview2-com` + `webview2-com-sys` (0.39.1, what `wry` itself uses), never the abandoned `webview2` crate (last release 2021, predates the API).

**The verdict is the deliverable.** Case A passing means the Windows shell serves real `ipfs://` origins like desktop and iOS. Case A failing means `origin_map.rs` is promoted from an Android module to a shared one and the Windows edge maps URLs exactly as Android does, which is also what Tauri's `wry` ships on Windows (`src/custom_protocol_workaround.rs`). Either way the mechanism is DECIDED by measurement before any shell exists.

**Make it re-runnable.** The runtime is evergreen and this corner has demonstrably regressed in stable, so the probe must be a job that can be re-run later, not a one-off transcript. It should also record which runtime version the runner had.

## Acceptance criteria

- [ ] A probe that runs on a `windows-latest` GitHub runner, needing no Windows hardware and no network (canned bytes only).
- [ ] Both cases (registered `ipfs://` scheme; internal `https://<cid>.ipfs.werust.invalid`) are exercised, and for each the document origin, the same-origin `fetch` result INCLUDING whether the interception handler fired, and the `pushState` outcome are asserted and reported.
- [ ] The WebView2 Runtime version present on the runner is recorded with the result, since the behaviour is evergreen and version-sensitive.
- [ ] A recorded VERDICT (a spike DECISIONS entry or an observation) naming which serving mechanism the Windows shell will use, in a form the shell task can build to without re-litigating it.
- [ ] The probe is re-runnable on demand (a workflow entry point, not a one-off local transcript).
- [ ] No werust core code, no IPFS, no shell: this task ships a probe and an answer, nothing else.

## Prompt

> Goal: settle, by MEASUREMENT on a `windows-latest` runner, whether a WebView2-registered `ipfs://` scheme (`ICoreWebView2CustomSchemeRegistration` with `HasAuthorityComponent` + `TreatAsSecure`, via the `webview2-com` crate) gives a document a REAL tuple origin, such that a same-origin `fetch` resolves AND fires `WebResourceRequested`, and `pushState` does not throw. Compare against case B, the internal `https://<cid>.ipfs.werust.invalid` origin that `crates/werust-android/rust/src/origin_map.rs` already implements. This is the analogue of the committed Android probe `SpaClientNavOriginTest.kt`; the design is in `docs/spikes/windows-platform-research/README.md` section 4. Canned bytes, no core, no IPFS, no network. Record the runner's WebView2 Runtime version with the result, because the runtime is evergreen and this exact corner regressed in stable 144 in January 2026. Deliver a recorded verdict naming the mechanism the Windows shell will use. Do NOT build any shell.
