---
title: "macOS: the WKWebView `Renderer` backend (engine only, no window chrome)"
slug: macos-wkwebview-renderer-backend
blockedBy: []
covers: []
needsAnswers: true
---

## What to build

The ENGINE half of the macOS desktop shell, split out so it can land and be reviewed without the chrome painting. Its sibling `macos-appkit-window-and-chrome` builds the window on top of it. From the `macos-desktop-build` cut prescribed by `docs/adr/0011-webview2-for-windows.md`; the ADR's Amendment 1 funds it.

A `Renderer` implementation over **WKWebView** on macOS. NOT WebKitGTK via Homebrew (that binary would need the user to have Homebrew's WebKitGTK, which is not a distribution story), and NOT a cross-platform GUI toolkit (werust has deliberately not adopted one).

**Where things live** (both premises an earlier version of this task got wrong):

- The `Renderer` trait is `crates/renderer/src/lib.rs` (`pub trait Renderer`, line 695), NOT `crates/webview-renderer`.
- `crates/webview-renderer` depends on `gtk4` and `webkit6` UNCONDITIONALLY, so nothing in it compiles on macOS. This backend needs its own crate (or a cfg-gated sibling), and `crates/webview-renderer/src/offthread.rs` (genuinely toolkit-free: it imports only `fetcher`, `renderer`, `werust_core` and the crate's own `SharedLifecycle`) must MOVE to a shared home rather than be copied.

**Lean on the iOS edge, which is already a working WKWebView `Renderer` backend** driving the shared Rust core (`crates/werust-ios/rust`, `IosBackend` / `CoreSession`). The engine plumbing is largely the same; what differs on macOS is the host (an `NSView` in an `NSWindow` rather than a `UIViewController`), and that is the sibling task's problem. Where iOS logic is genuinely shared, extract rather than fork it, and say which parts you extracted.

**Trust hooks are the qualification bar, not rendering** (ADR-0001): this backend qualifies only when `ipfs://` custom-scheme interception AND EIP-1193 provider injection both work. A backend that renders but cannot serve verified content is not a werust backend.

**Confirm the origin behaviour AT RUNTIME, and write it down.** WebKit is expected to give `WKURLSchemeHandler`-served documents real tuple origins, which is why macOS is the better-placed platform, but this repo's iOS parity on that point is a recorded MECHANISM ANALYSIS whose runtime confirmation still awaits a Mac (`docs/spikes/mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`, "iOS parity"). Confirming it on macOS RETIRES that caveat for both platforms, so it is worth doing deliberately, in the spirit of the Windows probe: assert the document origin, a same-origin `fetch` that fires the handler, and a non-throwing `pushState`. `crates/windows-origin-probe` is the shape to copy, including its negative control.

**Scope boundary: no window chrome.** No URL bar, no trust indicator, no menus, no debug view. A minimal host (a hidden or bare `NSWindow`/`NSView`) is fine and expected: this task proves the SEAM, not the product surface. No signing, no packaging.

**Verification honesty (ADR-0011 Amendment 1):** this cannot be verified on real hardware from the development machine, so state explicitly what CI proved versus what remains analysis awaiting a Mac. The `macos-14` runner already exists in `.github/workflows/`.

## Acceptance criteria

- [ ] A `Renderer` implementation over WKWebView compiles and runs on macOS, with NO widening of the trait.
- [ ] It does not live in a crate that unconditionally depends on gtk4/webkit6; `offthread.rs` is MOVED to a shared toolkit-free home, not copied.
- [ ] Navigation, history, the load lifecycle, the script-message bridge and custom-scheme interception all go through the seam.
- [ ] Both trust hooks work: an `ipfs://<cid>` URL loads hash-verified content, and a page sees the native EIP-1193 `window.ethereum`.
- [ ] The origin behaviour is CONFIRMED at runtime on macOS (document origin, same-origin `fetch` that fires the handler, `pushState`), recorded, and the iOS mechanism-analysis caveat is updated to say what is now measured.
- [ ] A CI job on the existing `macos-14` runner builds and exercises the backend; trait-contract tests cover what is testable without a Mac.
- [ ] What CI proved versus what still awaits real hardware is stated explicitly.
- [ ] The repo `verify` gate on Ubuntu stays green (the macOS half is `cfg`-gated; use the repo's source-shape test pattern where the gate cannot compile it).

## Prompt

> Goal: the ENGINE half of the macOS shell, no chrome. Implement the `Renderer` trait (`crates/renderer/src/lib.rs:695`) over WKWebView on macOS, in its own crate (`crates/webview-renderer` depends on gtk4/webkit6 unconditionally and cannot host it), MOVING the toolkit-free `offthread.rs` to a shared home rather than copying it. Lean on the existing iOS WKWebView backend (`crates/werust-ios/rust`, `IosBackend`/`CoreSession`) and extract what is genuinely shared instead of forking it. A backend qualifies on the TRUST HOOKS (`ipfs://` interception + EIP-1193 injection), not on rendering. Confirm the `WKURLSchemeHandler` origin behaviour AT RUNTIME the way `crates/windows-origin-probe` did on Windows, negative control included, since that also retires this repo's recorded iOS mechanism-analysis caveat. A hidden or bare NSWindow host is fine: the window, URL bar, trust indicator, menus and debug view are the sibling task `macos-appkit-window-and-chrome`. No signing, no packaging. State plainly what CI proved versus what awaits a Mac.

## Requeue 2026-07-30

CONDUCTOR HANDOFF (2026-07-30, drive-tasks). Gate 2 blocked this correctly: acceptance criterion 5 (the origin behaviour CONFIRMED at runtime on macOS) is undelivered, and expected.json is a PREDICTION, not a recording. That is not the agent's fault: a worker cannot reach CI on the repo it works in, and workflow_dispatch is refused for macos-renderer.yml because the workflow is not on the default branch yet. The conductor opened PR #2 from THIS branch purely as a CI vehicle so the macos-14 leg can run against this code; the branch was rebased onto current main so GitHub can compute a merge ref. The measured result will be appended to this task body before the next dispatch. DO NOT re-derive the answer by hand and DO NOT relabel the prediction as a measurement: wait for the recorded run in the task body, then re-stamp expected.json with the real OS/WebKit build, and correct the README's 'What still awaits a Mac' section and the DIAGNOSIS addendum to say what is now measured versus what remains analysis.


## MEASURED (conductor, 2026-07-30) — criterion 5 is now answerable; re-stamp, do not re-derive

The `macos-renderer` leg RAN against this branch's code on a real `macos-14` runner: **[run 30563185521](https://github.com/wighawag/werust/actions/runs/30563185521)**, `macos-origin-probe` compiled against the real SDK and executed. How it was reached, for the record: `workflow_dispatch` was refused while the workflow was absent from the default branch, and a PR could not run it either (GitHub cannot build a merge ref for a conflicting PR), so the workflow file was landed on `main` first (commit `4d83ce8`) and then dispatched with `--ref` at this branch.

**VERDICT: `registered-ipfs-scheme`.** A `WKURLSchemeHandler`-served document gets a REAL `ipfs://<cid>` tuple origin. macOS 14.8.7 (Build 23J520), AppleWebKit/605.1.15. `+[WKWebView handlesURLScheme:@"https"]` measured `true`, which is why there is no case B on WebKit.

The run went RED on exactly ONE field, and it is the one the DECISIONS block already named as least-confident: **case A `secure_context` was predicted `false`, measured `true`.** WebKit grants the handler-served origin a secure context with no `TreatAsSecure` equivalent needed. That is BETTER than predicted and it does not change the verdict. The negative control behaved exactly as designed (origin `null`, `fetch` `reject:TypeError`, `pushState` `throw:SecurityError`, handler never fired), so case A passing is evidence, not a tautology.

Verbatim report from the run:

```json
{

  "os_version": "Version 14.8.7 (Build 23J520)",
  "webkit_user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
  "https_is_handled_natively": true,
  "cid": "bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq",
  "case_a": {
    "page_url": "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/",
    "navigation": "completed:success",
    "origin": "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq",
    "secure_context": true,
    "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    "fetch": "ok:200",
    "fetch_handler_fired": true,
    "push_state": "ok:/blog/",
    "module_script": "ok:module",
    "css_font_handler_fired": true,
    "service_worker": "reject:TypeError",
    "handler_uris": [
      "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/",
      "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/blog/__data.json?x-sveltekit-invalidated=01",
      "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/probe.mjs",
      "ipfs://bafybeidbbasdtwcrvqkwk4hf5k3apzuc6txfje524zhiih5a2b4rtwpfzq/probe.woff2"
    ],
    "harness_error": null
  },
  "case_control": {
    "page_url": "about:blank",
    "navigation": "completed:success",
    "origin": "null",
    "secure_context": false,
    "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
    "fetch": "reject:TypeError",
    "fetch_handler_fired": false,
    "push_state": "throw:SecurityError",
    "module_script": "reject:TypeError",
    "css_font_handler_fired": false,
    "service_worker": "unavailable",
    "handler_uris": [],
    "harness_error": null
  }
}
```

**What to do with this (do NOT re-derive it by hand, and do NOT relabel a prediction as a measurement):**

1. Re-stamp `docs/spikes/macos-wkwebview-renderer-backend/expected.json` from the values above, including the real `os_version` / WebKit build, so it becomes a RECORDING like the Windows sibling's, and its `recorded` field states the run URL.
2. Commit the verbatim report as `docs/spikes/macos-wkwebview-renderer-backend/probe-report-2026-07-30.json`, mirroring `windows-ipfs-origin-probe-on-ci`.
3. Correct the README's "What still awaits a Mac" section and the `mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md` addendum: the origin mechanism is now MEASURED on WebKit (and therefore settled for iOS too, which shares `WKURLSchemeHandler`), while anything the leg did not exercise stays honestly listed as awaiting hardware.
4. Say plainly which of the leg's OTHER steps (the crate build, the trust-hooks smoke example) passed in that run and which did not, rather than claiming the whole job is green: the run's exit was non-zero because of the `secure_context` mismatch above.
