# Windows: the Win32 window that paints the chrome — what landed, and what is proven by what

Task: `windows-win32-window-and-chrome`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), sub-task 3 of its Windows breakdown, funded by [Amendment 1](../../adr/0011-webview2-for-windows.md#amendment-1-2026-07-30--the-defer-is-overturned-windows-and-macos-are-funded-now). Engine it sits on: [`windows-webview2-renderer-backend`](../windows-webview2-renderer-backend/README.md). Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md).

**Read this first.** This work was WRITTEN on Linux, blind, and then RUN on a `windows-latest` runner: the leg this task extends is green against this code (see [What CI proved](#what-ci-proved)). Everything below is split by what proves it, because the three sources prove very different things: an ordinary Ubuntu `verify` run, the LOCAL cross-target type-check, and the `windows-latest` job. Where a claim is a Windows runtime result it names the run; where it is host-independent it says so; where nothing has checked it, it lives under [What still awaits real Windows hardware](#what-still-awaits-real-windows-hardware-stated-plainly).

## What landed

- **`crates/werust-windows`** — the Win32 window, in two halves:
  - `src/profile.rs`, host-independent: the DURABLE WebView2 user-data folder rule (`%LOCALAPPDATA%\werust\WebView2`), unit-tested on Ubuntu.
  - `src/window.rs` + `src/chrome.rs` + `src/debugview.rs` + `src/win32.rs`, the **Win32 half** (`#[cfg(windows)]`): a top-level window with the toolbar (URL bar + progress, back/forward/reload/stop, the invalid-entry badge, the trust indicator and its EXPLANATION on a real tooltip, the ⋮ menu), the error banner, the status line, the re-parented WebView2 page, and the tabbed Console + Network debug view. It assigns fields of a `ChromePaint` to controls and forwards actions to the shared `BrowserShell`. It contains no rule.
- **`crates/desktop-paint`** — the shared painter half, EXTRACTED verbatim (with its tests) out of `werust-macos::paint` so this window consumes ONE carrier rather than copying it, and so the palette exists once instead of three times. Both windows re-export it as `paint`; nothing about the macOS crate's surface changed. See [`DECISIONS.md`](DECISIONS.md) §2.
- **`crates/desktop-paint/tests/gtk_stylesheet_agreement.rs`** — new: every colour the GTK `APP_CSS` declares must be the SAME colour the shared palette holds. "The same green on both desktops" stops being a promise kept by transcription.
- **Two engine changes** in `crates/windows-renderer`, both forced by hosting the engine in a real window: the container's own window proc now keeps the WebView2 controller's bounds in step with the container (`WM_SIZE`), and a small `DevTools` handle lets the shell open Edge's own `OpenDevToolsWindow` after the backend is behind the seam — with `AreDevToolsEnabled` now gated on a debug build, as every other platform's `web-inspector` row already is. [`DECISIONS.md`](DECISIONS.md) §3.
- **`crates/werust-windows/examples/window_smoke.rs`** — the window's only execution anywhere: a real window opened FAR off-screen, a pinned in-memory hash-verified `ipfs://` page loaded through the PRODUCTION verifying route, assertions on what the real WIDGETS hold, and a negative control whose bytes do not hash to their CID.
- **`.github/workflows/windows-renderer.yml`** — extended: it now builds and tests `werust-windows` + `desktop-paint` and RUNS `window_smoke`, with the path filters (`push` and `pull_request`) to match.
- **The local type-check harness** ([`typecheck-windows-from-linux.sh`](../windows-webview2-renderer-backend/typecheck-windows-from-linux.sh)) — extended to cover the window crate and its smoke, which is how the Win32 code below was iterated on from Linux at all.

Not in scope, deliberately: no installer, no code signing, no zip on a Release (task `windows-release-packaging-leg`), and no `windows` column in the platform-capability matrix (task `windows-parity-column-and-stub-tasks`, which runs after this so the cells describe what really shipped). Both were authored by this task.

## It PAINTS: where each surface's rule actually lives

| surface | the rule (shared, `werust-core`) | what Win32 does |
|---|---|---|
| URL bar text | `ChromeState::url_text` | `SetWindowTextW`, only when it changed (no caret jump) |
| invalid entry | `invalid_entry_badge_visible` / `_text` | shows the badge, colours the field's text in `WM_CTLCOLOREDIT` |
| back / forward | `ChromeState::can_go_back` / `_forward` | `EnableWindow` |
| stop vs reload | `ChromeState::is_loading` | `EnableWindow` |
| status line | `status_line` | `SetWindowTextW` |
| trust indicator | `trust_indicator`, `trust_indicator_detail`, `trust_indicator_css_class` | a `STATIC` + a real tooltip + the class's colour in `WM_CTLCOLORSTATIC` |
| error banner | `error_banner_visible` / `_text` / `_css_class` | shown only on failure, filled in the severity's colour |
| load progress | `load_progress_visible` / `_fraction` / `_tooltip` | a `msctls_progress32` INSIDE the URL bar + its tooltip |
| ⋮ menu | `BrowserMenu` | `HMENU` items, dispatched by stable id |
| debug rows | `console_row_text`, `network_*`, `tail_plan` | `SysListView32` rows, coloured by `NM_CUSTOMDRAW` |

The CSS-class NAMES come from the core's exported sets; the palette that gives each name a colour is `desktop-paint`'s, shared with the AppKit window and asserted equal to the GTK stylesheet's. A core class with no colour there reds the Ubuntu gate (`every_exported_class_has_a_colour`).

**Nothing had to be extracted INTO the core by this task**, and that is the point rather than an omission: `desktop-chrome-presentation-into-core` moved the chrome rules there, `macos-appkit-window-and-chrome` moved the debug-view row rules and `tail_plan`, and `one-derivation-close-the-aggregate-and-tooltip-gaps` closed the family-aggregate and tooltip-sentence gaps. A third native window found every rule it needed already in one place — which is what those three tasks were for. What this task DID extract is one layer out: the edge-side CARRIER (`desktop-paint`), so the Windows window consumes it rather than copying it. The macOS window consumes the same one; the GTK edge keeps its stylesheet and is held to the same colours by a test.

## What the Ubuntu `verify` gate proves TODAY (every ordinary run)

1. **The window paints the core's derivation, field for field** — `crates/desktop-paint`'s `the_paint_is_the_cores_derivation_verbatim` drives seven chrome states and asserts every painted field equals the core function that decides it. The Windows window paints that same carrier, so this covers it exactly as it covers macOS.
2. **The loading rules are followed**: a load in flight paints the neutral loading badge (no trust claim), shows progress, and raises NO banner; only a failure raises one.
3. **Every exported state class has a colour**, driven by the core's `CssClassFamily::ALL` aggregate — and a class the core does NOT export has none, so the guard is not vacuous.
4. **The shared palette and the GTK stylesheet agree**, colour by colour (new here).
5. **The ⋮ menu is the core's `BrowserMenu`**, version line disabled, Debug entry activatable, in order.
6. **The debug rows are the core's row derivation**, including the per-request trust column, and the incremental refresh survives ring-buffer eviction AT the cap.
7. **The durable profile rule is right**: `%LOCALAPPDATA%\werust\WebView2`, never `%TEMP%`, and an unreadable variable declines rather than inventing a path.
8. **The Win32 source has the shape it must have** — `crates/werust-windows/tests/windows_window_shape.rs` parses the four files the gate cannot compile: every surface present, no chrome rule called from Win32-land, no class name or label restated, no second palette, no second OS-colour-scheme reader, the new-window rule left to the engine (ADR-0010), the page geometry depending on the banner and never on progress, the durable profile really passed, and the CI leg really building/testing/running this crate.

## What the LOCAL type-check proves (and what it does not)

[`typecheck-windows-from-linux.sh`](../windows-webview2-renderer-backend/typecheck-windows-from-linux.sh) type-checks the engine, the window and both smokes against `x86_64-pc-windows-msvc` from Linux, via `cargo-xwin`. Run 2026-07-30 on this code: **clean — `cargo xwin clippy -p werust-windows -p windows-renderer --tests --examples` with no errors and no warnings.**

**That clippy line was run BESIDE the harness, not BY it.** As committed by this task the harness ended in `cargo xwin check`, which is weaker than the command recorded above, so this paragraph credited the tool with a check it did not perform. Task `windows-backend-error-mapping-and-leg-header-accuracy` closed the gap in the tool rather than in the prose ([why](../windows-backend-error-mapping-and-leg-header-accuracy/DECISIONS.md)): the harness now runs exactly that `cargo xwin clippy … --tests --examples`, re-run clean (no errors, no warnings from either Windows crate) on 2026-07-31 against the tree that landed it.

That means every `CreateWindowExW`, every `SendMessageW`, every struct layout and every seam signature in the Win32 half type-checks against the real Windows SDK bindings. It is **not a build and not a test**: nothing links the WebView2 loader, nothing runs, no window is created and no message loop turns. It proves the shape of the Win32 wiring, not that anything WORKS — that is what the CI leg below is for, and this is simply the fast inner loop that keeps CI from being the first place a typo is found.

## What CI proved

**The `windows-renderer` leg is GREEN against the tree that LANDS**, not against an ancestor of it. Run **[30589670305](https://github.com/wighawag/werust/actions/runs/30589670305)** (`workflow_dispatch`, ref `ci/windows-win32-window-and-chrome`, commit **`b312f31`**, 2026-07-30T23:10Z, every step succeeded). `b312f31`'s SOURCE tree is what this task ships: the only edits after it are to this README and to the captured output file below, neither of which the leg compiles. It measured on **Microsoft Windows Server 2025, 10.0.26100**, runner image `windows-2025-vs2026`, **WebView2 Runtime 150.0.4078.65** — the same runtime build the origin probe and the engine's own smoke measured on. The window smoke's verbatim output, from THAT run: [`window-smoke-2026-07-30.txt`](window-smoke-2026-07-30.txt).

What each step actually established, and by what kind of evidence:

1. **The Win32 window COMPILES and LINKS against a real Windows SDK.** PASSED. `cargo build -p windows-renderer -p werust-windows -p desktop-paint …` on `x86_64-pc-windows-msvc`. The Ubuntu gate can only parse this code.
2. **The crate's tests pass on Windows.** PASSED. `cargo test -p werust-windows …`: the 3 durable-profile unit tests + the 11 source-shape assertions, alongside `desktop-paint`'s 7 paint tests and its 2 new GTK-palette-agreement tests, the 276 `werust-core`, 36 `fetcher`, 20 `renderer`, 5 `webview-shared`, and the engine's own. The SHARED painter half is therefore exercised on the third platform, not only on the gate.
3. **The window is CONSTRUCTED and DRIVEN, and every widget holds the core's derivation.** PASSED — 26 of 26 checks. A real top-level window, off-screen, with a real `EDIT`, `STATIC`s, an `HMENU`, a tooltip, a `SysTabControl32` and two `SysListView32`s. Read back off the REAL widgets: the trust badge text, **the trust EXPLANATION off the real tooltip**, the status line, the URL bar's text, the ⋮ menu's titles (`["werust b312f31", "Debug"]` — the core's `BrowserMenu`, version line first), and the debug view's row counts.
4. **A hash-verified `ipfs://` page loads through the PRODUCTION verifying route and reads as verified in the WINDOW.** PASSED, offline (pinned in-memory retriever, no gateway, no network).
5. **The URL-bar-progress rule holds on a real load.** PASSED. Across a whole navigation `GetWindowRect` on the page window is byte-identical before and after: in-flight progress did NOT displace the page. The progress strip is gone once the load settles.
6. **The DURABLE profile is real.** PASSED. `C:\Users\runneradmin\AppData\Local\werust\WebView2` exists on disk after the run and is not under `%TEMP%` — the criterion planted at Gate 3 of the engine task, MEASURED rather than asserted.
7. **The debug view catches the page's own `console.log`.** PASSED, through the SHARED shim on the dedicated capture channel into the shared store and out as a real list-view row; clearing the shared store empties both tabs on the next tick.
8. **Devtools are the platform's own.** PASSED. `OpenDevToolsWindow` opened Edge's real DevTools window over the live page (in a debug build; a release build no longer enables them).
9. **The NEGATIVE CONTROL fails, and the failure takes the page.** PASSED. Bytes that do not hash to their CID: the load ended `Failed`, the banner appeared carrying werust's own protocol-named reason (`⚠ This page failed to load: renderer backend error: ipfs:// content-addressed load failed: block hash mismatch: bytes do not match cid bafkreiaqx3di4a2g7xqwz56yfutcesleck7eb2rk6xbnifoxfdusadhpby`), the page area SHRANK, and the trust indicator never said verified.
10. **Closing the debug window drops the slot**, so Debug opens a fresh one. PASSED.

**Why the earlier green run is not the one quoted.** Run [30588844677](https://github.com/wighawag/werust/actions/runs/30588844677) (commit `d7f8d97`) was green first, but `debugview.rs` then lost a dead type alias and its import, and the leg's own header comment grew. That is a trivial delta, and it is still exactly the "it was green one commit ago" shape this repo has already bounced twice, so the leg was dispatched AGAIN on the final tree and that later run is the one quoted above. Its window-smoke output differs from `d7f8d97`'s in a single token: the commit sha inside the version line the ⋮ menu shows.

**The extraction was proved on the OTHER platform too, not assumed.** Moving `werust-macos::paint` into `crates/desktop-paint` changes the macOS window's dependency, and the Ubuntu gate cannot compile AppKit. So the macOS leg was dispatched against this code as well: run **[30589324863](https://github.com/wighawag/werust/actions/runs/30589324863)** (`workflow_dispatch`, ref `ci/windows-win32-window-and-chrome`, commit `3ffbe2b`, macOS **14.8.7**, Xcode **15.4**), GREEN in every step — including `cargo run -p werust-macos --example window_smoke`, which CONSTRUCTS the real `NSWindow` and asserts what its widgets hold, and which now paints from the shared crate. `window_smoke: PASS`. "Behaviour-preserving" is therefore a measurement on both windows, not a claim about one. That run is on `3ffbe2b` rather than `b312f31`, and the difference between them is stated above: a deletion inside `crates/werust-windows/src/debugview.rs`, a `#[cfg(windows)]` module the macOS leg does not build, plus workflow comments and this README. Nothing macOS compiles changed between the two.

**The smoke earned its keep before it was green.** The first dispatch, run [30588443862](https://github.com/wighawag/werust/actions/runs/30588443862) on commit `3bae22a`, FAILED — 24 of 26, with exactly the two checks that read the trust EXPLANATION back off the tooltip. The cause was real and would have shipped invisibly: `TTM_ADDTOOL` was passed `size_of::<TTTOOLINFOW>()`, which is the comctl32 **version 6** structure size, and werust links 5.82 (no application manifest yet), so the control rejected the `cbSize` and added no tool at all — a trust badge with no explanation, on the exact surface that already shipped desktop-only for months. The fix is `TOOL_INFO_V2_SIZE` in `crates/werust-windows/src/chrome.rs`, which both versions accept. A smoke that reads a snapshot it already trusts would have passed both times.

## What still awaits real Windows hardware (stated plainly)

ADR-0011 Amendment 1 requires this split to be stated, not blurred. This work was written blind from Linux and measured on a CI runner; a runner is not a Windows box in front of a human.

- **Nothing here has run on Windows HARDWARE, only on a `windows-latest` CI runner.** It is a real Windows with a real evergreen WebView2 Runtime, and it is what settles "the widgets hold what the core derived"; it is not a desktop with a display, a GPU, a mouse or a user.
- **Nothing about how it LOOKS is verified.** No screenshot is taken and none could be judged automatically: the toolbar's proportions, whether a 210px trust badge is wide enough for the longest posture phrase at the user's font scale, whether the error banner reads as urgent, whether the debug list's columns are usable. A human has to look.
- **The chrome is CLASSIC-styled.** Without a comctl32 v6 manifest (deliberately deferred to packaging, [`DECISIONS.md`](DECISIONS.md) §4) the controls draw in the pre-Vista style, and system-drawn push BUTTONs do not follow dark mode. A dark-mode window therefore has dark chrome surfaces and light buttons. Nothing measured this; it follows from the manifest's absence.
- **HiDPI is UNVERIFIED and the window is not DPI-aware.** Every rectangle in `chrome.rs` is in raw pixels and there is no per-monitor-v2 manifest entry, so on a 150%/200% display Windows will bitmap-scale the chrome (blurry, but laid out correctly) and a multi-monitor drag between different scale factors is untested. This is the same manifest that carries visual styles, so it lands with packaging.
- **Input, focus and keyboard routing are unmeasured.** The smoke never clicks, types, scrolls or tabs. Enter-in-the-URL-bar goes through a real `SetWindowSubclass` and the F12 devtools key through the same proc, but the run drives navigation through the shell API, not through a keystroke. Whether focus moves sensibly between the chrome and the page — and whether the page receives real Win32 input at all — is untested here and was already listed as unproven by the engine task.
- **Window management is unmeasured**: resizing (the new `WM_SIZE` → `SetBounds` engine path is exercised only by the initial layout), maximise/restore, minimise, multi-monitor, and closing the window while a load is in flight.
- **The `WM_SETTINGCHANGE` re-read never fires on CI.** The runner's colour scheme does not change mid-run, so "the chrome follows the OS when the user flips dark mode" is asserted structurally (the reader, the handler, the shared rule) and not observed.
- **The debug view's COLOURS are unverified.** `NM_CUSTOMDRAW` is exercised only in so far as the rows render at all; that an error row is red and a verified trust cell green is a claim about pixels.
- **Network capture is SHIM-ONLY, and says so.** The Network tab sees only what the page requests through `fetch`/`XHR` (the shared shim), NOT browser-internal subresource loads (`<img>`, `<script>`, CSS `url()`) and not the main document. WebView2 could do better through `AddWebResourceRequestedFilter("*")` or the DevTools protocol; neither is wired, and both stay named follow-ons rather than silent omissions — the same honest gap the macOS window records.
- **The WebView2 Runtime is EVERGREEN and cannot be pinned.** A green run dates a measurement against the runtime version the run recorded, not a promise about tomorrow's.
- **Unsigned and unpackaged.** SmartScreen behaviour, install, upgrade and uninstall are entirely untouched.

## Manual verification (a human, on a Windows box)

The steps a runner cannot do. Run a debug build so devtools are enabled:

```
cargo run -p werust-windows
cargo run -p werust-windows -- ipfs://<cid>/
```

1. **It opens and renders.** The window shows the toolbar, an empty URL bar, the trust indicator, the status line and a page area. Type `example.com` and press Enter: the page loads, the URL bar keeps the typed text's resolved URL, the status line follows the load.
2. **Progress does not move the page.** Watch during a load: the progress strip appears INSIDE the URL bar and the page area does not resize by a pixel. Hover the URL bar mid-load: the tooltip names the phase and offers the cancel hint.
3. **A failure DOES move the page.** Load `ipfs://` with a CID whose bytes do not verify (or pull the network): the red banner appears under the toolbar, the page area shrinks, and the reason is werust's own protocol-named text.
4. **Trust, and its EXPLANATION.** On a hash-verified `ipfs://` page the badge reads verified and green; HOVER it and read the explanation. On a plain `https://` page it reads as a served origin. On an IPNS/ENS name it must never read "verified".
5. **The invalid entry.** Type `not a url` and press Enter: the text is KEPT, painted red, the badge appears, and NOTHING navigates.
6. **Back / forward / reload / stop** each do what they say, and Stop is only enabled while a load is in flight (Reload only when it is not).
7. **The ⋮ menu** shows `werust <version>` greyed and `Debug` live. Debug opens the debug view; the page's `console.log` appears in the Console tab and its `fetch` calls in the Network tab, each with an honest trust label. Clear empties both. Close it and re-open it from the menu: it opens fresh.
8. **Devtools.** Press F12 with the focus in the URL bar, and again with the focus in the page: Edge's real DevTools window opens both times. In a `--release` build it must NOT.
9. **Follow the OS.** With the window open, flip Windows' Settings → Personalisation → Colours between Light and Dark: the chrome AND the title bar follow, without a restart, and the page's `prefers-color-scheme` follows too. werust must never be dark while Windows is light.
10. **HiDPI.** Open it on a 150% or 200% display, and drag it between monitors with different scale factors. Expect blur (see above) — record what you see, that is the input the DPI follow-on needs.
11. **Resize, maximise, minimise, restore.** The page keeps filling the area between the toolbar and the status line at every size.
12. **A `target="_blank"` link navigates IN PLACE** (ADR-0010) — no second window opens.
13. **The profile is durable.** Confirm `%LOCALAPPDATA%\werust\WebView2` exists after a run, and that a cookie/localStorage value survives closing and re-opening werust.

## Re-running the checks

```
# the ordinary gate (Linux, no Windows box needed)
cargo fmt --check && cargo clippy && cargo build && cargo test

# the local cross-target type-check of every Windows source (NOT a build)
LLVM_BIN=/usr/lib/llvm-19/bin ./docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh

# on Windows, or via `gh workflow run windows-renderer.yml --ref <branch>`
cargo run -p werust-windows --example window_smoke
```
