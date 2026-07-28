# Decisions: the desktop debug view (Console + Network tabs)

Task: `debug-view-console-network-tabs-desktop`.
Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`.
Code: `crates/werust/src/main.rs` (the `DebugView` window, the row builders, and the pure render-from-store mapping functions), guarded by `crates/werust-core/tests/debug_view_desktop_wiring_shape.rs`.

These are the judgement calls this task bakes in, recorded so the mobile view task (`debug-view-console-network-tabs-mobile`) and the reviewer inherit them explicitly. Manual steps + a map of the wiring: [`README.md`](README.md).

## Decision 1: the debug view is a SEPARATE WINDOW, not an in-window panel

The Debug entry opens a `gtk4::Window` transient for the browser window (title `werust Debug`, 760x480 default), holding a `Notebook` of the two tabs. The task offered the choice (panel vs window/dialog) and asked for it to be recorded.

- **What it touches.** Only the desktop edge; the mobile view is a full-screen tabbed page by its own task.
- **Why.** (a) A browser-idiomatic bottom/side panel would crowd the page view on every open and would need its own show/hide plumbing in the one-window shell; a window gets "toggled closed again" from the platform's own close button, and re-activating Debug PRESENTS (raises) the existing window rather than opening a second copy. (b) It coexists cleanly with the F12 WebKit inspector, which is itself in-window: the two debug surfaces never fight for the same pane. (c) The window needs no change to the browser window's layout, so the change stays scoped to the hook the menu task left.
- **The alternative considered.** A togglable bottom panel (Chrome/Firefox devtools style). Rejected for the reasons above; the spec names the standalone no-tether surface, and a separate window is the simplest honest form of it on a desktop shell that has no docking model. If a panel is wanted later, the `DebugView` struct is the one site to reparent.

## Decision 2: the view refreshes on the EXISTING 50ms chrome pump, incrementally

The open debug view is refreshed inside the ONE existing `glib::timeout_add_local(Duration::from_millis(50), …)` pump that already drives the chrome, right after the shell's `pump()`. No second timer, no signal, no busy loop.

- **What it touches.** The pump closure in `open_window` (the shape guard asserts `timeout_add_local` still appears exactly once in the file).
- **Why.** The task requires "poll the store on the existing pump/refresh cadence … do not add a busy loop". The capture store changes off the seam's load-lifecycle events (console messages and resource loads arrive independently of `BrowserShell::pump()` returning true), so the view refresh runs on EVERY tick, but it is INCREMENTAL: each tab remembers how many entries it has rendered and appends only the tail (a `clear()` shrinks the store, which resets the lists). An idle tick is therefore two length checks; there is no per-tick rebuild of a few hundred rows.
- **The alternative considered.** A `glib::timeout` of its own for the view, or rebuilding the lists only when a count changes from a cached value. The first adds a second timer the task told us not to add; the second is what the incremental append already is, without throwing away the widget tree on every change (which would also reset the user's scroll position).

## Decision 3: rows are newest-at-BOTTOM, with stick-to-bottom auto-scroll

Both lists render oldest-first and append new rows at the bottom, the devtools-console idiom (Chrome DevTools, `journalctl` without `-r`). Auto-scroll engages ONLY when the list is already at the bottom when the new rows arrive (`is_at_bottom` + a deferred `idle_add_local_once` scroll, deferred because the adjustment's upper bound updates only after layout); a user scrolled up reading an earlier entry is never yanked back down.

- **Why recorded.** The task left the order open ("newest at the bottom (or top - record), auto-scroll optional"). Bottom + sticky scroll matches the mental model of a log (time flows down) and of every console the user already knows; newest-at-top would read as a feed, which a console is not.

## Decision 4: the Network tab speaks the trust indicator's EXACT vocabulary (glyph + wire name + CSS class)

Each network row's trust column is `network_trust_label(posture)`: the chrome trust indicator's glyph for the posture (`✓` / `◈` / `◇` / `⚠`) plus the core's wire name from `werust_core::debug::trust_posture_wire_name` (`content-verified`, `unverified-origin`, `name-via-trusted-rpc`, `mutable-name`), coloured by `network_trust_css_class(posture)`, which returns one of the SAME `trust-*` classes the indicator toggles (`trust-verified` etc., already styled in the app stylesheet).

- **What it touches.** The desktop Network tab; the mobile view task should render the same label the same way (it can reuse `trust_posture_wire_name` and the debug JSON's `trust` field, which carries the same names).
- **Why.** ADR-0006 and the spec are explicit: reuse the trust-indicator posture words exactly; do not invent a new label. Composing the indicator's glyph with the core's wire name (rather than reusing the page-level badge text `✓ verified`) keeps the per-request label self-describing in a table where the word "verified" alone would be ambiguous (content? name?), while every word and colour is one the indicator already owns. An ipfs:// row reads `✓ content-verified` in the indicator's green; an https:// row reads `⚠ unverified-origin` in the indicator's amber.
- **The alternative considered.** Reusing the page-badge strings (`✓ verified`, `⚠ unverified origin`) verbatim. Rejected: `✓ verified` is the PAGE-level summary wording; per-request the wire name is the more precise, and it is the same vocabulary the debug JSON already carries for the mobile edges.

## Decision 5: the view is READ-ONLY by construction, and unknown fields render as `?`

Every row is a non-editable (but text-selectable, so entries can be copied) `Label`; the debug-view code constructs no `Entry`/`TextView` at all, which the shape guard asserts. A typeable console REPL stays the native F12 inspector's job (spec Out of Scope). An unknown optional field (no status, no size, no mime, no source line) renders as `?` or stays absent, never a fabricated `0` / `:0`, mirroring the store's own honesty rule (its JSON serialises unknowns as `null`).

- **Pinned by** the `main.rs` unit tests (`network_rows_carry_method_status_mime_and_size_with_unknowns_honest`, the console source-line cases) and the shape guard's read-only assertion.

## Decision 6: the store handle reaches the view as a `DebugCapture` clone, not through the chrome JSON

The view holds another clone of the one `Arc` `DebugCapture` the capture points feed and the shell owns (created in `open_window`, passed to `build_menu_button` → `open_debug_view`), and reads `console()` / `network()` snapshots directly.

- **What it touches.** Desktop only; the mobile edges read the SAME store over the FFI `debug_json()` document, which is their only route (they are separate processes/languages).
- **Why.** Desktop is the same process as the core: going through the JSON document would re-encode a few-hundred-entry store on every pump tick for no consumer but this one window, and would lose the typed entries the row builders pattern-match on. The store's recorded FFI decision (its Decision 1: a dedicated `debug_json()` for the mobile edges, keeping the chrome JSON lean) already scoped the JSON document to the edges that need it; the desktop view reading the typed handle directly is the same decision seen from the desktop side. The vocabulary the JSON carries (level and trust wire names) is still what the desktop renders, via the same core functions, so the surfaces cannot drift.
