---
title: "macOS supplies the intent signal for settings mutations, and refuses one a page started"
slug: macos-marks-user-intent-for-settings-mutations
spec: settings-mutations-require-user-intent
blockedBy: [settings-mutation-requires-marked-user-intent-in-core-and-on-gtk]
covers: [6, 7]
---

## What to build

The core now applies a `werust://settings` mutation only for a MAIN-FRAME request whose navigation the chrome MARKED as intended, and the GTK edge supplies that mark. Do the same for macOS.

The macOS backend registers the `werust` scheme through its own `WKURLSchemeHandler` beside `ipfs`, before the first navigation (the configuration is copied when the web view is constructed, so a late registration is refused and reported). That handler fires for the main document AND every sub-resource.

What the edge owes the core: a mark for the navigations werust's own chrome starts (the AppKit URL bar's commit, and a submission from the settings page's own form, which is a page-initiated GET the chrome must recognise as the user acting inside a surface werust drew), and NO mark for anything else: in-page navigations, sub-resource requests, and the `WKUIDelegate` new-window path that loads a `_blank`/`window.open` target into the existing view.

macOS has an advantage the other native edges do not: `.github/workflows/macos-renderer.yml` already RUNS an off-screen window smoke on a `macos-14` runner. The parity matrix currently records an HONEST LIMIT that nothing has ever LOADED `werust://settings` on a Mac. This task can retire that limit for the mutation path by driving it in the smoke, and it should, because a driven refusal is worth far more than a compiled one.

## Acceptance criteria

- [ ] A change submitted from the settings page's own form, and a change typed into the AppKit URL bar, both apply and persist on macOS exactly as before.
- [ ] A page-initiated navigation to a mutating `werust://settings?...` URL, and a sub-resource request for one, do NOT change the persisted settings; the page still renders read-only with real current values.
- [ ] The new-window in-place path does NOT mark intent, so a `window.open('werust://settings?backend=...')` cannot mutate; covered by an assertion.
- [ ] The AppKit/WKWebView layer supplies a signal and decides nothing: the mutation decision stays in the shared core, pinned by an APPENDED block in the new per-edge source-shape guard under `crates/werust-core/tests/` that the core task created (append your edge's block; never rewrite a sibling's), and by the existing macOS window shape guard where it applies.
- [ ] The host-independent half is covered by tests that run on the pure Linux gate (the macOS crates are workspace members precisely so that half is gate-compiled and tested).
- [ ] The refusal is DRIVEN on `.github/workflows/macos-renderer.yml`: the off-screen smoke performs a page-initiated mutation attempt against a scratch settings directory and asserts the persisted settings are unchanged, and performs an intended one and asserts it applies. Name the run in the done record.
- [ ] The `macos` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented`, naming the leg that drove it. Edit ONLY that cell's line: three sibling edge tasks are flipping their own cells in the same row, so if one landed first, rebase and re-apply only your line.
- [ ] Tests and the smoke isolate the settings location to a scratch directory and assert the real one is UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`: it defines the core gate, the intent carrier and the shape-guard file this task appends to, and it creates the matrix row whose cell you flip.

## Prompt

> Goal: supply the macOS edge's user-intent signal for `werust://settings` mutations, and prove the refusal on the macOS CI leg. The core rule (main-frame AND chrome-marked intent, refused mutation still renders the page read-only) already landed with the GTK edge; read that task's done record and `work/specs/tasked/settings-mutations-require-user-intent.md` first.
>
> Where to look: `crates/macos-renderer/src/backend.rs` (the navigation bridge that already conforms to the navigation and UI delegates, the `WKURLSchemeHandler` registrations for `ipfs` and `werust`, and the registration-ordering rule), `crates/werust-macos/` (the AppKit window, its URL bar commit path, its shape guard and its off-screen `window_smoke` example), and `crates/desktop-paint` for the shared painted values. The WKWebView halves are `#[cfg(target_os = "macos")]`, so the Linux gate compiles and tests only the host-independent half: `.github/workflows/macos-renderer.yml` is what compiles and RUNS the rest.
>
> Mark the navigations werust's chrome starts: the URL bar commit, and the settings page's own form submission (a page-initiated GET the chrome must recognise as the user acting inside a werust-drawn surface). Leave UNMARKED everything a hostile page controls: in-page navigations, sub-resource requests, and the new-window hook that loads a `_blank`/`window.open` target into the existing view. WebKit's navigation policy callback carries the navigation kind, the source frame and the target frame: use them as INPUTS to the signal the edge sends the core, never as the decision, and never as something the core infers from the request.
>
> Keep the edge a signal source: the label/visibility/derivation discipline this repo enforces (`crates/werust-macos/tests/macos_window_shape.rs` reds if the AppKit layer starts deciding) applies here too. Append your edge's block to the per-edge source-shape guard the core task created under `crates/werust-core/tests/`.
>
> The measurement matters: the parity matrix records that nothing has ever LOADED `werust://settings` on a Mac, and this leg already runs an off-screen window smoke. Drive both cases there (an unmarked page-initiated attempt leaves the persisted settings unchanged; an intended one applies), against a SCRATCH settings directory so the runner's real settings file is untouched, and name the run in the done record. Per `CONTEXT.md`, the leg is already on `main`, so you can dispatch it yourself; if a criterion still ends up unmeasured, say so plainly rather than implying it passed.
>
> Then flip ONLY the `macos` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml`. Three sibling edge tasks (Android, iOS, Windows) are flipping their own cells in that row and appending to the same guard file: those are your only shared surfaces, so rebase and re-apply just yours if a sibling lands first.
