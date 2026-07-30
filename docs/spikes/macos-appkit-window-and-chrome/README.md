# macOS: the AppKit window that paints the chrome — what landed, and what is proven by what

Task: `macos-appkit-window-and-chrome`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the "how `macos-desktop-build` should be split" block, sub-task 3. Engine it sits on: [`macos-wkwebview-renderer-backend`](../macos-wkwebview-renderer-backend/README.md). Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md).

**Read this first.** This work was WRITTEN on Linux, blind, and it has since been RUN on a Mac: the `macos-14` leg this task extends is green against this branch ([run 30572253620](https://github.com/wighawag/werust/actions/runs/30572253620), see [What CI proved](#what-ci-proved)). Everything below is still split by what proves it, because the three sources prove very different things: an ORDINARY Ubuntu `verify` run, the LOCAL cross-target type-check, and the `macos-14` job. Where a claim is a macOS runtime result it names the run; where it is host-independent it says so; where nothing has checked it, it lives under [What still awaits a Mac](#what-still-awaits-a-mac-stated-plainly). The engine task's own earlier measurement is cited only where it is genuinely the engine's result, not this window's.

## What landed

- **`crates/werust-macos`** — the AppKit window, in two halves:
  - `src/paint.rs`, the **host-independent** half: every value the window paints, derived from the shared `werust-core` rules (`status_line`, `trust_indicator` / `_detail` / `_css_class`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`, the debug view's row rules, the `BrowserMenu` items) plus this edge's own palette and the macOS debug CAPTURE POINTS over the `Renderer` seam. It compiles and is **unit-tested on Ubuntu against the real core**, so "the macOS window paints the shared derivation" is a checked fact, not a claim.
  - `src/window.rs`, the **AppKit** half (`#[cfg(target_os = "macos")]`): an `NSWindow` with the toolbar (URL bar + progress, back/forward/reload/stop, the invalid-entry badge, the trust indicator, the ⋮ menu), the error banner, the status line, the embedded page view, the macOS menu bar, and the tabbed Console + Network debug view. It assigns fields of a `ChromePaint` to widgets and forwards actions to the shared `BrowserShell`. It contains no rule.
- **The debug-view ROW rules moved into `werust-core`** (`crates/werust-core/src/debug.rs`): `console_row_text`, `console_source_line`, `console_level_css_class`, `network_status_text` / `_mime_text` / `_size_text` / `_trust_label` / `_trust_css_class`, the incremental-refresh `tail_plan`/`TailPlan`, and the exported `DEBUG_CONSOLE_CSS_CLASSES` family. They were private to the GTK edge; both desktop debug views now paint from the one derivation, and the tests moved with the rules.
- **`crates/werust-macos/examples/window_smoke.rs`** — the window's only execution anywhere: a real `NSWindow` opened FAR off-screen, a pinned in-memory hash-verified `ipfs://` page loaded through the PRODUCTION verifying route, and assertions on what the real widgets hold, plus a negative control whose bytes do not hash to their CID.
- **`.github/workflows/macos-renderer.yml`** — extended: it now builds and tests `werust-macos` and RUNS `window_smoke`, and its path filters include the window crate.
- **The local type-check harness** ([`typecheck-macos-from-linux.sh`](../macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh)) — extended to cover the window crate and its smoke, which is how the AppKit code below was iterated on from Linux at all.

One engine change was forced by hosting it in a resizable window: `MacosRenderer::realize` now gives the `WKWebView` a width+height autoresizing mask, so the page keeps filling the container view when the window is resized. It is one line, it is not chrome, and without it the page would keep the size it had when the engine was realised.

Not in scope, deliberately: no code signing, no notarization, no `.app` bundle (task `macos-release-packaging-leg`), and no `macos` column in the platform-capability matrix (task `macos-parity-column-and-stub-tasks`, which runs after this so the cells describe what really shipped).

## It PAINTS: where each surface's rule actually lives

| surface | the rule (shared, `werust-core`) | what macOS does |
|---|---|---|
| URL bar text | `ChromeState::url_text` | `setStringValue`, only when it changed (no caret jump) |
| invalid entry | `invalid_entry_badge_visible` / `_text` | shows the badge, colours the field's text |
| back / forward | `ChromeState::can_go_back` / `_forward` | `setEnabled` |
| stop vs reload | `ChromeState::is_loading` | `setEnabled` |
| status line | `status_line` | `setStringValue` |
| trust indicator | `trust_indicator`, `trust_indicator_detail`, `trust_indicator_css_class` | label + tooltip + the class's colour |
| error banner | `error_banner_visible` / `_text` / `_css_class` | shown only on failure, in the severity's colour |
| load progress | `load_progress_visible` / `_fraction` / `_hint` | an `NSProgressIndicator` INSIDE the URL bar + its tooltip |
| ⋮ menu + menu bar | `BrowserMenu` | `NSMenu` items, dispatched by stable id |
| debug rows | `console_row_text`, `network_*`, `tail_plan` | one `NSTextField` per column, coloured by the class |

The CSS-class NAMES come from the core's exported sets (`TRUST_INDICATOR_CSS_CLASSES`, `ERROR_BANNER_CSS_CLASSES`, `DEBUG_CONSOLE_CSS_CLASSES`); the palette that gives each name a colour is this edge's, exactly as `APP_CSS` is the GTK edge's. A core class with no colour here reds the Ubuntu gate (`every_exported_class_has_a_colour`), which is the macOS twin of the GTK no-unstyled-class guard.

## What the Ubuntu `verify` gate proves TODAY (every ordinary run)

1. **The window paints the core's derivation, field for field.** `crates/werust-macos/src/paint.rs`'s `the_paint_is_the_cores_derivation_verbatim` drives seven chrome states (default, loading mid-pipeline, content-verified, mutable-name, hard failure, transient failure, invalid entry) and asserts every painted field equals the core function that decides it.
2. **The loading rules are followed.** A load in flight paints the neutral loading badge (no trust claim), shows progress, and raises NO banner; only a failure raises one.
3. **Every exported state class has a colour here** — and a class the core does NOT export has none, so the guard is not vacuous.
4. **The ⋮ menu is the core's `BrowserMenu`**, version line disabled, Debug entry activatable, in order.
5. **The debug rows are the core's row derivation**, including the per-request trust column speaking the chrome's exact vocabulary, and the incremental refresh survives ring-buffer eviction AT the cap (driven against the real store, 300+ entries).
6. **The macOS capture points are the SHARED shims** on the dedicated capture channel (never the provider's trust channel), and a captured message really lands in the store the view renders — driven through a fake `Renderer`, so the wiring is asserted with no WKWebView.
7. **The extraction is behaviour-preserving.** The moved row-rule tests now live in `werust-core` and pass there unchanged; the GTK edge keeps no second copy (asserted by `crates/werust-macos/tests/macos_window_shape.rs` and the updated `debug_view_desktop_wiring_shape.rs`).
8. **The AppKit source has the shape it must have** — `tests/macos_window_shape.rs` parses the file the gate cannot compile: every surface present, no chrome rule called from AppKit-land, no class name or label restated, no `NSAppearance` touched anywhere (ADR-0009), the new-window rule left to the engine (ADR-0010), the page geometry depending on the banner and never on progress, and the CI leg really building/testing/running this crate.

## What the LOCAL type-check proves (and what it does not)

`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` type-checks the engine, the window and both smokes against `aarch64-apple-darwin` from Linux. Run 2026-07-30 on this code: **clean, no errors, no warnings** (`cargo clippy --target aarch64-apple-darwin`).

That means every `objc2` call, every `define_class!` block, every selector signature and every seam signature in `src/window.rs` and `examples/window_smoke.rs` type-checks against the real AppKit bindings. It is **not a build**: it links nothing, sends no message, and swaps `werust-core` for a tiny stand-in (the real one cannot cross-compile from Linux: `ureq -> rustls -> ring` needs a C compiler and an Apple SDK). So it proves the shape of the Objective-C wiring, not that anything RUNS, and not that the window agrees with the real core — that second half is what `paint.rs`'s Ubuntu unit tests cover, which is precisely why the window touches the core only through `paint`.

## What CI proved

**The `macos-renderer` leg is GREEN against this code.** Run [30572253620](https://github.com/wighawag/werust/actions/runs/30572253620) (`workflow_dispatch`, ref `work/task-macos-appkit-window-and-chrome`, commit `d9aeca8`, 2026-07-30, all steps succeeded in 1m01s). That commit's tree differs from the landed one only in `work/` bookkeeping files; no source line differs. The leg RECORDS what it measured on, and this run measured on **macOS 14.8.7 (build 23J520), Xcode 15.4 (15F31d), AppleWebKit/605.1.15** (the last as reported by the origin probe's own user agent).

What each step establishes, and by what kind of evidence:

1. **The AppKit half COMPILES against a real SDK** (step *Build the macOS backend, the window and the origin probe*: `cargo build -p macos-renderer -p werust-macos -p macos-origin-probe`). Until this run, `src/window.rs` and `examples/window_smoke.rs` had only ever been type-checked cross-target from Linux; they are now built by the real toolchain.
2. **The host-independent tests hold on macOS too** (step *Test the macOS crates and the shared toolkit-free half*): `werust-macos`'s 7 paint unit tests and its 10 source-shape guards pass there exactly as they do on the Ubuntu gate, alongside `macos-renderer` (3 + 12), `webview-shared` (5) and `macos-origin-probe` (20 + 4). This is platform COVERAGE of claims the Ubuntu gate already makes, not new claims: everything in [What the Ubuntu `verify` gate proves TODAY](#what-the-ubuntu-verify-gate-proves-today-every-ordinary-run) is checked on both hosts, and none of it needs a Mac to be true.
3. **The engine's trust hooks still work with this task's one engine change in place** (step *Exercise BOTH trust hooks on a real WKWebView*): `trust_hooks_smoke` reports `posture: ContentVerified` on the hash-verified load, the injected provider round-tripping `chainId 0x1`, and `control state: Failed` / `control posture: UnverifiedOrigin` on the tampered control, ending `PASS`. That is what re-tests the autoresizing-mask line (DECISIONS 6).
4. **The real window runs** (step *Build and drive the REAL AppKit window*): `window_smoke` ends in `PASS`. Every runtime claim about the WINDOW below comes from THAT program, and from nowhere else.
5. **The recorded origin verdict still holds on this WebKit** (final step): `registered-ipfs-scheme`, asserted against `docs/spikes/macos-wkwebview-renderer-backend/expected.json`. That is the ENGINE's measurement, re-confirmed; it is recorded where it belongs, in [`macos-wkwebview-renderer-backend/README.md`](../macos-wkwebview-renderer-backend/README.md).

The assertions `window_smoke` made on the real object graph, in the order it made them (its verbatim output is in the run's step log; it is not restated here):

- a real `NSWindow` + toolbar + menu + debug view CONSTRUCT and lay out on a real Mac, which is the class of failure the cross-target type-check cannot catch (a selector typo, a missing feature flag, a bad `define_class!`) and which would show up here as a crash;
- on the fresh window: the trust indicator holds the core's badge, the badge's tooltip is the core's ADR-0006 EXPLANATION, the status line is the core's status, and there is neither an error banner nor an invalid-entry badge (all read back OUT of the real `NSTextField`s);
- the ⋮ `NSMenu`'s titles are the core `BrowserMenu`'s labels, in order;
- a hash-verified `ipfs://` page (offline, pinned, through the production verifying route) settles; the URL bar shows the loaded content-addressed URL; the trust indicator paints the core's verdict and reads as verified; the status line still mirrors the core;
- that successful load raised NO banner, and **the page view did not move or resize across the whole load** (a frame comparison: the URL-bar-progress rule, measured);
- the Debug item is enabled and activatable, activating it OPENED the view, the page's own `console.log` reached the Console tab (page → shared shim → capture channel → shared store → a rendered row), and clearing the shared store emptied both tabs;
- the NEGATIVE CONTROL (bytes that do not hash to their CID) FAILED, raised the prominent error banner carrying the core's protocol-named reason (`⚠ This page failed to load: renderer backend error: ipfs:// content-addressed load failed: block hash mismatch: bytes do not match cid bafkrei…`), displaced the page (the one state allowed to), and was never reported verified;
- closing the debug window cleared the slot, so Debug opens a fresh one.

What this run did NOT touch is unchanged and is listed in the next section: a `macos-14` runner has no display, no GPU and no human, so appearance, dark mode, input, live resize and everything a user does by hand remain unproven. The leg is re-runnable at any time (`gh workflow run macos-renderer.yml --ref <branch>`) and triggers on pull requests touching `crates/werust-macos/**`.

## What still awaits a Mac (stated plainly)

**This window was WRITTEN blind, from Linux.** The green run above leaves all of the following unproven, because a `macos-14` runner is not a desktop with a display, a GPU and a human:

- **How it LOOKS.** Nothing here judges legibility, spacing, font sizes, the hand-computed frames at unusual window sizes, or whether the accent colours (shared with the GTK edge) read well against a dark-mode chrome. The manual steps below exist for exactly this.
- **Dark mode.** ADR-0009 compliance is asserted as an ABSENCE (this crate never sets an `NSAppearance`), and macOS is expected to propagate the effective appearance into both the chrome and the web process. That expectation is untested at runtime.
- **Input, focus, scrolling and HiDPI.** The page view is focused through the seam and AppKit's responder chain carries real input; no automated test drives a click, a scroll or a keystroke.
- **Live resize.** The relayout is hand-computed and is exercised only via `windowDidResize:`; the CI smoke never resizes the window.
- **The debug view's scrolling behaviour.** The stick-to-bottom rule and the tab switching are asserted only as row counts, never as pixels.
- **Anything a user does that the smoke does not**: typing a URL, an ENS name, the invalid-entry path, `_blank` links, the Web Inspector, or a long browsing session.
- **Signing, notarization, packaging and the parity matrix** are other tasks (`macos-release-packaging-leg`, `macos-parity-column-and-stub-tasks`).

## Manual verification (a human, on a Mac)

Build and run:

```
cargo run -p werust-macos                       # opens https://example.com/
cargo run -p werust-macos -- ipfs://<cid>/       # a content-addressed page
```

Then check, in order:

1. **Chrome present.** The window shows: ◀ ▶ ⟳ ✕, the URL bar, the trust indicator on the right, the ⋮ button, and a status line at the bottom. Back/Forward are greyed until there is history; Stop is greyed until a load is in flight, Reload greyed while one is.
2. **Trust indicator.** On `https://example.com/` it reads "⚠ unverified origin"; hovering it shows the long explanation. On an `ipfs://<cid>/` page it reads "✓ verified"; on an `ipns://`/`.eth` page it must NOT read "verified" (ADR-0006/0007).
3. **Load progress lives in the URL bar.** During a slow load a thin bar advances inside the URL bar and its tooltip names the phase (plus "press Stop (✕) to cancel" while the backend load is in flight). **The page must not move or resize at any point during a load.**
4. **Failure surface.** Navigate to a bad `ipfs://` CID (or pull the network): a red banner appears between the toolbar and the page carrying the protocol-named reason, and the page area shrinks (this is the one state allowed to displace it). A timeout reads amber, with "reload to retry".
5. **Invalid entry.** Type `not a url` and press Enter: nothing navigates, the typed text STAYS, it turns red, and a "⛔ invalid URL" badge appears next to the bar.
6. **The ⋮ menu and the menu bar.** Both show the same items: a disabled `werust <version>` line and a Debug entry. The version must match `cargo run -p werust-macos` 's startup banner. The app menu also has Quit (⌘Q).
7. **Debug view.** Choose Debug: a separate "werust Debug" window opens with Console and Network tabs. Load a page that logs and fetches; rows appear live, error rows are red, warnings amber. The Network tab's trust column uses the same words and colours as the chrome badge. Clear empties both. Close the window and choose Debug again: a fresh window opens.
8. **ADR-0009 (follow the OS).** Switch System Settings → Appearance between Light and Dark with the browser open. The window chrome must follow, and a page with `prefers-color-scheme` styling must follow too. werust must never force dark on a light desktop.
9. **ADR-0010 (new windows navigate in place).** On a page with a `target="_blank"` link (or `window.open`), click it: the CURRENT view navigates, and no second window appears.
10. **Resize.** Drag the window edges: the toolbar stays one row, the page fills the space, the status line stays at the bottom, and the page keeps rendering at the new size (this is what the engine's autoresizing-mask change is for).

Please record what you saw — especially anything under "What still awaits a Mac" — as a dated note in `work/notes/observations/`.

## Re-running the checks

```
# the ordinary gate (Linux, no Mac needed)
cargo fmt --check && cargo clippy && cargo build && cargo test

# the local cross-target type-check of every macOS source (NOT a build)
docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh

# on a Mac, or via `gh workflow run macos-renderer.yml --ref <branch>`
cargo build -p macos-renderer -p werust-macos -p macos-origin-probe
cargo test  -p macos-renderer -p werust-macos -p macos-origin-probe -p webview-shared
cargo run   -p macos-renderer --example trust_hooks_smoke
cargo run   -p werust-macos   --example window_smoke
```
