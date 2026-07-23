# Android ANR diagnosis: the UI thread is blocked by SYNCHRONOUS ENS/IPNS resolution inside `navigate`

Durable diagnosis + fix record for task `android-anr-main-thread-diagnose-and-unblock` (v0.2.3 field finding B, root note `work/notes/observations/field-test-v0.2.3-back-nav-anr-urlbar-noprotocol-2026-07-23.md`).

## The finding (verbatim)

> "I got the 'isn't responding' android modal regularly, and interesting pressing 'wait' I could still type in the url bar, do anything there, but the modal would keep popping up."

Signature: the URL bar stays typeable (the WebView surface repaints, input is accepted), yet Android's ANR watchdog trips REGULARLY. That is the classic profile of the MAIN (UI) thread being **repeatedly blocked for multi-second stretches**, not a hard total freeze.

## Root cause (confirmed IN CODE, not assumed)

The Android edge drives the shared `werust-core` `BrowserShell` on the **UI thread**, and `BrowserShell::navigate` performs the ENS/IPNS resolution **synchronously, inline, with blocking network I/O**:

- `BrowserActivity.kt` calls `core.navigate(...)` directly from the UI thread — in the URL bar's `IME_ACTION_GO` listener (`core.navigate(text.toString()); afterCoreAction()`), from the Back/Forward/Reload button click listeners (`core.goBack()/goForward()/reload()`), and once at launch (`core.navigate(START_URL)` in `onCreate`). Every one of these runs on the Android main (UI) thread.
- `WerustCore.navigate` -> JNI `nativeNavigate` -> `SyncSession::navigate` -> `CoreSession::navigate` -> `BrowserShell::navigate` (`crates/werust-core/src/lib.rs`).
- For a bare `.eth` entry, `BrowserShell::navigate` calls `navigate_ens_name`, which calls `crate::ens::resolve(self.provider.as_ref(), name)` **synchronously on the calling thread**. `ens::resolve` makes **two sequential blocking `eth_call` HTTP round-trips** (`registry.resolver(node)` then `resolver.contenthash(node)`), each over the `RpcProvider`'s `ureq` transport whose default whole-request timeout is **30s** (`DEFAULT_GLOBAL_TIMEOUT` in `crates/werust-core/src/ethereum.rs`). An IPNS name adds a THIRD blocking call (`resolve_ipns_name` fetches + verifies the signed record).
- `CoreSession::new` builds the shell with `BrowserShell::new`, which uses the real `RpcProvider::new()` (real network), so this is the production path on device, not a test fixture.

So a single `.eth` navigation can block the UI thread for **up to ~30-60s** (two eth_calls, worse on a slow/flaky mobile network) before it ever returns. Android's ANR watchdog fires at ~5s of an unresponsive main thread -> the modal. The user dismisses it with "wait", types the next thing (the input queue is still serviced between blocks, which is why the bar stays typeable), and the **next** `.eth` load blocks the UI thread again -> the modal RECURS. That is exactly the reported "regularly, keeps popping, but I can still type" signature.

### Why the prior off-thread task did not fix this

`ipfs-retrieval-off-main-thread-no-ui-freeze` (done) moved the `ipfs://` **content RETRIEVAL** (CAR fetch + per-block verify + DAG reassembly) off the handler thread. On Android that retrieval already runs on the WebView WORKER thread (`shouldInterceptRequest`), NOT the UI thread. That task did NOT touch the ENS/IPNS **resolution** step, which happens earlier, inside `navigate`, on the UI thread. So the culprit here is a DIFFERENT main-thread hop than the one that task addressed - matching the task's read-first note ("something ELSE is on the Android main thread").

### Suspects RULED OUT (with evidence)

- **A too-tight pump/refresh loop (Handler/Choreographer/timer).** RULED OUT. `BrowserActivity.kt` has NO `Handler`, `Choreographer`, `postDelayed`, timer, or frame loop. It is entirely event-driven: `afterCoreAction()` (which reads the chrome JSON once) runs ONLY on a user action or a WebView lifecycle callback. `pump()` is called once per `onPageStarted`/`onPageFinished`/`onReceivedError` signal, never in a loop. The core `pump()` already returns `true`-on-change and the Android side already repaints only on those discrete signals, so there is no busy repaint loop to throttle. (The task listed this as a prime suspect "possibly tightened by v0.2.3 LoadStep polling"; there is no polling on Android - the LoadStep is read inside the same once-per-signal chrome refresh.)
- **A synchronous main-thread FFI hop in the `ipfs://` scheme interception.** RULED OUT as the ANR cause. `shouldInterceptRequest` runs on a WebView WORKER thread (documented in `BrowserActivity.kt`'s KDoc and guarded by `SyncSession`), not the UI thread. It CAN contend on the `SyncSession` mutex against a UI-thread `navigate`, but the block originates from the UI-thread `navigate` holding the lock during its blocking resolve - i.e. the same root cause, not an independent one.

## The fix (threading only; trust/lifecycle/verification UNCHANGED)

Move the blocking session-driving actions OFF the UI thread on the Android edge, then post the cheap UI updates back to the main thread:

- `BrowserActivity.kt` gains a single-thread background executor. `navigate` / `goBack` / `goForward` / `reload` now run on that executor (so the blocking ENS/IPNS resolve inside `CoreSession::navigate` never runs on the UI thread), and when the core action returns, `syncPendingLoad()` + `refreshChrome()` are posted back to the UI thread (`WebView.loadUrl` and widget mutation MUST be on the UI thread).
- The WebView lifecycle callbacks (`onPageStarted`/`onPageFinished`/`onReceivedError`) and the provider/ipfs interception are UNCHANGED: they are already off the UI thread or cheap, and the `SyncSession` mutex still serialises every native call, so no new data race is introduced. Because the long `navigate` now runs on the background executor instead of the UI thread, the worker-thread interception no longer contends with a UI-thread lock-holder that is blocked on network.
- Nothing about trust posture, load lifecycle, or `ipfs://`/ENS verification changes: the SAME `CoreSession` methods run in the SAME order and return the SAME chrome; only the THREAD they run on changes, plus a main-thread post for the WebView/widget writes.

The core `navigate`/resolve stays synchronous (per ADR-0004: no async runtime in the core); the concurrency boundary is at the OS edge, exactly as `ipfs-retrieval-off-main-thread-no-ui-freeze` put the retrieval boundary at the scheme-handler edge. This is the resolution-side twin of that content-side fix.

## The automatable guard (rides `cargo test`, network-isolated)

ANR is a device/emulator runtime property; the fix itself lives in Kotlin, which the `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`) does NOT run (there is no JVM/Robolectric test setup, and `dorfl.json`'s `verify` is a pure workspace `cargo test`). So the strongest guard that RIDES the gate is a Rust test at the `SyncSession` boundary the Kotlin fix depends on:

- `the_sync_session_is_safe_to_drive_from_a_background_thread` (in `crates/werust-android/rust/src/lib.rs`): drives `navigate` (the blocking action the fix moves off the UI thread) from a BACKGROUND thread while the UI thread only reads `chrome_json` / applies pending loads and the WebView worker thread resolves `ipfs://`. This pins the property the off-UI-thread dispatch relies on: the session stays coherent and never panics when the long action runs on a thread OTHER than the UI thread. If a future change reintroduced UI-thread-only assumptions into the session, this reds the gate.

The guard cannot assert Android's actual UI-thread scheduling (that is Kotlin/runtime-only); the manual device verification below covers that.

## Manual device/emulator verification (device-only, from the DIAGNOSIS)

The ANR watchdog is a device property, so verify on a device/emulator after the fix:

1. Build + install the debug APK (`./gradlew :app:installDebug` under `crates/werust-android`, with `ANDROID_HOME`/NDK set).
2. On a NORMAL or deliberately SLOW/flaky network, load several `.eth` names in a row (e.g. `ronan.eth`, `jolly-roger.eth`), and reload each. Before the fix this reproduced the recurring "isn't responding" modal; after the fix it must NOT appear.
3. While a slow `.eth`/`ipfs://` load is in flight, confirm the URL bar stays typeable AND no ANR modal fires (the main thread idles between frames instead of blocking on the resolve).
4. Optional (evidence): capture a main-thread trace during a load (Android Studio profiler / `am profile` / Perfetto). Before: the main thread sits in the JNI `nativeNavigate` frame for seconds during the eth_call. After: `nativeNavigate` runs on the background executor thread; the main thread only does the short `refreshChrome`/`loadUrl` post.

## Files

- `crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt` - the off-UI-thread dispatch + main-thread post (the fix).
- `crates/werust-android/rust/src/lib.rs` - the `SyncSession` background-thread-safety guard test.
