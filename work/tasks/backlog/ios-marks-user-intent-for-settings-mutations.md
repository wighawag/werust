---
title: "iOS supplies the intent signal for settings mutations, and refuses one a page started"
slug: ios-marks-user-intent-for-settings-mutations
spec: settings-mutations-require-user-intent
blockedBy: [settings-mutation-requires-marked-user-intent-in-core-and-on-gtk]
covers: [6, 7]
---

## What to build

The core now applies a `werust://settings` mutation only for a MAIN-FRAME request whose navigation the chrome MARKED as intended, and the GTK edge supplies that mark. Do the same for iOS.

iOS reaches the settings page through an EXPLICIT `WKURLSchemeHandler` registered for the `werust` scheme (WKWebView will not hand an unregistered custom scheme to any handler, which is why that per-scheme registration exists), routing through the iOS FFI into the shared core. That handler fires for the main document AND every sub-resource, which is what the core gate exists to survive.

What the edge owes the core: a mark for the navigations werust's own chrome starts (the URL bar commit path, and a submission from the settings page's own form, which is a page-initiated GET the chrome must recognise as the user acting inside a surface werust drew), and NO mark for anything else: in-page navigations, sub-resource requests, and the `WKUIDelegate` new-window hook that loads a `_blank`/`window.open` target into the main view.

WebKit's navigation policy callback carries the facts this needs (what kind of navigation it is, which frame originated it, whether the target frame is the main frame). Use them as INPUTS to the signal the edge sends the core, never as a decision the edge makes and never as something the core infers from the request.

## Acceptance criteria

- [ ] A change submitted from the settings page's own form, and a change typed into the URL bar, both apply and persist on iOS exactly as before.
- [ ] A page-initiated navigation to a mutating `werust://settings?...` URL, and a sub-resource request for one, do NOT change the persisted settings; the page still renders read-only with real current values.
- [ ] The `WKUIDelegate` in-place new-window path does NOT mark intent, so a `window.open('werust://settings?backend=...')` cannot mutate; covered by an assertion.
- [ ] The Swift layer supplies a signal and decides nothing: the mutation decision stays in the shared core, pinned by an APPENDED block in the new per-edge source-shape guard under `crates/werust-core/tests/` that the core task created (append your edge's block; never rewrite a sibling's).
- [ ] The Rust-side half (the iOS FFI and whatever carries the signal into the core) is covered by unit tests that run on the pure Linux gate.
- [ ] Behavioural evidence is NAMED and not overclaimed: `.github/workflows/mobile-ios.yml` BUILDS and launches but asserts no behaviour, so the done record states that the iOS evidence is the gate-compiled Rust half, the source-shape guard and that build leg, plus a recorded manual check in the Simulator if one was done, following the manual-verification-steps convention of `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md`.
- [ ] The `ios` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented`, stating that honest limit. Edit ONLY that cell's line: three sibling edge tasks are flipping their own cells in the same row, so if one landed first, rebase and re-apply only your line.
- [ ] Tests isolate the settings location to a scratch directory and assert the real one is UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`: it defines the core gate, the intent carrier and the shape-guard file this task appends to, and it creates the matrix row whose cell you flip.

## Prompt

> Goal: supply the iOS edge's user-intent signal for `werust://settings` mutations, so a page cannot repoint werust's IPFS retrieval backend on iOS. The core rule (main-frame AND chrome-marked intent, refused mutation still renders the page read-only) already landed with the GTK edge; read that task's done record and `work/specs/tasked/settings-mutations-require-user-intent.md` first.
>
> Where to look: the Swift shell is `crates/werust-ios/App/Sources/WKWebViewShellController.swift` (its navigation delegate, its `WKUIDelegate` new-window handling, the URL bar commit path, and the `WerustSchemeHandler` registered for the `werust` scheme), and the Rust edge is `crates/werust-ios/rust/src/lib.rs` (the FFI surface including the settings-apply entry point that routes into the shared core).
>
> Mark the navigations werust's chrome starts: the URL bar commit, and the settings page's own form submission (a page-initiated GET that the chrome must recognise as the user acting inside a werust-drawn surface). Leave UNMARKED everything a hostile page controls: in-page navigations, sub-resource requests, and the new-window hook that loads a `_blank`/`window.open` target into the main view (a router by design, which must not become a trust bypass). WebKit's navigation policy callback carries the navigation kind, the source frame and the target frame: use them as INPUTS to the signal, never as the decision.
>
> Keep the Swift layer a signal source and let the core decide, the same discipline this repo enforces for the chrome derivation (the mobile edges read `werust_core::chrome_json` rather than re-deriving; a guard reds the gate if a twin returns). Append your edge's block to the per-edge source-shape guard the core task created under `crates/werust-core/tests/`; do not rewrite a sibling edge's block.
>
> Be precise about evidence: `mobile-ios.yml` builds the app and launches it on a Simulator but asserts NO behaviour, so it cannot prove the refusal. Say exactly that in the done record and name what you did prove (the Linux-gate Rust tests, the shape guard, the build leg, and any manual Simulator check with its steps recorded). Do not write a criterion the gate or that leg cannot check.
>
> Then flip ONLY the `ios` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml`, stating the honest limit in the row's comment the way the `retrieval-backend` row already does for iOS. Three sibling edge tasks (Android, macOS, Windows) are flipping their own cells in that row and appending to the same guard file: those are your only shared surfaces, so rebase and re-apply just yours if a sibling lands first.
