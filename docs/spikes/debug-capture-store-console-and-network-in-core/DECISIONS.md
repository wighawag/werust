# Decisions: the debug capture store (console + network) in `werust-core`

Task: `debug-capture-store-console-and-network-in-core`.
Spec: `work/specs/tasked/in-app-debug-menu-console-and-network.md`.
Code: `crates/werust-core/src/debug.rs`, plus the shell wiring in `crates/werust-core/src/lib.rs` and the mobile accessors in `crates/werust-{android,ios}/rust/src/lib.rs`.

This records the judgement calls made while building the FOUNDATION (the store, the entry types, the FFI surface, the capture-enabled flag) so the follow-on tasks (`debug-console-network-capture-per-platform`, the two debug-view tasks, and the Phase-2 `debug-network-capture-toggle-config`) inherit them explicitly instead of re-deriving them.

## Decision 1: a DEDICATED `debug_json()` accessor, not an additive `debug` section on the chrome JSON

The spec left this open ("an additive `debug` section on the existing chrome JSON vs a dedicated `debug_json()` accessor"). Chosen: a dedicated accessor.

- **What it touches.** The mobile FFI surface: `CoreSession::debug_json()` on both mobile cores, `Java_..._nativeDebugJson` / `nativeDebugClear` (Android JNI) and `werust_ios_debug_json` / `werust_ios_debug_clear` (iOS C-ABI, declared in `crates/werust-ios/Sources/werust_mobile.h`). The desktop shell reads `BrowserShell::debug_capture()` / `debug_json()` directly (it holds the shell, it needs no FFI).
- **Why.** The chrome JSON is re-encoded on EVERY chrome refresh (it paints the URL bar, the loading state, the trust indicator), so folding a few-hundred-entry store into it would re-serialize the whole capture on every refresh, for every user, whether or not the debug view is open. The debug document is read only while the debug view IS open.
- **The alternative considered.** An additive `"debug": {...}` section on the chrome JSON. It was additive too (no existing field re-meaned), and would have needed no new FFI export. Rejected on the cost above; the chrome JSON stays byte-for-byte what it was, so every existing chrome reader (desktop, Kotlin `Chrome`, Swift `Chrome`) is untouched. There IS a test on each mobile edge pinning that the chrome JSON gained no debug field.
- **Consequence for the view tasks.** A debug view polls `debug_json()` on the SAME refresh cadence it already has (no busy loop, per the spec's user story 4); it must not be read from the chrome refresh path.

## Decision 2: the capture-enabled flag gates NETWORK capture only, not console

`DebugCapture::set_network_capture_enabled(bool)` (default `true`) gates `push_network`; `push_console` is never gated.

- **What it touches.** The Phase-2 task `debug-network-capture-toggle-config` (the debug-menu capture on/off + reload toggle).
- **Why.** The spec names the Phase-2 toggle as a NETWORK-capture toggle ("network capture is always-on now but the store is shaped so a later config toggle is a small addition"). Making the same flag also silence the console would quietly turn that setting into an everything-switch, so a later task wiring "network capture: off" would surprise a user by also emptying the Console tab.
- **The alternative considered.** One `capture_enabled` flag over both buffers (simpler). Rejected: it re-means the concept the spec named, and console capture is cheap and always wanted (it is the console TAB's only source).
- **If a console toggle is ever wanted**, it is a SECOND flag, named for the console, not a re-meaning of this one.

## Decision 3: a per-entry text bound (`MAX_TEXT_CHARS`) alongside the entry-count bound

The store caps entry COUNT (`MAX_CONSOLE_ENTRIES` / `MAX_NETWORK_ENTRIES`, 300 each, oldest-evicted) AND truncates each captured text field (message, source, url, mime, method) to `MAX_TEXT_CHARS` (2000 chars).

- **Why.** The count cap alone does not bound the store: one `console.log(hugeString)` of a serialized document is megabytes in a single entry, so "bounded" would be false in exactly the case that matters. Truncation keeps the worst case proportional to the cap.
- **User-visible consequence** (hence recorded): a very long console message is shown TRUNCATED in the Console tab. A debug view shows the head of a long message anyway, but the view tasks should not assume the full text is present.
- Counted in `char`s, never bytes, so a cut can never split a UTF-8 sequence.

## Decision 4: `request_trust_posture(scheme, verified)` is conservative, and the name-trust axes are the caller's

The per-request posture helper returns `ContentVerified` ONLY for an `ipfs` request whose bytes ACTUALLY verified; everything else (an `https://` subresource, an `ipfs://` request that did NOT verify, a `werust://` internal page) is `UnverifiedOrigin`.

- **What it touches.** The capture-points task `debug-console-network-capture-per-platform`, which calls it.
- **Why.** This is the per-request twin of `TrustPosture::after_verify` (ADR-0006's per-page rule) and obeys the same invariant: the posture tracks the ACTUAL load path, never the URL string, so a URL that merely LOOKS content-addressed is never labelled verified. No new trust label is invented, and the debug JSON uses the SAME lower-kebab wire names (`content-verified` / `unverified-origin` / `name-via-trusted-rpc` / `mutable-name`) the chrome trust indicator uses.
- `NameViaTrustedRpc` / `MutableName` are properties of the PAGE's name resolution, not of an individual subresource request, so the helper never derives them: a capture point sets them explicitly (`NetworkEntry::with_trust`) for the main-document entry from the load's own posture.

## Decision 5: no `docs/platform-capability-matrix.toml` row yet

No capability row was added for the in-app debug menu.

- **Why.** The matrix tracks user-facing capabilities that could silently ship on ONE platform (ADR-0005). This task ships NO user-facing capability on any platform: there are no capture points and no debug view yet, and the core surface it does add is available identically on all three edges (desktop reads the shell directly, both mobile cores expose the same accessor + clear over their FFI). A row here would be three `stubbed` cells describing a capability that exists nowhere.
- **Who adds it.** The row belongs with the first task that makes the capability reachable by a user (`general-browser-menu-with-version-and-debug-entry` + the debug-view tasks), where the per-platform asymmetry the guard exists to catch becomes real (notably iOS's honestly partial network coverage).

## Decision 6: timestamps are caller-supplied milliseconds, not read from a clock

`ConsoleEntry::timestamp` / `NetworkEntry::timestamp` (and `duration`) are plain `u64` milliseconds the capture point supplies, defaulting to `0` for "unknown".

- **Why.** The core binds no clock/time source for this (the platform event already carries or can cheaply produce a timestamp), and a caller-supplied value keeps the store deterministic and unit-testable with no time mocking. It also keeps the module free of a new dependency.
- An unknown numeric field (`line`, `status`, `size`, `duration`) serializes as JSON `null`, never a fabricated `0`, so the debug view can show "unknown" honestly.

## Decision 7: the store is a shared `Arc<Mutex<_>>` handle, like `RedirectSink`

`DebugCapture` is a cheap clone-shares-one-store handle whose methods all take `&self`.

- **Why.** The capture points run where the platform event runs, which is OFF the UI thread on Android (`shouldInterceptRequest`) and on the desktop scheme-handler worker (`docs/adr/0008`), and the seam's handler closures are `Send`. A `&mut` store on the shell could not be reached from them. This is the exact idiom `crates/werust-core/src/ipfs.rs`'s `RedirectSink` already uses, so the edge wiring (`BrowserShell::with_debug_capture`, mirroring `with_redirect_sink`) is the shape the edges already know.
- A poisoned lock degrades to a no-op push / an empty read (and reports the capture-enabled DEFAULT rather than claiming capture is off), never a panic: a debug surface must not be able to crash the browser.
