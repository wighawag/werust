# macOS: the WKWebView two-finger swipe back/forward gesture is not enabled (2026-07-31)

Noticed while filling the `macos` column of `docs/platform-capability-matrix.toml` (task `macos-parity-column-and-stub-tasks`): `crates/macos-renderer/src/backend.rs` never sets `allowsBackForwardNavigationGestures` on its `WKWebView`, which defaults to `false` — so the two-finger swipe back/forward a Mac user expects does nothing, and history navigation is only reachable through the on-screen `◀`/`▶` buttons. Exactly the iOS shape recorded in `ios-edge-swipe-back-gesture-not-enabled-2026-07-26.md`, on the same WebKit property.

Not fixed here (out of this task's matrix-and-stub-tasks scope). Recorded because the new `system-back-navigates-history` cell marks macOS `n-a` — macOS genuinely has no OS-level system Back routed to the app — and points at this note as the place its platform analogue lives, so "n-a" is not read as "the swipe gesture works".
