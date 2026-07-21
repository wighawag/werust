# Decisions — browser-shell-url-bar-and-live-interactive-view

Durable record of the design choices made building the browser shell (URL bar, back/forward/reload/stop, live interactive view). Linked from the task done record so a reviewer + the human can ratify or reverse. Current truth remains the code + ADRs; this file only explains the load-bearing choices.

## Extended the `Renderer` seam with session-history navigation

**Chosen:** added four methods to the `Renderer` trait (`crates/renderer/src/lib.rs`) — `go_back()`, `go_forward()`, `can_go_back() -> bool`, `can_go_forward() -> bool` — alongside the existing `navigate`/`reload`/`stop`. `go_back`/`go_forward` default to a no-op and `can_go_*` default to `false`, so a backend without session history (a fixed-subset native path) is not forced to fake one. The WebKitGTK backend (`crates/webview-renderer/src/backend.rs`) overrides all four, delegating to WebKitGTK's own session (back/forward) list (`WebView::can_go_back/go_back/...`).

**Why:** acceptance criterion 2 requires "Back, forward, reload, and stop work and reflect navigation state", and the task mandates all navigation goes THROUGH the seam. The seam as landed by `renderer-seam-trait-and-webview-backend-navigate` had `navigate`/`reload`/`stop` but no history verbs. Back/forward is the same layer as those verbs (a navigation-driving method on the rendering backend), so extending the seam is the coherent placement — not a shell-side URL stack (which would re-implement history the backend already owns, and would drift from the webview's real session list on redirects/fragment navigations).

**Alternatives considered:**
- *A shell-owned URL history stack* driving `navigate` for back/forward. Rejected: it duplicates and would diverge from the backend's real session list (redirects, in-page history), and a future native backend that owns navigation would then have two competing histories.
- *Defer back/forward to a later task.* Rejected: it is an explicit acceptance criterion of THIS task.

**What it touches:** every `Renderer` implementor. Existing test backends and the native T0 backend keep compiling via the no-op/`false` defaults (they legitimately have no session history yet). The future native renderer that owns navigation will override these, exactly as the webview does. Precedent for extending the seam per-task is already set: the conductor's forward-note on `eip1193-provider-injection-via-script-bridge` directs that task to extend the seam with a browser->page response push.

**Coherence check:** the new names do not re-mean any existing `CONTEXT.md`/ADR term; ADR-0001 already lists "history" among the concerns the seam must carry. No overlap with `navigate` (which starts a *new* load) — `go_back`/`go_forward` move within existing session history.

## Shell logic is a GTK-free `BrowserShell` over `dyn Renderer`

**Chosen:** the shell's seam-facing logic (URL bar text, nav controls, chrome reflecting load state) lives in `crates/werust/src/shell.rs` as a GTK-free `BrowserShell` holding a `Box<dyn Renderer>` and a plain `ChromeState` value. `crates/werust/src/main.rs` is a thin GTK view that paints `ChromeState` into widgets and forwards actions.

**Why:** the task says to test the shell<->seam wiring at the seam boundary, not the GTK internals. Splitting the logic out makes navigation-state transitions, chrome updates, history availability, and focus forwarding testable against a fake `dyn Renderer` with no display — mirroring how `webview-renderer` already split its GTK-free `LoadLifecycle` from the GTK `WebViewRenderer`.

**Interactivity via the embedded widget, not `send_*`:** per the task's FORWARD-POINTER, the webview backend's `send_pointer`/`send_key`/`send_scroll` are deliberate no-ops (GTK routes real OS input to the focused embedded widget). The shell therefore achieves "input reaches the page" by embedding the live `ViewHandle` widget and focusing it through the seam (`focus_page` -> `Renderer::set_focus`); the seam tests assert the focus CALL crosses the seam, not that the webview no-ops move anything.
