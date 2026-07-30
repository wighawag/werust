# macOS: the AppKit window that paints the chrome — what landed, and what is proven by what

Task: `macos-appkit-window-and-chrome`. Decision it executes: [`docs/adr/0011-webview2-for-windows.md`](../../adr/0011-webview2-for-windows.md), the "how `macos-desktop-build` should be split" block, sub-task 3. Engine it sits on: [`macos-wkwebview-renderer-backend`](../macos-wkwebview-renderer-backend/README.md). Judgement calls made while building it: [`DECISIONS.md`](DECISIONS.md).

**Read this first.** The macOS CI leg has **not yet run against this code** at the time of writing (this work was written on Linux, and the branch it lives on has not been pushed by the author of the change). Everything below is split into what an ORDINARY Ubuntu `verify` run proves today, what the LOCAL cross-target type-check proves, and what only the `macos-14` job — which this task extends and which triggers on the pull request — can prove. Nothing here claims a macOS runtime result. The engine task's own measured run is cited only where it is genuinely the engine's result, not this window's.

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

**Nothing about this window, yet — the leg has not been run against this code.** Stating it plainly is the point of this section (ADR-0011 Amendment 1, and the Gate-2 lesson from the engine task: a prediction in a measurement's slot is worse than an admitted gap).

- The `macos-renderer` job now carries two new obligations: `cargo build/test -p werust-macos` and `cargo run -p werust-macos --example window_smoke`. Both are steps that EXIST; neither has produced a result for this code.
- The job triggers on the pull request that lands this task (its path filters include `crates/werust-macos/**`), and can be dispatched by hand at any time: `gh workflow run macos-renderer.yml --ref <branch>`.
- What the job proved for the ENGINE this window sits on is a real, earlier measurement and is recorded where it belongs: [`macos-wkwebview-renderer-backend/README.md`](../macos-wkwebview-renderer-backend/README.md) (run 30563185521, macOS 14.8.7, AppleWebKit/605.1.15 — both trust hooks, the fail-closed control, and the `ipfs://` origin verdict). This task changes one line of that engine (the autoresizing mask), which that same job re-tests.

When the run happens, the window smoke is what will have been proved:

- a real `NSWindow` + toolbar + menu + debug view CONSTRUCT and lay out on a real Mac (any Objective-C selector typo, missing feature flag or bad `define_class!` shows up here as a crash, which the type-check cannot catch);
- the URL bar, the trust indicator (badge AND its ADR-0006 explanation tooltip) and the status line hold exactly what the core derived, read back OUT of the widgets;
- the ⋮ `NSMenu`'s titles are the core `BrowserMenu`'s labels, in order, and its Debug item is enabled and activatable;
- a hash-verified `ipfs://` page loads through the window and reads as verified;
- the page view does not move or resize across a whole successful load (the URL-bar-progress rule, measured as a frame comparison);
- the page's own `console.log` reaches the debug view's Console tab (page → shared shim → capture channel → shared store → a rendered row), and clearing the shared store empties both tabs;
- the NEGATIVE CONTROL (bytes that do not hash to their CID) FAILS, raises the error banner with the core's protocol-named reason, displaces the page (the one state allowed to), and is never reported verified;
- closing the debug window clears the slot, so Debug opens a fresh one.

## What still awaits a Mac (stated plainly)

**This window was WRITTEN blind, from Linux.** Even a green CI run leaves all of the following unproven, because a `macos-14` runner is not a desktop with a display, a GPU and a human:

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
