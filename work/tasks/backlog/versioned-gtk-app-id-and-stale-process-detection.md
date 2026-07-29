---
title: "Version the GTK application_id so a new werust binary does not silently reuse an old version's process window (the version-stuck bug)"
slug: versioned-gtk-app-id-and-stale-process-detection
spec: in-app-debug-menu-console-and-network
blockedBy: []
covers: []
---

## What to build

FIELD FINDING (v0.2.9): GTK `Application` with a fixed `application_id` enables single-instance D-Bus activation: launching a new binary screams "activate the existing instance" and hands off, even when the versions differ. The user ran v0.2.8, then downloaded and launched v0.2.9 — the console printed "werust 0.2.9", GTK forwarded the activation to the running 0.2.8 process, and the window showed 0.2.8's menu with the stale 1rpc.io/eth endpoint. Same for v0.2.7: same stale 0.2.8 window. The user spent time debugging a "version mismatch bug" that was actually just a stale process.

Fix: version the `application_id` so different werust releases never share a process window.

**Mechanism (prescribed):**

- The current `APP_ID` is `com.github.wighawag.werust` at `crates/werust/src/main.rs:42`. Change it to include the MARKETING VERSION (the same `werust_core::version()` the menu already uses): `com.github.wighawag.werust.v0_2_9` (DOTS are not recommended in D-Bus bus names; dots are valid in the GTK app ID check but underscores are safer and more conventional for D-Bus). `build.rs` of `crates/werust` (not `werust-core`) emits `APPLICATION_ID` at compile time using the already-existing `WERUST_VERSION` (or the same fallback chain the core uses).

- The test at line 1975 (`application_id("com.github.wighawag.werust.test")`) is fine as-is — the `.test` suffix distinguishes it.

- This means EVERY release gets its own application_id, so launching a second copy of the SAME release still reuses the running instance (the user-expected single-window behavior), but a DIFFERENT version does not. This is the correct trade-off: the user might have two different versions open to compare, but that is far rarer than the "version stuck" trap.

- What about leftover processes? Each version is now its own D-Bus service, so killing an old version is explicit (`kill` or `pkill -f werust_v0_2_8`). No auto-cleanup needed — the OS reaps nobody's orphan. If a user runs many versions successively, ~/.cache/com.github.wighawag.werust.v0_2_* accumulates stale cache dirs. Acceptable (disk is cheap, caches are small). A future cleanup-garbage-versions utility is a separate concern.

**Alternative considered and rejected:** checking the running instance's version via D-Bus property and auto-killing the old instance. This is fragile (D-Bus version property = extra surface, different GC lifetimes, the old process might have unsaved state). Versioning the app ID is simpler and sound: no IPC is needed because the two versions simply cannot address each other.

Where to look: `crates/werust/src/main.rs` (the APP_ID constant and the build function that constructs it from the resolved version; add a `build.rs` to `crates/werust` or reuse the `WERUST_VERSION` build.rs that already exists — prefer reusing the existing build.rs in `crates/werust-core` by making the `APPLICATION_ID` const depend on `werust_core::version()` rather than adding a second build script). Since `version()` is `env!("WERUST_VERSION")` that is `"0.2.9"` in a release build, the version string is available at compile time in the `werust` crate via `werust_core::version()`. The app_id would be `format!("com.github.wighawag.werust.v{}", version().replace('.', "_"))`.

## Acceptance criteria

- [ ] `com.github.wighawag.werust.v0_2_9` on a v0.2.9 release (or dev build) — the test at line 1975 is unchanged.
- [ ] Launching v0.2.9 while v0.2.8 is running creates a NEW window for v0.2.9 (does not forward to the 0.2.8 process). Launching v0.2.9 while another v0.2.9 is running reuses the existing v0.2.9 window.
- [ ] The version used in the app_id is the SAME `werust_core::version()` the menu and the console banner use, so no new version source drift.
- [ ] Test: the app_id matches the baked version (a unit test asserts the constructed id, or a shape test).
- [ ] No new IPC, no auto-kill, no cache directory explosion beyond what is acceptable.

## Prompt

> Goal: include the werust version in the GTK application_id so that different releases have separate D-Bus bus names and a new binary never silently reuses an old version's process window. This is the fix for the "version stuck" bug where the user launched v0.2.7 and v0.2.9 but always saw v0.2.8's menu because the old 0.2.8 process was still running.
>
> Where to look: `crates/werust/src/main.rs` line 42 `APP_ID: &str = "com.github.wighawag.werust"` and the site that uses it at line 612. Make the app_id depend on `werust_core::version()` (which is resolved at compile time via `env!("WERUST_VERSION")` in the existing `build.rs`). Replace dots with underscores in the version portion so the app_id is valid GTK/D-Bus syntax. Keep the `.test` suffix in the test line 1975 as-is. No new build script, no auto-kill, no D-Bus property.
