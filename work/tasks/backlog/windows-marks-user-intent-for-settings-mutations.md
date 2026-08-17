---
title: "Windows supplies the intent signal for settings mutations, and refuses one a page started"
slug: windows-marks-user-intent-for-settings-mutations
spec: settings-mutations-require-user-intent
blockedBy: [settings-mutation-requires-marked-user-intent-in-core-and-on-gtk]
covers: [6, 7]
---

## What to build

The core now applies a `werust://settings` mutation only for a MAIN-FRAME request whose navigation the chrome MARKED as intended, and the GTK edge supplies that mark. Do the same for Windows.

The WebView2 backend registers the `werust` scheme beside `ipfs` before the first navigation (WebView2 fixes the set of custom scheme names at environment creation and makes it immutable for the browser-process lifetime, which is why the environment is created lazily and a late registration is refused loudly). That handler fires for the main document AND every sub-resource.

What the edge owes the core: a mark for the navigations werust's own chrome starts (the Win32 URL bar's commit, which is subclassed precisely because an `EDIT` swallows Enter, and a submission from the settings page's own form), and NO mark for anything else: in-page navigations, sub-resource requests, and the new-window-requested path that loads a `_blank`/`window.open` target into the existing view.

WebView2's navigation-starting event carries a per-navigation "was this user initiated" fact and whether it is a redirect; treat those as INPUTS to the signal the edge sends the core, never as the decision. Note the trap the origin probe already documented for this edge: WebView2's own notions are per-navigation and do not distinguish "the user clicked something in the PAGE" from "werust's chrome asked for this", so a bare user-initiated flag is NOT the intent signal on its own.

Like macOS, this edge has a leg that RUNS: `.github/workflows/windows-renderer.yml` drives an off-screen window smoke on a `windows-latest` runner, and the parity matrix records the honest limit that nothing has ever LOADED `werust://settings` on Windows. Retire that limit for the mutation path by driving it there.

## Acceptance criteria

- [ ] A change submitted from the settings page's own form, and a change typed into the Win32 URL bar, both apply and persist on Windows exactly as before.
- [ ] A page-initiated navigation to a mutating `werust://settings?...` URL, and a sub-resource request for one, do NOT change the persisted settings; the page still renders read-only with real current values.
- [ ] The new-window-requested in-place path does NOT mark intent, so a `window.open('werust://settings?backend=...')` cannot mutate; covered by an assertion.
- [ ] A bare WebView2 "user initiated" flag is NOT treated as intent on its own, and the reason is stated where a future reader will see it.
- [ ] The Win32/WebView2 layer supplies a signal and decides nothing: the mutation decision stays in the shared core, pinned by an APPENDED block in the new per-edge source-shape guard under `crates/werust-core/tests/` that the core task created (append your edge's block; never rewrite a sibling's), plus the existing windows window/backend shape guards where they apply.
- [ ] The host-independent half is covered by tests that run on the pure Linux gate (the Windows crates are workspace members precisely so that half is gate-compiled and tested).
- [ ] The refusal is DRIVEN on `.github/workflows/windows-renderer.yml`: the off-screen smoke performs a page-initiated mutation attempt against a scratch settings directory and asserts the persisted settings are unchanged, and performs an intended one and asserts it applies. Name the run in the done record.
- [ ] The `windows` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml` flips from `stubbed` to `implemented`, naming the leg that drove it. Edit ONLY that cell's line: three sibling edge tasks are flipping their own cells in the same row, so if one landed first, rebase and re-apply only your line.
- [ ] Tests and the smoke isolate the settings location to a scratch directory and assert the real one is UNTOUCHED.
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Blocked by

- `settings-mutation-requires-marked-user-intent-in-core-and-on-gtk`: it defines the core gate, the intent carrier and the shape-guard file this task appends to, and it creates the matrix row whose cell you flip.

## Prompt

> Goal: supply the Windows edge's user-intent signal for `werust://settings` mutations, and prove the refusal on the Windows CI leg. The core rule (main-frame AND chrome-marked intent, refused mutation still renders the page read-only) already landed with the GTK edge; read that task's done record and `work/specs/tasked/settings-mutations-require-user-intent.md` first.
>
> Where to look: `crates/windows-renderer/src/backend.rs` (the WebView2 COM wiring: navigation-starting, source-changed, new-window-requested, and the `ipfs`/`werust` scheme registrations plus the environment-creation ordering rule), `crates/windows-renderer/src/pure.rs` (the decisions the COM wiring makes, gate-tested), `crates/werust-windows/` (the Win32 window, the subclassed URL bar `EDIT`, its shape guard and its off-screen `window_smoke` example), and `crates/desktop-paint` for the shared painted values. The WebView2 halves are `#[cfg(windows)]`, so the Linux gate compiles and tests only the host-independent half: `.github/workflows/windows-renderer.yml` compiles and RUNS the rest.
>
> Mark the navigations werust's chrome starts: the URL bar commit, and the settings page's own form submission (a page-initiated GET the chrome must recognise as the user acting inside a werust-drawn surface). Leave UNMARKED everything a hostile page controls: in-page navigations, sub-resource requests, and the new-window-requested hook that loads a `_blank`/`window.open` target into the existing view. WebView2 exposes a per-navigation user-initiated fact, but it does not distinguish a click in the PAGE from werust's own chrome asking, so it is an INPUT, not the signal: say that where a future reader will find it.
>
> Keep the edge a signal source: `crates/werust-windows/tests/windows_window_shape.rs` exists to red the gate if the Win32 layer starts deciding, and `crates/werust-core/tests/windows_renderer_leg_shape.rs` pins the leg's shape (do not disturb what it asserts). Append your edge's block to the per-edge source-shape guard the core task created under `crates/werust-core/tests/`.
>
> The measurement matters: the parity matrix records that nothing has ever LOADED `werust://settings` on Windows, and this leg already runs an off-screen smoke. Drive both cases there (an unmarked page-initiated attempt leaves the persisted settings unchanged; an intended one applies), against a SCRATCH settings directory so the runner's real settings file is untouched, and name the run in the done record. The leg is already on `main`, so you can dispatch it yourself (`CONTEXT.md`); if a criterion ends up unmeasured, say so plainly rather than implying it passed.
>
> Then flip ONLY the `windows` cell of the `settings-mutation-user-intent` row in `docs/platform-capability-matrix.toml`. Three sibling edge tasks (Android, iOS, macOS) are flipping their own cells in that row and appending to the same guard file: those are your only shared surfaces, so rebase and re-apply just yours if a sibling lands first.
