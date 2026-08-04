---
title: A `desktop-paint` comment still says the edge contributes its own Stop button's label
date: 2026-08-04
status: open
---

Spotted while collapsing the AppKit Reload/Stop pair (task `reload-stop-collapse-and-spinner-on-the-macos-window`): the comment above `progress_tooltip` in `ChromePaint::of` (`crates/desktop-paint/src/lib.rs`, ~line 295) still reads "this edge contributes only the label its own Stop button carries (`window::build`, a `\"✕\"` title)". Both consumers of this carrier have since collapsed that pair into one control, so neither has a Stop button of its own and neither passes a label: the code passes the core's `STOP_AFFORDANCE_LABEL`, which is now the Stop MODE's glyph. Comment only — the code is correct.

Not fixed here: it is in the shared carrier, which both desktop CI legs watch, and this task was asked to change a painter.
