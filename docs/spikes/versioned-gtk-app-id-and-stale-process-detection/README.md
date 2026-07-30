# Versioned GTK application id: the measurement

Backing evidence for the task `versioned-gtk-app-id-and-stale-process-detection`: werust's GTK application id now carries the version (`com.github.wighawag.werust.v0_2_9`), so a newly launched release can never be handed off to a still-running OLDER release that would answer with its own compiled-in behaviour (its RPC endpoint, its constants, its feature flags).

The id itself is built by `app_id()` in `crates/werust/src/main.rs` and pinned by the unit tests next to it (exact id, per-version distinctness, and GLib validity for every shape the build-time-resolved version can take). What those tests cannot show is the D-Bus consequence, which is what this directory measures.

## The measurement

`app-id-uniqueness.py` stands up a headless `Gio.Application` for the ALREADY-RUNNING release, then probes from separate processes whether a launch of the same id and of a different id becomes the primary instance (its own process, its own window) or a remote one (handed to the running instance). Needs `python3-gi` and a session bus; no display, no GTK, no window.

Run on 2026-07-30, GLib/GTK from the repo's normal desktop toolchain:

```
$ ./app-id-uniqueness.py com.github.wighawag.werust com.github.wighawag.werust   # BEFORE
same version  com.github.wighawag.werust             -> remote
new version   com.github.wighawag.werust             -> remote

$ ./app-id-uniqueness.py com.github.wighawag.werust.v0_2_8 com.github.wighawag.werust.v0_2_9   # AFTER
same version  com.github.wighawag.werust.v0_2_8      -> remote
new version   com.github.wighawag.werust.v0_2_9      -> primary
```

Reading it:

- BEFORE (one unversioned id for every release): the newly launched release is `remote`, so the running OLD process takes the session. That is exactly the field failure, where v0.2.9 printed its banner and the running v0.2.8 (on `1rpc.io/eth`) served every `.eth` navigation.
- AFTER: `new version -> primary`, so v0.2.9 starts its own process with its own compiled code, while `same version -> remote` keeps intra-version single-instance behaviour (a second copy of the SAME release still raises the running window).

An incidental confirmation from the same session: registering a second instance of the same id makes a D-Bus call TO the primary (a holder that never runs a main loop fails the probe with `org.freedesktop.DBus.Error.NoReply`), while a different id never contacts it at all. The two versions genuinely cannot address each other, which is why no version handshake or auto-kill is needed.

## Confirming with the real binaries

Two dev builds, each with its own baked version (`WERUST_VERSION` is what the release path injects; build outside the repo so the scratch target tree is not swept into a commit):

```
WERUST_VERSION=0.2.8 CARGO_TARGET_DIR=/tmp/werust-appid cargo build -p werust && cp /tmp/werust-appid/debug/werust /tmp/werust-0.2.8
WERUST_VERSION=0.2.9 CARGO_TARGET_DIR=/tmp/werust-appid cargo build -p werust && cp /tmp/werust-appid/debug/werust /tmp/werust-0.2.9
/tmp/werust-0.2.8 & sleep 3; /tmp/werust-0.2.9
```

Expected: TWO windows, and `busctl --user list | grep wighawag.werust` shows both `…werust.v0_2_8` and `…werust.v0_2_9`. Launching `/tmp/werust-0.2.9` a second time raises the existing v0.2.9 window instead of opening a third.

Only the headless measurement above was run automatically (opening browser windows on the operator's desktop is not something an autonomous build should do); the two binaries were built and confirmed to bake distinct versions:

```
$ env -u DISPLAY -u WAYLAND_DISPLAY /tmp/werust-0.2.8   # and …-0.2.9
werust 0.2.8 — a Rust web browser (webview backend)
werust 0.2.9 — a Rust web browser (webview backend)
```

Since `app_id()` is a pure function of that same `werust_core::version()`, distinct baked versions mean distinct bus names.

## Decisions

- **Non-`[A-Za-z0-9-]` characters in the version are folded to `_`, not just dots.** The task prescribed replacing dots. But the version is not always a release triple (a dev build is `git describe` output, and an operator can inject an arbitrary `WERUST_VERSION`), and an invalid application id fails QUIETLY (GLib rejects it and the application ends up with no id, hence no uniqueness at all). Folding every disallowed character makes the id valid by construction. Alternative considered: replace dots only and let an odd version produce an invalid id; rejected because the failure mode is silent and is precisely the uniqueness this task is buying. Touches nothing else: `app_id()` has one call site, and the version SOURCE is unchanged (still `werust_core::version()`).
- **The version element is prefixed with `v` (`…werust.v0_2_9`).** Required, not cosmetic: a D-Bus bus-name element may not begin with a digit, so `…werust.0_2_9` is invalid (asserted in the test). This matches the id the task specified.
