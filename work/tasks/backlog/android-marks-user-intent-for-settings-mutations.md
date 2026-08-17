---
title: "Android supplies the intent signal for settings mutations, and refuses one a page started"
slug: android-marks-user-intent-for-settings-mutations
spec: settings-mutations-require-user-intent
blockedBy: [settings-mutation-requires-marked-user-intent-in-core-and-on-gtk]
covers: [6, 7]
---

## What to build

The core now applies a `werust://settings` mutation only for a MAIN-FRAME request whose navigation the chrome MARKED as intended, and the GTK edge supplies that mark. Do the same for Android, so the weakest edge does not define werust's safety.

Android is the edge where "the engine surely does the right thing" has already been proven wrong twice (DOM storage defaulted off; a trust decision not persisting), and it is werust's most divergent edge: it does not serve `ipfs://` as the document origin, mapping loads to an internal `https://<cid>.ipfs.werust.invalid` host instead, and its scheme dispatch is a generic intercept that hands ANY registered scheme (including `werust://`) to the core.

What the edge owes the core: a mark for the navigations werust's own chrome starts (the URL bar commit path, and a submission from the settings page's own form, which is a page-initiated GET the chrome must recognise as the user acting inside a surface werust drew), and NO mark for anything else. The two paths that must stay unmarked are the ones a hostile page controls: an in-page navigation (a link, `location=`, a `<img>`/`fetch` sub-resource) and the multiple-windows hook that routes a `_blank`/`window.open` target into the same WebView.

Do not re-decide the rule at the edge. The decision (does this request get to mutate?) is core logic; Android supplies the signal only. Keep the Kotlin layer a signal source, exactly as the repo keeps it a painter for the chrome derivation.

## Acceptance criteria

- [ ] A change submitted from the settings page's own form, and a change typed into the URL bar, both apply and persist on Android exactly as before.
- [ ] A page-initiated navigation to a mutating `werust://settings?...` URL, and a sub-resource request for one, do NOT change the persisted settings; the page still renders read-only with real current values.
- [ ] The `_blank`/`window.open` in-place routing does NOT mark intent, so a `window.open('werust://settings?backend=...')` cannot mutate; covered by an assertion.
- [ ] The Kotlin layer supplies a signal and decides nothing: the mutation decision stays in the shared core, pinned by an APPENDED block in the new per-edge source-shape guard under `crates/werust-core/tests/` that the core task created (append your edge's block; never rewrite a sibling's).
- [ ] The Rust-side half (the Android FFI and whatever carries the signal into the core) is covered by unit tests that run on the pure Linux gate.
- [ ] Behavioural evidence is NAMED, not implied: if `.github/workflows/android-instrumented.yml` exists by the time this is built (task `android-instrumented-emulator-ci-leg`), add an instrumented case asserting the refusal and name that leg in the done record; if it does not, the done record states that the evidence is the gate-compiled Rust half plus the source-shape guard plus a recorded hand-run on a device or emulator, and says which.
- [ ] The `android` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented`, naming what proves it. Edit ONLY that cell's line: three sibling edge tasks are flipping their own cells in the same row, so if one landed first, rebase and re-apply only your line.
- [ ] Tests isolate the settings location to a scratch directory and assert the real one is UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`: it defines the core gate, the intent carrier and the shape-guard file this task appends to, and it creates the matrix row whose cell you flip.

## Prompt

> Goal: supply the Android edge's user-intent signal for `werust://settings` mutations, so a page cannot repoint werust's IPFS retrieval backend on Android. The core rule (main-frame AND chrome-marked intent, refused mutation still renders the page read-only) already landed with the GTK edge; read that task's done record and the spec `work/specs/tasked/settings-mutations-require-user-intent.md` first.
>
> Where to look: the Kotlin shell is `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` (its WebViewClient, its WebChromeClient's multiple-windows hook, and the URL bar commit path), and the Rust edge is `crates/werust-android/rust/src/` (the FFI surface, the generic scheme dispatch that hands `werust://` to the shared core, and the origin map). Android's scheme intercept fires for the main document AND every sub-resource, which is exactly why the core gate exists.
>
> Mark the navigations werust's chrome starts: the URL bar commit, and the settings page's own form submission (a page-initiated GET that the chrome must recognise as the user acting inside a werust-drawn surface, not as web content acting). Leave UNMARKED everything a hostile page controls: in-page navigations, sub-resource requests, and the multiple-windows hook that routes a `_blank`/`window.open` target into the same WebView (that hook is a router by design, and it must not become a trust bypass). Android's request objects expose per-request facts (is this the main frame, was there a user gesture) and its navigation callbacks expose the initiating context: use them as INPUTS to the signal, never as the decision, and never as something the core infers from the request.
>
> Keep the Kotlin layer a signal source and let the core decide, the same discipline this repo enforces for the chrome derivation (the mobile edges read `werust_core::chrome_json` rather than re-deriving). Append your edge's block to the per-edge source-shape guard the core task created under `crates/werust-core/tests/`; do not rewrite a sibling edge's block.
>
> Evidence: there is no Android CI leg unless `android-instrumented-emulator-ci-leg` has landed. If it has, add an instrumented case asserting that a page-initiated mutation is refused and name `.github/workflows/android-instrumented.yml` in the done record. If it has not, do NOT claim CI evidence: say in the done record that the evidence is the Linux-gate Rust tests plus the shape guard plus a recorded hand-run (`cd crates/werust-android && ./gradlew :app:connectedDebugAndroidTest`, the way the existing `WebStorageTest`/`SpaClientNavOriginTest` probes are run), and record the device/emulator and System WebView version if you ran one, the way `docs/spikes/android-enable-dom-storage-and-guard-web-platform-parity/MEASUREMENTS.md` does.
>
> Then flip ONLY the `android` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml`, naming what proves it. Three sibling edge tasks (iOS, macOS, Windows) are flipping their own cells in that same row and appending to the same guard file: those two lines are your only shared surfaces, so rebase and re-apply just yours if a sibling lands first.
