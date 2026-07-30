---
title: "macOS: capture the MAIN-DOCUMENT and scheme-handled network rows, not only the page's own fetch/XHR"
slug: macos-debug-network-capture-main-document-and-scheme-handled
blockedBy: []
covers: []
---

## What to build

Close the second gap the `macos` parity column had to mark `stubbed`: `debug-capture-console-and-network` in `docs/platform-capability-matrix.toml`.

The CONSOLE half of that capability is real on macOS and was watched working on a `macos-14` runner (the shared `console_shim` on the dedicated capture channel, a page's own `console.log` reaching a rendered row). The NETWORK half is not: the macOS edge's only capture point is the shared `desktop_paint::install_debug_capture`, which injects the best-effort `network_shim` — the `fetch`/`XHR` calls the PAGE makes and nothing else. So the macOS Network tab has NO main-document row, and no row for the `ipfs://` / `werust://` requests werust's own scheme handler serves. That row's description explicitly names the main-document entry (it must mirror the load's own two-axis posture so the Network tab can never contradict the chrome's trust indicator, ADR-0006), so the capability is genuinely short here, not merely thinner.

iOS already has exactly these points and is the model: its Swift edge calls `werust_ios_capture_network` from BOTH `WKURLSchemeHandler`s and from the navigation delegate's main-frame navigation, and the core reconciles which row is the main document through `BrowserShell::is_main_frame` and the load's own posture — never a per-edge URL compare and never a per-edge trust rule. Do the macOS equivalent over the same core entry points: the `SchemeBridge` in `crates/macos-renderer` already sees every intercepted request and its `SchemeResponse` (status, mime, size, and whether it was hash-verified), and the `NavigationBridge` already sees the main-frame navigation lifecycle.

Constraints that matter more than the wiring:

- **Capture is READ-ONLY observation.** It must not alter the load path, the `ipfs://` verification, or any posture, and it must not answer or delay a request. The scheme handler runs the ADR-0008 off-thread boundary; a capture push is a bounded ring-buffer insert and must stay one, off the session lock (the Android ANR shape, `docs/adr/0008`).
- **Derive nothing at the edge.** The per-request posture comes from the core's `request_trust_posture` / the main-document reconciliation, in the trust indicator's exact vocabulary. A second per-edge rule is precisely what `docs/adr/0005` and the chrome-painter rule exist to forbid.
- **Do not silently widen the Win32 edge.** `install_debug_capture` lives in the SHARED `crates/desktop-paint`, and the Windows shell consumes it. The macOS-specific capture points hang off the macOS backend; if any part is genuinely shared, keep `desktop-paint` a CARRIER (no new derivation) and say what you moved. Windows has the analogous gap with different platform hooks (`AddWebResourceRequestedFilter` / the DevTools protocol) and owns it via its own parity column and follow-on task — do not fix it here by accident, and do not leave the Windows edge worse than it was.
- **The residual limit stays honest.** WebKit exposes no per-resource callback, so browser-internal subresources over schemes werust does NOT handle (`<img>`/`<script>`/CSS `url()` over `https://`) remain out of reach on macOS exactly as on iOS. That limit is accepted by the spec's Out of Scope and must stay recorded, not quietly implied away by flipping the cell.

## Acceptance criteria

- [ ] After a load on macOS, the debug view's Network tab shows a MAIN-DOCUMENT row carrying the load's own two-axis trust posture (the same posture the chrome's trust indicator shows for that load), and it can never contradict the indicator.
- [ ] Requests the macOS scheme handler serves (`ipfs://`, `werust://`) appear as rows with their honest per-request posture, method, status, mime and size, derived by the core's existing rules — no new per-edge trust or row rule.
- [ ] Capture stays read-only and off the hot path: no change to what loads, what verifies, or what posture is reported, and no lock held across a retrieval (a test or a stated argument, in the repo's style).
- [ ] The wiring is covered on the Ubuntu `verify` gate (unit tests over the seam/fake plus the source-shape guards `crates/macos-renderer/tests/macos_backend_shape.rs` / `crates/werust-core/tests/debug_capture_edge_wiring_shape.rs` in their existing style), and the `macos-14` leg's `window_smoke` asserts a main-document row really appears after its real hash-verified load.
- [ ] The Windows edge is not regressed or silently changed; if shared code moved, the change says which edges it touches and why.
- [ ] The `debug-capture-console-and-network` row's `macos` cell flips from `stubbed` to `implemented` in the same change, naming what proves it and keeping the residual subresource limit stated; the parity guard stays green with no weakening.

## Blocked by

- None — the macOS engine, window and debug view have all landed (`macos-wkwebview-renderer-backend`, `macos-appkit-window-and-chrome`).

## Prompt

> Goal: give the macOS debug view's Network tab the rows it is missing, and flip the `debug-capture-console-and-network` row's `macos` cell in `docs/platform-capability-matrix.toml` from `stubbed` to `implemented`. Today the macOS edge's only network capture point is the shared injected `network_shim` (page `fetch`/`XHR`), so there is no main-document row and no row for the `ipfs://`/`werust://` requests werust's own `WKURLSchemeHandler` serves — and this capability's description requires the main-document entry to mirror the load's own posture (ADR-0006) so the Network tab cannot contradict the chrome's trust indicator. Model it on iOS, which already does this (`werust_ios_capture_network`, called from both scheme handlers and the navigation delegate): add capture points at the macOS `SchemeBridge` and `NavigationBridge` in `crates/macos-renderer`, pushing into the ONE shared `werust_core::debug::DebugCapture` store the view already renders, with the posture and the main-document decision coming from the CORE (`request_trust_posture`, `BrowserShell::is_main_frame`), never a new per-edge rule. Capture is read-only observation: it must not alter the load, the verification or the posture, and must not hold the session lock across a retrieval (`docs/adr/0008`). `install_debug_capture` is shared with the Win32 edge in `crates/desktop-paint` — keep it a carrier, keep the macOS-specific points on the macOS backend, and do not change Windows behaviour by accident (Windows owns its analogous gap via its own parity column). Keep the WebKit residual limit honest: browser-internal subresources over unhandled schemes stay uncapturable. Cover it on the Ubuntu gate (unit tests over a fake `Renderer` + the existing source-shape guards) and assert the main-document row in the `macos-14` `window_smoke`.
