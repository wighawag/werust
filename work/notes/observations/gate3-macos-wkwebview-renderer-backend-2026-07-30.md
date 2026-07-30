---
title: "Gate-3 conductor review: macos-wkwebview-renderer-backend (APPROVE, after a Gate-2 block that was right)"
date: 2026-07-30
status: open
reviewOf: macos-wkwebview-renderer-backend
verdict: approve
---

## Verdict: APPROVE (second attempt)

Merged as `733bc3e`. The FIRST dispatch was BLOCKED by Gate 2 on acceptance criterion 5 (the origin behaviour CONFIRMED at runtime), and the block was correct: nothing in the diff had ever been compiled against a macOS SDK or executed, and `expected.json` was a falsifiable PREDICTION rather than a recording. The Windows sibling had landed the opposite way, so the inconsistency was glaring.

**The decisive evidence now exists.** The `macos-renderer` leg is GREEN on `main`: [run 30565414164](https://github.com/wighawag/werust/actions/runs/30565414164), which builds the backend against a real SDK, runs its tests, exercises both trust hooks on a live WKWebView, and re-measures the origin behaviour against the recorded verdict.

## What the CI run actually proves (quoted, not paraphrased)

From the trust-hooks smoke step:

```
page report: {"origin":"ipfs://bafkreigledotdonpj4hfupvfks64l3355rea2mznztbbbujjdeqxxrcvwu","provider":"object","chainId":"0x1"}
posture:     ContentVerified
control state:   Failed
control posture: UnverifiedOrigin
PASS: both trust hooks work on a real WKWebView, and the negative control failed as it must.
```

So on a real WKWebView: an `ipfs://<cid>` page loads with the real tuple origin and a `ContentVerified` posture (the bytes hash-verified), the page sees the native EIP-1193 `window.ethereum` and round-trips `eth_chainId` to `0x1` over the script bridge, and a TAMPERED CID fails closed to `Failed` / `UnverifiedOrigin`. That is the trust-hook qualification bar from ADR-0001, met by measurement rather than by argument.

The origin probe measured **`registered-ipfs-scheme`** on macOS 14.8.7 (Build 23J520), AppleWebKit/605.1.15: a `WKURLSchemeHandler`-served document gets a real `ipfs://<cid>` tuple origin, a working same-origin `fetch` that fires the handler, and a non-throwing `pushState`, while the negative control (no handler-served origin) reproduces origin `null`, `fetch` `reject:TypeError`, `pushState` `throw:SecurityError`. `+[WKWebView handlesURLScheme:@"https"]` measured `true`, which is why WebKit has no case B: the Android/Windows internal-`https` fallback is not constructible here.

**This settles iOS too.** macOS and iOS share the `WKURLSchemeHandler` mechanism, so this repo's long-standing recorded MECHANISM ANALYSIS (`mobile-ronan-eth-buttons-no-navigation/DIAGNOSIS.md`, "iOS parity") is now backed by a runtime measurement rather than reasoning.

## The re-record contract had its first real exercise, and it behaved well

The dispatched run went RED on exactly one field: case A `secure_context`, predicted `false`, measured `true` (WebKit needs no `TreatAsSecure` equivalent). That is the field the DECISIONS block had already flagged as least-confident, it is BETTER than predicted, and it does not touch the verdict, which rests only on origin + fetch + handler-fired + pushState. The re-stamped `expected.json` records the change WITH its reason in the provenance line rather than silently overwriting it, which is the precedent every future re-record should follow.

## How the measurement was reached (worth remembering, it will recur)

A worker cannot reach CI on the repo it is working in. `workflow_dispatch` was refused because `macos-renderer.yml` did not exist on the default branch, and the PR route failed too: GitHub cannot build a merge ref for a CONFLICTING PR, and the branch had a modify-versus-rename conflict (the requeue handoff note edited the task body on `main` while the branch had already done the backlog-to-done move). The fix was to land the workflow file on `main` first (`4d83ce8`), which makes `workflow_dispatch --ref <work-branch>` legal, then dispatch it at the branch.

One honest side effect: that landing commit ALSO fired the leg on `main`, where the crates did not yet exist, so run 30563154828 failed in 15s. My commit message claimed the file would be inert on `main`; that was wrong, because the path filter includes the workflow file itself. It self-healed the moment the code landed.

## Acceptance criteria, ticked

- [x] A `Renderer` impl over WKWebView compiles and runs on macOS, with no trait widening (a test pins the seam's method list).
- [x] Not in a gtk4/webkit6-bound crate: `crates/macos-renderer` is new, and the toolkit-free half was MOVED into a new `crates/webview-shared` (it could not go in `crates/renderer`, which would have been a dependency cycle).
- [x] Navigation, history, load lifecycle, script bridge and custom-scheme interception all go through the seam.
- [x] Both trust hooks work, proven on a live WKWebView with a negative control.
- [x] Origin behaviour CONFIRMED at runtime, recorded, and the iOS caveat updated to what is now measured.
- [x] A CI job on the existing `macos-14` runner builds and exercises it; host-independent tests run in the ordinary gate.
- [x] What CI proved versus what awaits hardware is stated explicitly.
- [x] The Ubuntu gate stays green.

## Nit triage (7 non-blocking findings)

**Tasked by me:** `macos-spike-doc-accuracy-and-harness-guard`, covering the committed `typecheck-macos-from-linux.sh` doing `rm -rf` on a caller-supplied `SCRATCH_DIR` with no temp-root guard (a typo eats a working directory), plus two doc claims that do not match reality (the README says the leg runs on PRs touching the recorded verdict, but the path filter omits the docs directory; and it describes five `webview-shared` tests as lifecycle tests when `lifecycle.rs` has none).

**For the human, three ratifications:** the verifying `ipfs://` route spawns one raw `std::thread` per intercepted request with no pool or cap, where the WebKitGTK sibling uses `gio::spawn_blocking`; a scheme registered AFTER the WKWebView is realised is dropped with only a stderr line, never intercepted and never reported through the seam (recorded, and the trait returns unit, but it is a silent-ish failure mode); and the `expected.json` re-record precedent described above.

**Naming, for a later rename rather than now:** `crates/macos-renderer` is platform-named while `crates/webview-renderer` is technology-named for what is now only the WebKitGTK backend, and the genuinely generic home is `crates/webview-shared`. With two system-webview backends in the tree and a third coming, that trio will read wrong.
