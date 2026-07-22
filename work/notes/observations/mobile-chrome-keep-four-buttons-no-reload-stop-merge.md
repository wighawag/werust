# Decision: mobile chrome keeps four compact buttons (no Reload/Stop merge)

2026-07-22 — task `fix-mobile-chrome-urlbar-crowded-by-buttons`.

The task offers merging Reload/Stop into one button as an OPTIONAL, recommended way to reclaim width. I chose NOT to merge and instead kept four compact buttons (Back / Forward / Reload / Stop) on both mobile edges.

Why:
- The `werust-core` chrome model documents the intended pattern as enable/disable, not merge: `ChromeState::is_loading` doc says "The window swaps/enables the two from this," and the desktop GTK shell (`crates/werust/src/main.rs`) keeps four separate buttons, enabling Stop only while loading and Reload only when idle. Merging on mobile would diverge the mobile edges' concept from the desktop edge for no required benefit.
- The required acceptance criteria (compact nav buttons, URL bar as the widest element, >= ~48dp touch targets, unchanged navigation) are all met by compacting the four buttons; the merge is explicitly optional.
- Keeping four buttons keeps BOTH mobile edges consistent with each other AND with desktop, which is the stronger coherence outcome.

What it touches: only the two mobile OS-edge files (`BrowserActivity.kt`, `WKWebViewShellController.swift`). No core, no desktop, no other task. Reversible: a later task can merge on all three edges if desired.

Alternative considered: merge Reload/Stop into a single toggling button on mobile only. Rejected because it forks the button-set concept between mobile and desktop while the core/desktop model is enable/disable.
