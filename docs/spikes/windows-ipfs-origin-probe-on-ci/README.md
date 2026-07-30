# Windows origin probe (gate 0): the measured verdict

Task: `windows-ipfs-origin-probe-on-ci`. Design this executes: [`docs/spikes/windows-platform-research/README.md`](../windows-platform-research/README.md) section 4. Decision it closes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), step 0 of its breakdown. Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md).

The probe is `crates/windows-origin-probe`; it runs on a `windows-latest` GitHub runner via [`.github/workflows/windows-origin-probe.yml`](../../../.github/workflows/windows-origin-probe.yml) (`workflow_dispatch` for on-demand re-runs). Canned bytes, no werust core, no IPFS, no network.

## VERDICT

**The Windows shell serves REAL `ipfs://` origins, via `ICoreWebView2CustomSchemeRegistration` with `HasAuthorityComponent = TRUE` and `TreatAsSecure = TRUE`.** `origin_map.rs` is NOT promoted; it stays an Android module. Windows joins desktop Linux and iOS in showing the page the same URL werust's core speaks, and Android remains the only mapped platform.

Measured 2026-07-30 on a GitHub `windows-latest` runner (image `windows-2025-vs2026` `20260714.173.1`), **WebView2 Runtime 150.0.4078.65**. Verbatim run: [`probe-report-2026-07-30.json`](probe-report-2026-07-30.json). Pinned for re-runs: [`expected.json`](expected.json).

| | case A: registered `ipfs://` | case B: internal `https` origin | negative control: registered `ipfs://` *without* `HasAuthorityComponent` |
|---|---|---|---|
| document origin | `ipfs://bafybei…pfzq` | `https://bafybei…pfzq.ipfs.werust.invalid` | `null` (opaque) |
| secure context | yes | yes | no |
| same-origin `fetch('/blog/__data.json?x-sveltekit-invalidated=01')` | **`ok:200`** | `ok:200` | `reject:TypeError` |
| `WebResourceRequested` fired for that fetch | **yes** | yes | **no** |
| `history.pushState({}, '', '/blog/')` | **`ok:/blog/`** | `ok:/blog/` | `throw:SecurityError` |
| `<script type="module">`-shaped `import()` | `ok:module` | `ok:module` | `reject:TypeError` |
| CSS `@font-face url()` reached the handler | yes | yes | no |
| `navigator.serviceWorker.register('/sw.js')` | `reject:InvalidStateError` | `reject:TypeError` | `unavailable` |

Case A passes every check ADR-0011 named, so by that ADR's own decision rule the mechanism is the registered scheme.

## Why this result is believable (the negative control)

A probe where every case passes has measured nothing. So the run also carries a NEGATIVE CONTROL: the identical URL, the identical canned bytes and the identical page, with exactly ONE registration flag flipped (`HasAuthorityComponent = false`, and with it `TreatAsSecure`, which Microsoft documents as effective only alongside it).

The control reproduces the Android bug **verbatim**, including Blink's own message — the same sentence `crates/werust-android/rust/src/origin_map.rs` quotes from the on-device Android diagnosis:

> Fetch API cannot load `ipfs://bafybei…pfzq/blog/__data.json?x-sveltekit-invalidated=01`. URL scheme "ipfs" is not supported.

and, for the subresources:

> Access to script at '`ipfs://bafybei…pfzq/probe.mjs`' from origin '`null`' has been blocked by CORS policy: No 'Access-Control-Allow-Origin' header is present on the requested resource.

with `WebResourceRequested` never firing for either. That is the failure this probe exists to detect, detected. Case A passing on the same runner, in the same process shape, minutes apart, is therefore a real difference in WebView2 behaviour and not a harness that cannot fail.

The control is asserted on every re-run: if it ever starts PASSING, the run fails with "the probe is no longer able to detect the failure it exists to detect", because at that point nothing else in the report can be trusted either.

## What this does and does not settle

**Settled, by measurement:**

- A WebView2-registered `ipfs://` scheme with `HasAuthorityComponent` gives the document a real tuple origin (`ipfs://<cid>`), a secure context, a same-origin `fetch` that RESOLVES *and* reaches `WebResourceRequested`, and a working `pushState`. So a SvelteKit `adapter-static` client-side navigation — the thing that died on Android — works.
- [WebView2Feedback #4328](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4328) ("custom schemes don't work with `fetch()`") does NOT reproduce when `HasAuthorityComponent = true`; it reproduces exactly and only in the control, where the origin is opaque. That matches the reading in the research spike (section 1): the open bug is consistent with reporters omitting the flag.
- [#4362](https://github.com/MicrosoftEdge/WebView2Feedback/issues/4362) (CSS `url()` subresources never reaching the handler) likewise does NOT reproduce with the flag set: the `@font-face` request reached the handler in both case A and case B.
- The internal-`https` mechanism (case B) also works on WebView2, so the fallback is real and available if this ever regresses. It is simply not needed.
- The `windows-latest` runner image HAS the WebView2 Runtime preinstalled (the small unknown flagged in the research spike, section 7). No bootstrapper step was needed.

**NOT settled here, and left to the Windows shell task:**

- A service worker cannot register on the real `ipfs://` origin (`InvalidStateError`) but also failed on the internal origin here (`TypeError`, the canned `sw.js` being served from a `.invalid` host with no real network). This probe does not resolve the per-platform service-worker divergence already recorded in `work/notes/observations/service-worker-registration-differs-by-ipfs-serving-origin-2026-07-30.md`; it only confirms it is not a reason to prefer one mechanism over the other.
- The scheme-name-set constraint (registrations are fixed at environment creation, immutable for the browser-process lifetime) is REAL and was navigated here by giving each case its own process and user-data folder. ADR-0011 finding 5 already prescribes the shell's answer: create the environment LAZILY. This probe does not exercise that.
- Everything else about a Windows shell. No shell code was written; that is deliberate, and gate 0 is now the only thing that was blocking it.

## Re-running it

On demand, from the Actions tab: run the `windows-origin-probe` workflow. It also runs automatically on `main` when the probe or its recorded verdict changes.

By hand on a Windows box:

```
cargo run -p windows-origin-probe -- --expected docs/spikes/windows-ipfs-origin-probe-on-ci/expected.json --out probe-report.json
```

The WebView2 Runtime is EVERGREEN and cannot be pinned, and this exact corner regressed in stable 144 in January 2026 ([#5495](https://github.com/MicrosoftEdge/WebView2Feedback/issues/5495)). So the probe does not merely report: it ASSERTS the run against `expected.json` and exits non-zero naming the field that moved. A red `windows-origin-probe` job means the ground under the Windows shell's serving mechanism has shifted, and the recorded verdict must be re-decided (and re-recorded with the reason), not silently overwritten.

The Ubuntu `verify` gate cannot run WebView2, but it does compile and unit-test the probe's host-independent half — the decision rule, the canned site and the CLI — so the logic that turns three cases into a verdict is covered by every ordinary run of the gate.
