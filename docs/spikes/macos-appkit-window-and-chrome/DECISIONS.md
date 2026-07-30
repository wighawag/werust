# Decisions — `macos-appkit-window-and-chrome`

The judgement calls this task made, what each one TOUCHES, and the alternatives considered. Recorded here (rather than buried in code) because each is a choice a reviewer, a later task or a user could reasonably be surprised by. The task's own product rules were not re-decided: ADR-0006/0007 (trust posture), ADR-0009 (follow the OS colour scheme), ADR-0010 (new windows navigate in place) and the URL-bar-progress rule are consumed as they stand.

## 1. A separate `werust-macos` BINARY, not a cross-platform `werust`

**Chosen:** a new crate `crates/werust-macos` producing a `werust-macos` binary.

`crates/werust` depends on `gtk4`/`webkit6` UNCONDITIONALLY, so it cannot build on macOS at all; making it conditional would turn a window task into a refactor of the shipped Linux binary, and two binaries cannot both be named `werust` in one workspace. ADR-0011's split already names the macOS crates separately.

**Touches:** `macos-release-packaging-leg` (it packages `werust-macos`, and must decide what the shipped `.app` is CALLED — the binary name is not necessarily the product name); `macos-parity-column-and-stub-tasks` (the column describes this crate). **Alternative considered:** cfg-splitting `crates/werust` into a GTK arm and an AppKit arm — rejected for now, but it is the natural move once a third desktop shell (Windows) exists and the shells are known to be the same shape.

## 2. The AppKit layer paints a SNAPSHOT (`paint.rs`), it does not call the core

**Chosen:** one host-independent module assembles `ChromePaint` / `ConsoleRowPaint` / `NetworkRowPaint` / `MenuItemPaint` by calling `werust-core`, and the AppKit layer only reads those fields.

The AppKit half is the code the Ubuntu gate can never compile. Every rule that leaks into it leaves the reach of the repo's only always-on check. With the seam here, "the macOS window paints the shared derivation" is a UNIT TEST against the real core on every Linux run, and the un-gated half is a straight-line assignment block. It is the same shape the mobile edges already use (Kotlin/Swift paint from `chrome_json()`); the carrier here is a Rust struct because both sides are Rust.

**Touches:** the Windows shell task, which faces the identical problem and should copy this rather than invent a third arrangement; the type-check harness, whose stand-in core is only sound BECAUSE the window touches core through `paint`. **Alternative considered:** calling `werust_core` directly from `window.rs` (fewer types) — rejected: it moves the derivation-agreement out of every checkable surface. **Guard:** `macos_window_shape.rs::the_appkit_layer_paints_and_never_derives` fails if the AppKit half starts calling a chrome rule.

## 3. `DEBUG_CONSOLE_CSS_CLASSES` is its own family, NOT a member of `CHROME_CSS_CLASS_SETS`

**Chosen:** the console-level class family is exported from `werust_core::debug` beside the rule that produces it, and `CHROME_CSS_CLASS_SETS` is left holding exactly the two CHROME families it already had.

`CHROME_CSS_CLASS_SETS` means "the mutually-exclusive families a chrome painter toggles on ONE widget" (the trust badge, the error banner). Console-level classes colour a debug-view ROW; folding them in would re-mean an existing name and would make every painter's toggle loop iterate names belonging to another surface. Same pattern, different surface, so a sibling name rather than a widened one.

**Touches:** both edges' no-unstyled-class guards, which now iterate the chrome sets PLUS this family (the GTK `APP_CSS` test was extended in the same change, and it already styled these classes). **Alternative considered:** adding it to `CHROME_CSS_CLASS_SETS` — rejected on the coherence ground above.

## 4. `tail_plan` moved to the core too, beyond the row helpers the task listed

**Chosen:** `tail_plan`/`TailPlan` (the debug view's incremental-refresh plan) moved into `werust_core::debug` with the row rules.

The task named the row-text helpers; `tail_plan` is the same class of thing (a pure function of a store snapshot) and the macOS view needs exactly it — without it the second view would either re-implement the sequence-anchored eviction rule (the bug the GTK view was fixed for, twice) or rebuild 300 rows every 50ms. Moving it is what makes "both debug views paint from one derivation" true of the refresh as well as the text.

**Touches:** nothing outside the two desktop views today; it becomes the mobile views' answer too if `mobile-chrome-presentation-from-one-derivation` wants it.

## 5. macOS network capture is SHIM-ONLY, and says so

**Chosen:** the macOS capture points inject the shared `console_shim` + `network_shim` over the shared capture channel — the iOS arrangement — and nothing else.

`WKWebView` has no console callback and no per-resource load callback, so injected JS is the only page-wide reach a WebKit shell has. The consequence is honest and recorded: the Network tab sees `fetch`/`XHR` but NOT browser-internal subresource loads (`<img>`, `<script>`, CSS `url()`) and NOT the main document. iOS covers part of that gap with native points its Swift shell drives (`WKURLSchemeHandler` tasks, `WKNavigationDelegate` main-frame navigations); this window does not, yet.

One consequence worth naming: with no main-document row, the "main-document row takes the LOAD's posture" reconciliation the desktop capture does has nothing to reconcile here, so the Network tab cannot contradict the trust indicator — it is simply quieter.

**Touches:** `macos-parity-column-and-stub-tasks` (the `debug-view-console-network` row's macOS cell should say `implemented` for the view and record this capture gap, or stub it onto a follow-up); a future task could add the scheme-handler + navigation-delegate points on the macOS backend the way iOS did.

## 6. One engine line changed: the `WKWebView`'s autoresizing mask

**Chosen:** `MacosRenderer::realize` now sets `ViewWidthSizable | ViewHeightSizable` on the webview inside its container.

The engine's container is the view a HOST embeds and resizes; before this, the page kept the size it had when the engine was realised while the window grew around it. It is engine behaviour (how the engine fills its own container), not chrome, so it belongs there rather than being worked around from the window — which could not reach the webview anyway once the backend is boxed behind the seam.

**Touches:** `crates/macos-renderer`, another task's crate; its own CI leg re-tests it, and its shape guard still holds (no chrome name enters the backend).

## 7. Hand-computed frames, not Auto Layout

**Chosen:** the window is a few fixed-height strips over one flexible page area, laid out by arithmetic in `Chrome::relayout` inside a FLIPPED container view; the debug rows are likewise placed by index.

Auto Layout would mean constraint code that no one on this project can run before CI, for a layout with no ambiguity in it. Hand frames are reviewable on paper and deterministic, and the relayout has exactly one trigger set (open, resize, banner/badge visibility). **Cost:** a resize re-frames every debug row (bounded by the store's 300-row cap). **Alternative considered:** `NSStackView` + anchors — reasonable, and worth revisiting if the chrome grows a genuinely flexible region.

## 8. The palette is the GTK edge's, not macOS semantic colours

**Chosen:** `CLASS_COLORS` carries the same hex values `APP_CSS` uses, on otherwise system-drawn controls.

werust's trust vocabulary should read the SAME on both desktops; picking `NSColor.systemGreen` here and `#0a7d28` there would make the two windows disagree about what "verified" looks like for no reason. The surrounding chrome is standard AppKit and therefore already follows the OS appearance, and this crate never sets an `NSAppearance` (ADR-0009). **Residual risk, recorded rather than guessed:** these hues were chosen against a light GTK theme; whether they are legible on a dark macOS chrome is a MANUAL check (README, step 8). If one is not, the fix is a shared re-tune of both edges, not a macOS fork.

## 9. `werust-macos` has no headless verbs

**Chosen:** the binary takes an optional URL and nothing else; `resolve` / `version` / `--help` stay in the `werust` binary.

That CLI (task `headless-cli-mode`) is toolkit-free and runs anywhere; duplicating its dispatch would give macOS a second, drifting copy of the exact thing this task exists to stop. **Touches:** `headless-cli-mode` — if the macOS binary ever has to be the only one a user has, that dispatch should MOVE into a shared module rather than being copied. On a non-macOS host the binary refuses loudly and names `cargo run -p werust` instead.

## 10. The existing `macos-renderer` workflow was EXTENDED, not forked

**Chosen:** the `macos-14` job gains build/test of `werust-macos` and a `window_smoke` run, plus path filters; the engine task's `typecheck-macos-from-linux.sh` gains the window crate.

Two jobs would double the runner time and let the window's leg drift from the engine's; the harness is explicitly the loop the engine task left "so the sibling window task inherits it". **Touches:** any future Windows shell should read both files before adding a third pattern.

## 11. The smoke asserts on WIDGETS, off-screen, with a pinned fixture

**Chosen:** `window_smoke` opens the real window FAR off-screen as an accessory app, loads a pinned in-memory hash-verified `ipfs://` page through the PRODUCTION verifying route, and reads values back OUT of the real `NSTextField`s / `NSMenu` — with a negative control whose bytes do not hash to their CID.

Asserting on `ChromePaint` there would prove nothing a Linux run has not already proved. What only a Mac adds is that the Objective-C object graph constructs and behaves, so that is what the smoke measures. Off-screen + accessory follows the engine smoke's discipline: a CI run shows nothing and steals no focus. **Note:** as recorded in the README, this smoke had NOT been run against this code at the time of writing.

## 12. The pump timer holds its target for the process's life

**Chosen:** a repeating 50ms `NSTimer` targeting the window controller, never invalidated.

`NSTimer` retains its target, and the controller holds the timer, so the pair lives until the process ends — which for a single-window browser is exactly the run of the app (the GTK shell's `timeout_add_local` pump has the same lifetime). It is worth knowing before a second window or a closable browser window exists: at that point the timer must be invalidated when the window closes.
