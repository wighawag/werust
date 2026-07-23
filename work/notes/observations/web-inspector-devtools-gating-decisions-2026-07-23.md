# Web inspector (devtools) gating + shortcut decisions (2026-07-23)

Recorded for task `enable-web-inspector-devtools-all-platforms` (spec
`ens-to-ipfs-resolution-phase1-rpc-skeleton`). These are the load-bearing
choices this task made; linked from the task's done record. None re-mean an
existing CONTEXT.md glossary term or overlap the parity-matrix concepts; they
introduce a new capability row (`web-inspector`) and a per-platform build/dev
gate.

## Confirmed premise (re-checked, as the prompt asked)

- Desktop `crates/webview-renderer/src/backend.rs`: `WebView::builder()` set NO
  `WebKitSettings`, so `enable-developer-extras` was OFF today. The WebKit Web
  Inspector cannot be opened without it. Confirmed by reading the file before
  touching it (no `.settings(...)` / `WebKitSettings` anywhere).
- The GTK INTERACTIVE debugger (widget tree / CSS, NOT web content) is bound to
  Ctrl+Shift+I and Ctrl+Shift+D (GTK4 docs: https://docs.gtk.org/gtk4/running.html).
  The human's "shift+F12" recollection notwithstanding, the real GTK default is
  Ctrl+Shift+I/D. So a web-inspector shortcut MUST avoid Ctrl+Shift+I.

## Decision 1 — desktop web-inspector shortcut is F12 (avoids the GTK debugger)

Chosen: F12 opens the WebKitGTK Web Inspector over the page.
Why: F12 is the desktop-browser-idiomatic devtools key and is NOT bound by the
GTK interactive debugger (which owns Ctrl+Shift+I / Ctrl+Shift+D). Using F12
therefore cannot collide with the GTK debugger, satisfying the acceptance
criterion. The task's parenthetical also floated Ctrl+Shift+I, but that DOES
collide with the GTK debugger, so it is rejected here.
Alternatives considered: Ctrl+Shift+I (rejected: collides with GTK debugger);
intercept-and-route Ctrl+Shift+I to the web inspector (rejected: needlessly
steals GTK's own debugger key and is more fragile than picking a free key).
Touches: the shell key handling in `crates/werust/src/main.rs` only; no other
command/flag.

## Decision 2 — developer-extras / inspectability gated on a DEBUG build

Chosen: enable the inspector capability only in a debug build on every platform,
via each platform's native debug signal, so a RELEASE build is not silently
inspectable:
- Desktop: gate `enable-developer-extras` + the F12 shortcut behind
  `cfg!(debug_assertions)` (true for `cargo build`/dev, false for
  `cargo build --release`). werust's release binaries are built `--release`
  (GoReleaser Rust builder, ADR-0002), so shipped desktop builds are NOT
  inspectable; a developer `cargo run` build is.
- Android: gate `WebView.setWebContentsDebuggingEnabled(true)` behind
  `BuildConfig.DEBUG`. The app module only defines a `debug` (unsigned) build
  type today, but gating on `BuildConfig.DEBUG` is future-proof: a later
  `release` build type is not inspectable by default.
- iOS: gate `webView.isInspectable = true` behind `#if DEBUG` (iOS 16.4+ guarded
  by `if #available`). Simulator builds (the only supported iOS build today) are
  DEBUG, so the sim is inspectable; a future device/release build is not.
Why: the task explicitly asked to "gate mobile inspectability + desktop
developer-extras behind a debug build / setting and record the decision", so a
release build is not silently inspectable if that is a concern. A build-type gate
(vs a runtime user setting) is the lowest-friction, no-new-user-surface choice
and matches each platform's idiomatic debug signal; it introduces no new config
key or user-visible flag.
Alternatives considered: a runtime user setting (rejected for now: adds a new
user-visible surface + persistence for a developer feature; a debug-build gate is
simpler and is what the platforms' own inspector affordances key off anyway). A
release build could still opt in later by flipping the gate, without reworking
the wiring.
Touches: desktop backend + shell, Android `BrowserActivity`, iOS
`WKWebViewShellController`; a new `web-inspector` capability row in
`docs/platform-capability-matrix.toml` (all three `implemented`).

## Decision 3 — capability name is `web-inspector`

Chosen: register the parity-matrix row as `web-inspector`.
Why: it is the platform's OWN full inspector (WebKit/Chrome devtools: console
REPL + network), not a custom werust window; the name says exactly that and does
not overlap any existing capability row (address-bar, ipfs-render, provider,
trust-indicator, ...). All three cells are `implemented` per the matrix's "per
its reach" rule (desktop in-window, iOS via Safari over USB, Android via
chrome://inspect over USB) — each platform really wires its native inspector.
