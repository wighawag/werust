# The desktop debug view: Console + Network tabs from the menu's Debug entry

Task: `debug-view-console-network-tabs-desktop`. Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`. Decisions: [`DECISIONS.md`](DECISIONS.md).

The desktop browser menu's Debug entry (the ⋮ menu at the right end of the toolbar, `general-browser-menu-with-version-and-debug-entry`) now opens the real in-app debug view: a separate `werust Debug` window with a CONSOLE tab and a NETWORK tab, rendered live from the shared capture store (`werust_core::debug::DebugCapture`, `debug-capture-store-console-and-network-in-core`) that the desktop capture points feed (`debug-console-network-capture-per-platform`). This is the standalone, no-tether debug surface; the native F12 WebKit inspector is untouched and stays as the deep devtools (the typeable console REPL, DOM, sources).

## Where it lives

All in `crates/werust/src/main.rs`:

| Piece | What it is |
| --- | --- |
| `open_debug_view(&window, &debug_view)` | the hook the menu's Debug entry calls (the menu task's placeholder is gone); opens/presents the window |
| `DebugViewState` | the shared state the menu entry and the pump see: the capture store + the currently-open view |
| `DebugView` | the window: a header (`Console + Network capture` title + `Clear` button) over a `Notebook` of the two tabs |
| `console_row` / `network_row` | the row builders (labels only, READ-ONLY) |
| `console_row_text` / `console_level_css_class` / `console_source_line` | the pure Console-tab mapping (level + message + source:line, coloured per level) |
| `network_status_text` / `network_mime_text` / `network_size_text` / `network_trust_label` / `network_trust_css_class` | the pure Network-tab mapping (method / status / mime / size / trust / url), unknowns rendered as `?` |
| the 50ms pump in `open_window` | refreshes the open view on the EXISTING cadence (incremental append anchored on the store's monotonic entry sequence; no new timer, no busy loop) |
| `tail_plan` | the pure per-tab refresh plan: append the entries after the last-rendered sequence, rebuild when that anchor was evicted (at the ring-buffer cap) or the store was cleared |

The CONSOLE tab shows one row per captured console entry: `[<level>] <message> (<source>:<line>)`, coloured by level (`debug-console-error` red, `debug-console-warn` amber, `debug-console-info` blue, `debug-console-debug` grey). The NETWORK tab shows one row per captured request: method, status, MIME, size, the honest per-request trust posture, and the URL. The trust column speaks the chrome trust indicator's EXACT vocabulary (ADR-0006): its glyph, the core's wire name (`✓ content-verified`, `⚠ unverified-origin`, `◈ name-via-trusted-rpc`, `◇ mutable-name`), and the same `trust-*` CSS classes, so a content-verified ipfs:// row is the same green the indicator's verified badge is. No new trust label exists.

The `Clear` button calls the store's `clear()` (both buffers); the next refresh resets both lists. Rows are newest-at-bottom with stick-to-bottom auto-scroll. Closing the window drops it; activating Debug again opens a fresh one (or raises the open one).

## What the automated gate covers, and what it cannot

In the pure-Rust `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`):

- `crates/werust/src/main.rs` unit tests: the render-from-store mapping (console row text with level + message + source:line and honest absent fields; the per-level CSS classes all distinct; network status/mime/size columns with unknowns as `?`; the trust column composed of the indicator's glyph + the core's wire name + the indicator's own CSS classes, distinct per posture, including the spec's ipfs:// = `✓ content-verified` / https:// = `⚠ unverified-origin` split), and the refresh plan (`tail_plan`): the past-cap test drives the REAL store past its 300-entry cap and asserts the newest entry still renders and the evicted rows drop (the Gate-2 defect), display-free and network-isolated.
- `crates/werust-core/src/debug.rs` unit tests (added with the fix): every pushed entry carries a monotonic `sequence` that survives `pop_front` eviction, a `clear()` never rewinds it, and it never reaches the debug JSON.
- `crates/werust-core/tests/debug_view_desktop_wiring_shape.rs`: the wiring shape (the Debug hook opens a Notebook of Console + Network tabs over the shared store; Clear drives the store's `clear()`; the view refreshes inside the ONE existing pump timeout; the Network tab reuses `trust_posture_wire_name` and the `trust-*` classes; the view builds no input widget; the F12 inspector wiring is untouched; the parity-matrix row tracks the mobile gap).
- `crates/werust-core/tests/browser_menu_edge_wiring_shape.rs` (updated): the menu still routes the Debug id to the named hook; the two MOBILE hooks keep their honest placeholder until `debug-view-console-network-tabs-mobile`.
- `crates/werust-core/tests/platform_capability_parity.rs`: the new `debug-view-console-network` row (desktop `implemented`, both mobile cells `stubbed` onto the mobile view task).
- The store itself (bounded buffers, `clear()`, the trust rule) remains covered by `werust-core`'s `debug` module tests; the capture points by `debug_capture_edge_wiring_shape.rs`.

What no automated test in this repo can cover: that the real GTK window appears, paints, scrolls, and clears on a display. Those are the manual steps below.

## Manual verification steps (the GTK window)

Not yet executed in this task (no display session was run). Each step is written so it can be executed and its result recorded here.

1. `cargo run -p werust` on a machine with a display.
2. Open the ⋮ menu at the right end of the toolbar and click `Debug`: a separate `werust Debug` window opens with two tabs, `Console` and `Network`, and a `Clear` button in the header.
3. Console tab: after the default page load (and after navigating to any page that logs), console entries appear as `[<level>] <message> (<source>:<line>)`; errors are red, warnings amber. New entries append at the bottom and the list auto-scrolls while it is at the bottom; scrolling up and letting more entries arrive does not yank the view back down.
4. Network tab: after a load, requests appear with method, status, MIME, size, trust and URL. An `https://` page's rows read `⚠ unverified-origin` (amber); on an `ipfs://` page (e.g. `ipfs://<cid>/...` or an ENS name), the verified rows read `✓ content-verified` (green), matching what the toolbar trust indicator shows for the same page (an ENS page's main-document row reads `◈ name-via-trusted-rpc`, never contradicting the indicator).
5. Live update: with the debug window open, navigate the browser window (or reload): new console + network rows appear within a moment (the 50ms pump cadence), with no window reopen needed.
6. Clear: click `Clear` in the debug window: both tabs empty immediately; new captures keep flowing in afterwards.
7. Toggle: close the debug window (its own close button) and click `Debug` again: a fresh window opens. Clicking `Debug` while the window is already open raises it rather than opening a second copy.
8. F12 coexistence: with the debug window open, press F12 in the browser window: the native WebKit inspector still opens in-window (debug builds), independent of the debug window; both stay open together.
9. Read-only: clicking or typing in either tab never edits anything (rows are selectable for copying only); there is no input field in the debug window.
10. Long session (the at-cap case): keep the debug window open through a busy session that pushes past 300 console/network entries (a few navigations to heavy pages suffice): the tabs keep updating with the newest entries (they do NOT freeze at the 300th), the oldest rows roll off the top, and the row counts stay at 300. (Automated equivalent: the past-cap section of the ignored end-to-end test, `cargo test -p werust -- --ignored` on a display.)
