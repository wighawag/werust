---
title: "Gate-3 conductor review: general-browser-menu-with-version-and-debug-entry (APPROVE)"
date: 2026-07-27
status: open
reviewOf: general-browser-menu-with-version-and-debug-entry
verdict: approve
---

## Verdict: APPROVE

Merged as `bdb2234`, after one Gate-2 block that was CORRECT and one conductor-prescribed fix. 290 `werust-core` tests re-run locally green.

## The Gate-2 block was the single most valuable finding of this drive

Gate-2 blocked the first build with: the menu would ship reading **`werust 0.0.0`**. I verified the claim rather than taking it (`git show v0.2.4:Cargo.toml` and `v0.2.6:Cargo.toml` both carry `version = "0.0.0"`; `.goreleaser.yaml` derives only the ARCHIVE NAME from the tag and never injects a version into the compiled Rust). So the headline feature of the task — SHOW THE VERSION — would have shipped a placeholder, in the release being cut off this very work. That is a defect worth the extra dispatch, and it is the kind a source-shape test cannot catch because every layer was individually correct: the menu really did read `version()`, and `version()` really did read `CARGO_PKG_VERSION`. The lie was in a value nobody owned.

The prescribed fix landed as specified and then some:

- `crates/werust-core/build.rs` resolves ONE version at build time: injected `WERUST_VERSION` -> `git describe --tags --always` -> `CARGO_PKG_VERSION`, never failing the build when git or `.git` is absent.
- The resolution logic lives in `src/version_resolution.rs` and is `include!`d by the build script, so **the same code the build runs is unit-tested** in the pure-Rust gate. A build script cannot itself be `cargo test`ed; this sidesteps that cleanly and was not something I asked for.
- `.github/workflows/release.yml` exports `WERUST_VERSION` from the tag in **all three** Rust-compiling legs (desktop/GoReleaser, android-apk, ios-simulator), with `fetch-depth: 0` added so `git describe` works on the non-tag path.
- The workspace `Cargo.toml` is bumped `0.0.0` -> `0.2.6` with a comment explaining it is the last-resort source.

**Empirically verified, not just read:** `WERUST_VERSION=v0.2.7 cargo build -p werust-core` produces an artifact carrying `0.2.7` — so the injection AND the leading-`v` strip both work end to end. Regression guard: `every_rust_compiling_leg_injects_the_tag_version_into_the_build`.

## Acceptance criteria, ticked against the merged tree

- [x] **A general menu affordance on every platform, structured to grow.** New `crates/werust-core/src/menu.rs` owns the menu MODEL (items listed by core); each edge renders whatever core lists. Tests: `every_edge_has_a_menu_affordance_opening_a_native_menu_surface`, `every_edge_renders_whatever_items_the_core_lists_so_the_menu_can_grow`, `the_menu_is_a_general_container_not_a_debug_only_menu`. Putting the item list in CORE rather than triplicating it per edge is what actually makes "structured to grow" true: adding an item is one core change, not three.
- [x] **The version comes from ONE source.** `werust_core::version()` -> `env!("WERUST_VERSION")`, reaching the mobile edges over the FFI. Tests: `the_menu_shows_the_werust_version_from_the_one_shared_source`, `no_edge_hardcodes_a_version_string`, `the_version_is_resolved_at_build_time_and_is_never_empty_or_a_placeholder`.
- [x] **A Debug entry that opens the debug view.** Landed as the recommended open-debug-view HOOK for the two debug-view tasks to fill. Test: `the_menu_has_a_debug_entry_that_is_activatable`.
- [x] **User-facing, always available, not debug-build-gated.** Test: `the_menu_is_never_debug_build_gated_on_any_edge`.
- [x] **Desktop + Android + iOS, parity-tracked**, with a new `browser-menu` capability row and recorded manual steps in the spike README.

## Nit triage (6 non-blocking findings)

- **Nit 1 is worth correcting and I am flagging it loudly**: four doc sites (`platform-capability-matrix.toml:413`, `werust_mobile.h:171`, `WerustCore.swift:219`, `WerustCore.kt:139`) STILL describe `version()` as the Rust workspace version / `CARGO_PKG_VERSION` — which is precisely the claim the requeue existed to kill. The code is right and the docs now lie in the opposite direction. Cheap to fix on the next touch of those files; no behaviour at risk.
- **Nit 3 matters for the release I am about to cut**: nothing ties the workspace `Cargo.toml` version to the newest tag, so at `v0.2.7` the last-resort path (a no-git source tarball) would report `0.2.6`. The released BINARIES are unaffected (CI injects the tag), so this is not release-blocking, but I am bumping the workspace version to `0.2.7` as part of cutting the tag, exactly as the in-code comment instructs ("Bump it with the release it names"). A guard test tying the two together is a reasonable follow-up.
- **Nit 6 is the one for the human, and I am carrying it into the report**: the Kotlin and Swift menu code is NEVER compiled by the pure-Rust `verify` gate. The `browser-menu` parity row reads "implemented on all three" on the strength of a source-SHAPE guard, and a release is being cut off it immediately. The shape guards are good (they are what caught hardcoded versions and debug-gating), but they cannot catch a Kotlin compile error or a menu that renders wrong. **This needs one manual pass on a real Android/iOS build.**
- **Nits 2 and 4 are ratifications**, both benign: `fetch-depth: 0` on the two mobile checkout steps (a small clone-cost increase, needed for `git describe`), and a dry-run cache path that can carry a stale `describe` string (the TAGGED path, the only one that must be exact, builds fresh with injection).
- **Nit 5 is a code/comment disagreement**: the Kotlin comment claims an unknown menu id "fails visibly" while `else -> false` does nothing visible. Reword on next touch.

## Coherence

The menu model in core with per-edge rendering is the same shape the rest of this codebase already uses (one shared fact in core, edges render it), and it is what makes the two debug-view tasks cheap: they fill a hook rather than each inventing a menu. No concept was forked.
