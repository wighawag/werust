---
title: "The macOS smoke's page-focused Escape check could never discriminate focus, so the focus half of the macOS shortcut layer was unguarded from the day it landed"
date: 2026-08-04
status: closed
kind: observation
severity: unguarded-claim
introducedBy: shortcuts-and-mouse-history-buttons-on-the-macos-edge
closedBy: macos-smoke-blur-url-bar-does-not-end-the-field-editor
affects: crates/werust-macos/examples/window_smoke.rs, docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md
---

Noticed while fixing the RED `macos-renderer` leg on `main`. The leg failed exactly one check:

```
FAIL Escape with the PAGE focused stops the load instead of reverting the bar
```

The check read the URL bar's TEXT after pressing Escape with the page focused, and asserted it still held the half-typed URL. It cannot hold. `press_key -> sendEvent -> claim_key -> perform_chrome_action` is fully synchronous and the assertion ran with no pump in between, so the bar's text at that instant is whatever the pressed action left there:

- the PAGE branch, `ChromeAction::Stop` (`crates/werust-macos/src/window.rs`), calls `shell.stop()` and then `refresh_chrome()`; `refresh_chrome` always calls `Chrome::apply`, which overwrites the URL field with `paint.url_text` whenever it differs, and `ChromePaint::url_text` is verbatim `ChromeState::url_text` (`crates/desktop-paint/src/lib.rs`), i.e. the BELIEVED url;
- the BAR branch, `ChromeAction::RevertUrlBar`, writes that same believed url into the field by design.

So both branches leave the bar showing the believed url, and the assertion was unreachable in BOTH focus states. It did not regress: it never discriminated, from the moment it landed. The failing CI line is fully explained by the Stop repaint alone and is NOT evidence that the smoke's `blur_url_bar` failed to blur (the sibling checks that blur and then assert `Focus::Page` were passing on the same run).

The consequence worth recording: this was the ONE check that was supposed to prove the AppKit edge reports focus correctly, i.e. that focus is a real INPUT to the shared resolution on this edge rather than a constant. Nobody on this project has a Mac (`work/notes/findings/apple-signing-tiers-and-the-no-mac-evidence-gap-2026-08-01.md`), so between `shortcuts-and-mouse-history-buttons-on-the-macos-edge` landing and this note, the focus half of the macOS shortcut layer had NO evidence behind it at all, and `docs/spikes/shortcuts-and-mouse-history-buttons-on-the-macos-edge/README.md` claimed under "What CI proved" that it did (item 8, since corrected).

**The lesson, not the incident:** a check must be written against an observation the two branches can actually differ in. Both branches here converge on the same widget text, so the widget could never be the witness; the EFFECT (an in-flight load that one branch cancels and the other leaves running) is the only thing that separates them, which is what the Windows smoke was already doing.

Closed by `macos-smoke-blur-url-bar-does-not-end-the-field-editor` (`docs/spikes/macos-smoke-blur-url-bar-does-not-end-the-field-editor/README.md`): the symptom check is replaced by that effect-based pair, the reported focus is asserted directly after a blur that now also ends the field-editor session, and a source-shape guard (`crates/werust-macos/tests/macos_shortcut_shape.rs`, `the_two_escapes_are_told_apart_by_the_load_not_by_the_url_bars_text`) reds the ordinary Ubuntu gate if the text-based check ever comes back.
