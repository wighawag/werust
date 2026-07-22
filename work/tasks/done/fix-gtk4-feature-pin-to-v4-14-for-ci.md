---
title: Lower the gtk4 feature pin to v4_14 so CI (Ubuntu 24.04 / GTK 4.14) builds
slug: fix-gtk4-feature-pin-to-v4-14-for-ci
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: []
covers: []
---

## What to build

Fix the remaining CI build failure. After the system-deps fix, `cargo clippy`/`cargo
build` now get past dependency resolution but fail compiling `gdk4-sys` with:

```
The system library `gtk4` required by crate `gdk4-sys` was not found.
> pkg-config --libs --cflags gtk4 'gtk4 >= 4.18'
```

Root cause: `crates/webview-renderer/Cargo.toml` pins GTK-4.18 features
(`gtk4 = { version = "=0.10.0", features = ["v4_18"] }` and
`webkit6 = { version = "=0.5.0", features = ["v2_50", "gtk_v4_18"] }`), which make the
`-sys` crates require `gtk4 >= 4.18` at build time. But `ubuntu-24.04` (the current
`ubuntu-latest` GitHub runner) ships **GTK 4.14.5** (and WebKitGTK **2.52.3**). So the
GTK pin is too high for the runner; a dev laptop with GTK 4.18 builds fine, CI does not.

Note the asymmetry from the runner's apt log: WebKitGTK on Ubuntu 24.04 is 2.52.3 (the
SAME version as the dev machine), so the `webkit6` `v2_50` feature is FINE. ONLY the GTK
feature level is too high.

## What to change

Lower the GTK feature level to `v4_14` (Ubuntu 24.04's GTK, and the widest install
base), keeping the WebKitGTK feature at `v2_50` (2.52.3 supports it):

- `gtk4 = { version = "=0.10.0", features = ["v4_14"] }`  (was `["v4_18"]`)
- `webkit6 = { version = "=0.5.0", features = ["v2_50", "gtk_v4_14"] }`  (was `["v2_50", "gtk_v4_18"]`)

Apply the change everywhere these deps are declared with a GTK feature: check ALL
Cargo.toml that name `gtk4`/`webkit6` (`crates/webview-renderer`, the desktop `werust`
binary if it pins gtk4 features, and the mobile crates do NOT use desktop gtk4 so leave
them). The `werust` binary uses `gtk4` for the window/main-loop — align its feature pin
too if it sets one, or leave it if it inherits none.

The code uses only long-stable GTK4/WebKitGTK APIs (`WebView::builder`, `load_uri`,
`connect_load_changed`/`connect_load_failed`, `evaluate_javascript`, `register_uri_scheme`,
`user_content_manager`, script-message handlers) — none are GTK 4.16/4.18-only, so no
code change should be needed. If clippy/build flags a genuinely 4.18-only API, surface
it (do NOT silently bump back to v4_18); the expectation is none exists.

## Acceptance criteria

- [ ] `gtk4`/`webkit6` feature pins are lowered to the `v4_14` / `gtk_v4_14` level (WebKitGTK stays `v2_50`) in every Cargo.toml that pins them.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` pass locally (the dev machine has GTK 4.18, which is >= 4.14, so a lower pin still builds there).
- [ ] No code change is required (or, if one is, it is because a genuinely 4.18-only API was in use — surface it rather than reverting the pin).
- [ ] The change is minimal and scoped to the feature pins; do not alter the crate VERSIONS (`=0.10.0` / `=0.5.0`) or the desktop/mobile behaviour.

## Prompt

> Goal: make CI's `cargo build` succeed on `ubuntu-latest` (Ubuntu 24.04, GTK 4.14 /
> WebKitGTK 2.52.3). The `webview-renderer` crate pins `gtk4` feature `v4_18` and
> `webkit6` `gtk_v4_18`, forcing a `gtk4 >= 4.18` system requirement the runner cannot
> meet (it has GTK 4.14). Lower the GTK feature to `v4_14` (`gtk4` -> `["v4_14"]`,
> `webkit6` -> `["v2_50", "gtk_v4_14"]`) in every Cargo.toml that pins them; keep the
> WebKitGTK `v2_50` feature (Ubuntu 24.04 has WebKitGTK 2.52.3). The code uses only
> long-stable GTK4/WebKitGTK APIs, so expect NO code change. Do not change crate
> versions. This widens the distros werust builds on and turns CI green.
>
> Done = the GTK feature pins are v4_14, the gate is green locally, and CI's cargo build
> no longer fails on `gtk4 >= 4.18`.
