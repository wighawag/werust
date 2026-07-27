---
title: "A bounded console + network capture store in werust-core, exposed over the chrome/FFI surface, honest trust-posture per network entry"
slug: debug-capture-store-console-and-network-in-core
spec: in-app-debug-menu-console-and-network
blockedBy: []
covers: [5, 6]
---

## What to build

Foundation for werust's in-app debug menu (design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`). Add a toolkit-free CAPTURE STORE in `werust-core` that holds recent CONSOLE log entries and NETWORK request entries, exposed over the SAME chrome / FFI-JSON surface every edge already reads (like `chrome()` / the chrome JSON, `is_loading`, `trust_posture`, `load_step`). This is the ONE shared fact the per-platform capture points feed and the per-platform debug view renders; it lands FIRST so those build on it.

READ-FIRST / drift check: confirm there is no console/network capture yet (a `git grep` for `console-message`/`onConsoleMessage`/network capture returns nothing in the platform code) and that `ChromeState` + the FFI JSON (`crates/werust-core/src/lib.rs`, `crates/werust-android/rust/src/ffi_json.rs`, `crates/werust-ios/rust/src/ffi_json.rs`) are the shared surface the edges read.

Build:
- **Entry types** (in `werust-core`): a `ConsoleEntry { level (Log/Info/Warn/Error/Debug), message, source (url), line, timestamp }` and a `NetworkEntry { method, url, status, mime, size, from_cache, scheme, trust: TrustPosture, timestamp, duration }`. The network entry carries werust's HONEST trust posture PER REQUEST (an `ipfs://` request is `ContentVerified`, an `https://` subresource is `UnverifiedOrigin`, etc.) - coherent with ADR-0006, do NOT re-mean the trust posture. Keep the fields pragmatic; record the shape.
- **A bounded store** on the shell (or a dedicated `DebugCapture` the shell owns): two ring buffers (console, network), each capped at a sane maximum (e.g. a few hundred entries) with oldest-evicted, so a long session does not grow unboundedly (mirror the retrieval-budget / ens_pages bounded-state discipline). Expose:
  - `push_console(entry)` / `push_network(entry)` - the capture points (task `debug-console-network-capture-per-platform`) call these.
  - read access for the debug view: the entries reach the edges over the chrome / FFI JSON (a `debug` section, additive - existing readers ignore it), OR a dedicated `debug_json()` / accessor if that keeps the chrome JSON lean (decide + record; additive either way, no existing field re-meaned).
  - `clear()` (the debug view's clear button will call it).
- **Capture gating hook**: phase-1 is ALWAYS capture, but design the store so a future `set_network_capture_enabled(bool)` toggle (task `debug-network-capture-toggle-config`) is a small addition, not a rework (e.g. a capture-enabled flag the push checks; default true now).

Keep it CORE-only (no platform/UI code here): the capture POINTS and the debug VIEW are separate tasks. This task is the seam + the store + the entry types + the FFI surface, fully unit-testable.

## Acceptance criteria

- [ ] `werust-core` has `ConsoleEntry` + `NetworkEntry` types and a BOUNDED capture store (ring buffers, oldest-evicted, capped) owned by the shell; `push_console`/`push_network`/`clear` mutate it.
- [ ] The `NetworkEntry` carries an honest per-request `TrustPosture` (coherent with ADR-0006; not a re-meaning); an ipfs:// entry is content-verified, an https:// one is unverified-origin.
- [ ] The captured data reaches the edges over the shared chrome / FFI JSON (additive `debug` section or a dedicated accessor - recorded), so all four edges can render the SAME store; existing chrome readers are unaffected.
- [ ] The store is bounded (a capped size, oldest-evicted) so a long session does not grow unboundedly; a network-capture-enabled flag exists (default true) so the later toggle is a small addition.
- [ ] Core-only (no platform/UI code); fully unit-tested: pushing past the cap evicts oldest, clear empties, the FFI/accessor round-trips console + network entries with their fields incl. the per-entry trust posture. Network-isolated.

## Blocked by

- None. (Foundation; `debug-console-network-capture-per-platform` and the debug-view tasks build on it.)

## Prompt

> Goal: add the FOUNDATION for werust's in-app debug menu - a bounded console+network CAPTURE STORE in `werust-core`, exposed over the shared chrome/FFI-JSON surface the edges already read. Core-only; the capture points and the debug UI are separate tasks. Design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`.
>
> Where to look: `crates/werust-core/src/lib.rs` (`ChromeState`, the shell), `crates/werust-android/rust/src/ffi_json.rs` + `crates/werust-ios/rust/src/ffi_json.rs` (the FFI JSON surface). Add `ConsoleEntry{level,message,source,line,ts}` + `NetworkEntry{method,url,status,mime,size,from_cache,scheme,trust:TrustPosture,ts,duration}`; a bounded store (two ring buffers, capped, oldest-evicted) on the shell with push_console/push_network/clear; expose the data over the chrome/FFI JSON (additive `debug` section OR a dedicated accessor - record which). NetworkEntry carries an HONEST per-request trust posture (ADR-0006; ipfs://=ContentVerified, https://=UnverifiedOrigin) - do NOT re-mean trust. Add a network-capture-enabled flag (default true) so the later toggle is a small add.
>
> Done = the types + bounded store + FFI surface + the capture-enabled flag, core-only, unit-tested (cap evicts oldest, clear empties, FFI round-trips entries incl. trust). FIRST re-check no console/network capture exists yet.
