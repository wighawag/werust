---
date: 2026-07-31
---

# The mobile DEBUG VIEW's row rules are still hand-written twins of `werust_core::debug`

Noticed while collapsing the mobile CHROME presentation onto the one derivation (`mobile-chrome-presentation-from-one-derivation`, which was scoped to the chrome only). The debug view has the same shape of duplication one surface over: `DebugView.kt` and `WKWebViewShellController.swift` each carry their own `networkTrustLabel()`, `consoleLevelColor()` and `trustColor()`, while `werust_core::debug` already owns `network_trust_label`, `network_status_text`, `network_mime_text`, `network_size_text`, `console_row_text` and `console_level_css_class`, the very functions `desktop-paint` carries to the AppKit and Win32 debug views. The debug JSON crosses the same FFI on the same cadence, so the same mechanism (carry the derived row strings) would apply directly.

The colours are a second, related twin: both mobile views hard-code hex literals that are transcriptions of `desktop_paint::CLASS_COLORS` (`0x1A5FB4`, `0xC01C28`, …), with no test that the two agree, while the GTK edge has exactly such a test (`the_gtk_stylesheet_and_the_shared_palette_agree`).

Not touched here (out of scope, and the chrome task deliberately left colour at the edges; see `docs/spikes/mobile-chrome-presentation-from-one-derivation/DECISIONS.md` D4).
