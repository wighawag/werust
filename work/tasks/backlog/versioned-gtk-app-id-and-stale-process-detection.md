---
title: "Version the GTK application_id so a new werust binary opens its own window with its own compiled code instead of silently reusing an old process — the old binary may have stale behavior (wrong RPC endpoint, old compile-time constants)"
slug: versioned-gtk-app-id-and-stale-process-detection
spec: in-app-debug-menu-console-and-network
blockedBy: []
covers: []
---

## What to build

FIELD FINDING (v0.2.9): GTK `Application` with a fixed `application_id` enables single-instance D-Bus activation: launching a new binary activates the existing instance and hands off, even when the versions differ. The consequence is **worse than a wrong version label**: the old process has its OLD compiled-in behavior, including a different RPC endpoint configured at build time. The user ran v0.2.8 (with `1rpc.io/eth`, which blocks `eth_call`), then launched v0.2.9 (with Infura, which works) — the console printed "werust 0.2.9", GTK forwarded to the running 0.2.8 process, and every `.eth` site failed with the 1rpc "Not allowed" error. The version number in the menu was just the symptom that helped find the real problem: **stale compiled behavior serving the new session**.

Fix: version the `application_id` so different werust releases never share a process window. A new binary always starts its own process with its OWN compiled code (RPC endpoint, version string, feature flags — every compile-time constant), not the old process's.

**Mechanism (prescribed):**

- The current `APP_ID` is `com.github.wighawag.werust` at `crates/werust/src/main.rs:42`. Change it to include the MARKETING VERSION (the same `werust_core::version()` the menu already uses): `com.github.wighawag.werust.v0_2_9` (use underscores for dots since D-Bus bus names conventionally avoid dots that are not part of the well-known name hierarchy; dots are valid in the GTK app ID check but underscores are safer). The version is available at compile time via `werust_core::version()` which is `env!("WERUST_VERSION")`.

- The test at line 1975 (`application_id("com.github.wighawag.werust.test")`) is fine as-is.

- This means EVERY release gets its own application_id, so launching a second copy of the SAME release still reuses the running instance (user-expected single-window behavior), but a DIFFERENT version does not. This is the correct trade-off: the user might want two different versions open to compare, but that is far rarer than the "stale process serves wrong behavior" trap.

- What about leftover processes? Each version is now its own D-Bus service, so killing an old version is explicit (`kill` or `pkill -f werust.v0_2_8`). No auto-cleanup needed — the OS reaps nobody's orphan. Cache dirs for old versions accumulate in `~/.cache/com.github.wighawag.werust.v0_2_*`; acceptable (disk is cheap, caches are small). A future cleanup utility is a separate concern.

**Alternative considered and rejected:** checking the running instance's version via D-Bus property and auto-killing the old instance. This is fragile (D-Bus version property = extra surface, different GC lifetimes, the old process might have unsaved state). Versioning the app ID is simpler and sound: no IPC is needed because the two versions simply cannot address each other.

Where to look: `crates/werust/src/main.rs` (the APP_ID constant and the site that uses it). Make the app_id depend on `werust_core::version()`, replacing dots with underscores in the version portion. Keep the `.test` suffix in the test as-is. No new build script — reuses the existing `WERUST_VERSION` that `werust-core/build.rs` already resolves.

## Acceptance criteria

- [ ] `com.github.wighawag.werust.v0_2_9` on a v0.2.9 release (dots replaced with underscores). The test at line 1975 is unchanged.
- [ ] Launching v0.2.9 while v0.2.8 is running creates a NEW window for v0.2.9 (does not forward to the 0.2.8 process). The new window has v0.2.9's own compiled code (correct RPC endpoint, correct version).
- [ ] Launching v0.2.9 while another v0.2.9 is running reuses the existing v0.2.9 window (intra-version single-instance preserved).
- [ ] The version used in the app_id is the SAME `werust_core::version()` the menu and the console banner use, so no new version source drift.
- [ ] Test: the app_id matches the baked version (a unit test asserts the constructed id, or a shape test).
- [ ] No new IPC, no auto-kill, no cache directory explosion beyond what is acceptable.

## Prompt

> Goal: include the werust version in the GTK application_id so that different releases have separate D-Bus bus names and a new binary never silently reuses an old version's compiled behavior (RPC endpoint, compile-time constants, feature flags). This is the fix for the "stale process trap" where the user launched v0.2.9 with Infura but saw every `.eth` site fail because the running v0.2.8 process (with 1rpc.io/eth) was answering the window.
>
> Where to look: `crates/werust/src/main.rs` line 42 `APP_ID: &str = "com.github.wighawag.werust"` and the site that uses it at line 612. Make the app_id depend on `werust_core::version()` (which is resolved at compile time via `env!("WERUST_VERSION")` in the existing `build.rs`). Replace dots with underscores in the version portion so the app_id is valid GTK/D-Bus syntax. Keep the `.test` suffix in the test line 1975 as-is. No new build script, no auto-kill, no D-Bus property.
