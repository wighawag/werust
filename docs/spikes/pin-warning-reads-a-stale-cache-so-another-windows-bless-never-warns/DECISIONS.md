# Decisions: refreshing the pin cache at navigation, and retiring the `cfg!(test)` default (`pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`)

The READ side of the pin-store defect its parent (`pin-store-read-modify-write-and-test-isolation`) closed on the write side. Three choices were open enough to be worth recording: each says what was chosen, why, what was rejected and what it touches, so a reviewer can ratify or reverse it.

Task: `work/tasks/*/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns.md`. Parent decisions this one revisits: `docs/spikes/pin-store-read-modify-write-and-test-isolation/DECISIONS.md`.

## 1. The cache is re-read at EVERY navigation the shell drives, and nowhere else

**Chosen.** `BrowserShell::refresh_pin_store_cache` re-reads `pins.json` into `self.pins` from four call sites, all of them navigation entry points: the ENS front door (`navigate_ens_name`, which is also where a reload of an ENS page re-resolves), the plain branch of `navigate`, the plain branch of `reload`, and `enter_history_entry` (Back / Forward / the iOS swipe). The rule is statable in one line — every top-level navigation re-reads, nothing in the paint path does — and the doc comment on the method is the primary home of the reasoning.

**Why.** A navigation happens once per user action, is already I/O-bearing (a name resolution plus a page load), and is exactly the moment the previous entry's answer stops applying. The task prescribed the front door; the other three sites cost one line each and remove a question a reviewer would otherwise have to ask, because the TOFU axis is derived for any entry that matches a known ENS site by ROOT-CID PREFIX — so a plain `ipfs://<rootcid>/…` navigation and a history move onto one both produce the axis, and would otherwise answer from a snapshot taken at launch.

**Rejected.** Re-reading in `refresh_chrome` (the paint path): a file read on every chrome refresh, several per load, and the reason `ChromeState` carries a plain `MutableNameTrust` value at all is that no presentation rule may touch the filesystem. A modification-time check before the read (the task allowed it if argued equally correct): it is a second syscall to avoid a ~200-byte read that already happens at most once per navigation, and `mtime` granularity is a correctness hazard for two writes inside one clock tick — the cheaper trigger is not cheaper.

**Touches.** Nothing outside `werust-core`, and no public surface. An entry the front door REFUSES (an invalid URL-bar entry) is deliberately not a navigation and does not re-read.

## 2. `TrustedNamePins::load()` is kept and DELEGATED to, with an `Option` return

**Chosen.** `load()` had zero callers because `PinStoreLocation::Settings` re-implemented `settings_dir().map(load_from)` to distinguish "no directory" from "empty". Rather than delete it, its signature became `pub fn load() -> Option<Self>` (`None` = no settings directory on this system) and the `Settings` arm now delegates to it. One way to load the store, as the acceptance criterion requires.

**Why.** Deleting it would have moved the knowledge that the user's store is `pins.json` under the settings directory OUT of the `pins` module and into `lib.rs`'s private enum, leaving `pins` with a `save()` whose read twin lived somewhere else. Delegating keeps that knowledge in ONE module and keeps the pair symmetric: `save() -> bool` already reports the no-directory case, and `load() -> Option<Self>` is its mirror. It also matters for what lands next: spec `trust-store-hardening`'s head task (`trust-store-fails-closed-instead-of-reading-as-nothing-trusted`) turns an unreadable store into a distinct third state, and it now has ONE read site to teach rather than two.

**Rejected.** Deleting `load()` (the criterion's other permitted option), for the reasons above; keeping the `Self` return and having `Settings` call `settings_dir().is_some().then(load)` (two resolutions of the same directory, and the same duplication under a different spelling).

**Touches.** `TrustedNamePins::load` is `pub` and its return type changed, but it had no callers anywhere in `crates/`, so nothing else moved.

## 3. The pin store is an explicit PRODUCTION opt-in; the `cfg!(test)` default is retired

**Chosen.** `PinStoreLocation::default()` is now `Ephemeral` unconditionally: a shell reads and writes a durable store only when its constructor was told to. Production asks with the new `BrowserShell::with_settings_pins()`, at the five entry points where a real user's session is built (the GTK, macOS and Windows windows; the Android JNI `nativeNew`; the iOS C-ABI `werust_ios_session_new`). Tests ask for a scratch directory with the existing `with_pins_dir`, or ask for nothing. The parent's `if cfg!(test)` branch — this repo's only one — is gone, and `crates/werust-core/tests/pin_store_edge_wiring_shape.rs` now guards both halves: every production entry point asks, and no production code branches on being a test build.

**Why.** The mobile hole exists precisely because `cfg!(test)` is per-CRATE: `werust-android` / `werust-ios` tests build shells through their own production `CoreSession::new()`, which `werust-core`'s branch cannot see. Closing that hole while KEEPING the branch would have meant an explicit ephemeral opt-in at ~45 mobile test call sites, fail-open for every test written afterwards. Inverting the default closes it at 5 sites instead, fail-safe for every crate and every future test, including the desktop edge crates nobody had noticed.

**Rejected, and why the parent's rejection is answered.** The parent considered exactly this shape and rejected it because "forgetting the call at one of the five edges would silently disable TOFU persistence there". That objection is real and is the correct direction to worry about — but it rests on the failure being SILENT, and in this repo it need not be: three of the five entry points are not even compiled by the gate (the JNI module is `cfg(target_os = "android")`, the macOS and Windows windows build only on their own OS), which is the same situation `debug_capture_edge_wiring_shape.rs` and `browser_menu_edge_wiring_shape.rs` already answer with a source-shape guard. With the guard, forgetting the opt-in reds `cargo test` on Linux, so the failure direction the parent feared is now loud, and the one it did not fear (every crate's tests reading the developer's blessed names) is closed by construction rather than by a per-crate branch.

**Touches.**

- Five production entry points, each one line. A NEW edge that builds a `BrowserShell` must call `with_settings_pins()` and add itself to the guard's list; the guard's failure message says so.
- The macOS and Windows **smoke examples** deliberately do NOT opt in, so a CI smoke now runs against no pin store rather than the runner's own. Neither blesses anything, so nothing changes for what they measure.
- Two small public methods per mobile crate (`CoreSession::with_settings_pins`, `CoreSession::has_durable_pin_store`, plus `SyncSession::over` / `SyncSession::has_durable_pin_store` on Android) and one on the shell (`BrowserShell::has_durable_pin_store`). The predicate exists so a crate whose tests reach a shell through a production constructor can ASSERT its hermeticity instead of arguing it — which is what each mobile crate's `a_test_session_reads_no_pin_store_and_never_touches_the_real_pins_json` does.
- The iOS C-ABI tests build their session handle with a local `ffi_test_session()` (`Box::into_raw(Box::new(CoreSession::new()))`) instead of calling `werust_ios_session_new`, because that export is now the production opt-in. What the export itself does beyond the opt-in is one `Box::into_raw`; the opt-in is guarded by the shape test.
- The observation this closes, `work/notes/observations/mobile-core-session-tests-read-the-real-pin-store-2026-07-31.md`, and the parent's decision 2, which this supersedes.

## Not recorded here: the cross-process race

Read-modify-write is still not atomic ACROSS PROCESSES (two windows can interleave a read and a save and lose one pin). That is deliberately NOT written down as a standing limit: `trust-store-serialises-read-modify-write-so-no-bless-is-lost` (spec `trust-store-hardening`) FIXES it with an advisory lock held across the read and the save, and a note saying "this cannot be fixed cheaply" beside the task that fixes it is a contradiction the tree should not carry.
