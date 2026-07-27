---
title: "Gate-3 conductor review: debug-capture-store-console-and-network-in-core (APPROVE)"
date: 2026-07-27
status: open
reviewOf: debug-capture-store-console-and-network-in-core
verdict: approve
---

## Verdict: APPROVE

Merged as `14b4a6d`, first dispatch, no recovery needed. Gate-1 and Gate-2 both green. Verified against what landed on `origin/main`; 270 `werust-core` tests re-run locally green.

## Acceptance criteria, ticked against the merged tree

- [x] **`ConsoleEntry` + `NetworkEntry` types and a BOUNDED store owned by the shell.** New `crates/werust-core/src/debug.rs` (817 lines) with `DebugCapture` over two `VecDeque` ring buffers; `push_console` / `push_network` / `clear` mutate it. Tests: `pushing_past_the_cap_evicts_the_oldest_console_entry`, `pushing_past_the_cap_evicts_the_oldest_network_entry`, `clear_empties_both_ring_buffers`.
- [x] **The store is BOUNDED, oldest-evicted.** `MAX_CONSOLE_ENTRIES = 300` / `MAX_NETWORK_ENTRIES = 300`, `pop_front` on overflow. It also went FURTHER than asked with a second axis of boundedness: `MAX_TEXT_CHARS = 2_000` per text field, so one pathological `console.log` of a whole document cannot blow the store even while the entry COUNT stays in range. That is the right instinct (a count bound alone is not a memory bound). Test: `an_oversized_message_and_url_are_truncated_so_one_entry_cannot_grow_unboundedly`.
- [x] **`NetworkEntry` carries an HONEST per-request `TrustPosture`, not a re-meaning.** It reuses `renderer::TrustPosture` directly (the ADR-0006 type, not a parallel enum) and `trust_posture_wire_name` emits the SAME four wire names the chrome JSON already uses (`unverified-origin`, `content-verified`, `name-via-trusted-rpc`, `mutable-name`) — I diffed these against `werust-android/rust/src/ffi_json.rs` and `werust-ios/rust/src/ffi_json.rs` and they match exactly, all four variants covered. It defaults to `UnverifiedOrigin` (fail-closed), so an entry nobody set cannot imply trust it does not have. There is a test pinning precisely this: `the_json_carries_the_same_trust_wire_names_the_chrome_json_uses`. **No new trust label was invented.**
- [x] **Reaches the edges over the shared FFI surface, additively.** A `debug` document plus `clear` exports on both mobile FFIs and the `werust_mobile.h` header; existing chrome readers are untouched (the choice of a dedicated debug document over widening the chrome JSON is recorded in DECISIONS.md Decision 1, which keeps the hot chrome JSON lean — the right call, since the chrome JSON is read every pump tick and the debug document only when the view is open).
- [x] **A capture-enabled flag exists, default true.** `network_capture_enabled` with a getter/setter, so the Phase-2 `debug-network-capture-toggle-config` task is a small addition rather than a rework. Test: `network_capture_is_enabled_by_default_and_the_flag_gates_the_push`. Note the deliberate scoping recorded in the code: the flag gates NETWORK capture only, not console, because re-meaning it as an everything-switch would silently change what the later setting does. That is a correct coherence call.
- [x] **Core-only, fully unit-tested, network-isolated.** 15 `debug::` tests covering eviction, clear, the flag, truncation, JSON round-trip, scheme derivation, and posture defaults. (See the scope caveat below on the Android push wrappers.)

## Coherence

The one thing I most wanted to catch here — a second trust vocabulary minted for the Network tab — did NOT happen, and the build defended against it explicitly rather than accidentally. `request_trust_posture` also refuses to hand out a verified posture unless the caller can state the request really verified.

## Nit triage (the 5 non-blocking Gate-2 findings)

Full text in `review-nits-debug-capture-store-console-and-network-in-core-2026-07-27.md`. Three of these are load-bearing for the NEXT task in this drive and I have planted a FORWARD-NOTE in `debug-console-network-capture-per-platform` rather than leaving them to be rediscovered:

- **Nit 1 is the important one and I am treating it as binding on the capture task.** The Android push wrappers route through the WHOLE session lock (`SyncSession::push_*` -> `self.with(...)`), which re-couples capture to the same lock a multi-second `resolve_ipfs` holds on a worker thread. `onConsoleMessage` runs on the Android UI thread, so a page logging in a loop would take the global session lock ON THE UI THREAD while a CAR retrieval holds it — that is precisely the ANR shape user story 4 exists to prevent. The store itself is an `Arc<Mutex<_>>` specifically so a capture point needs no `&mut` shell, so the fix is available and cheap: expose a CLONED `DebugCapture` handle and push off the session lock. Forward-noted as a REQUIREMENT on the capture task.
- **Nit 3 (all-`pub` entry fields let a capture point assign a huge string directly, bypassing `MAX_TEXT_CHARS`)** matters exactly when the capture points start constructing entries — i.e. the next task. Forward-noted: construct via `new()`/`with_*`, never by field assignment.
- **Nit 4 (per-request posture vs ADR-0006's two-axis loudest-warning rule)** is a genuine coherence question deferred into the capture task by Decision 4: on an ENS page the Network tab could show `content-verified` rows while the chrome indicator shows `name-via-trusted-rpc`. Forward-noted so the capture task sets the main-document entry's posture from the load's own posture.
- **Nit 2 (the Android push plumbing pre-empts the capture task and is unrecorded)** is a scope observation, not a defect: the surface is additive and harmless, it just belongs to the next task's story. No action beyond ratification.
- **Nit 5 (poisoned-lock inconsistency: `push_network` drops the entry while `network_capture_enabled()` reports the default `true`)** is real but vanishingly unlikely (no panic-prone code in the critical sections). Recorded, not actioned.

## For the human

Two things want a nod, neither blocking: the cap values (300 entries / 2000 chars — generous for a phone, cheap for desktop) and the decision to keep the debug payload OFF the hot chrome JSON as a separate document.
