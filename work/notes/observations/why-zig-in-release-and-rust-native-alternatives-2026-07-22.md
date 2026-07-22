---
title: Why Zig is in the release desktop leg, and the rust-native cross-compile alternatives
date: 2026-07-22
kind: observation
tags: [release, ci, cross-compile, zig, cargo-zigbuild, adr-candidate]
---

## Why Zig is here at all

The release desktop leg uses GoReleaser's `builder: rust`, which cross-compiles with
`cargo-zigbuild`, which uses `zig cc` as the CROSS-LINKER. Zig here is ONLY a
cross-linker for building the `aarch64-unknown-linux-gnu` desktop binary from an
`x86_64` runner. It is NOT the Zig language, NOT the wezig Zig renderer arm, and NOT a
contradiction of ADR-0002's "Zig-less build path" (which means Zig-less for the
LANGUAGE/renderer, not the linker). Removing the `aarch64` desktop target would remove
Zig from the pipeline entirely. The Android APK + iOS `.app` jobs never touch Zig (NDK /
Xcode toolchains).

## Rust-native alternatives (the human asked "was there no rust-native alternative?")

Yes, but each has a catch BECAUSE the desktop binary links SYSTEM WebKitGTK/GTK:

1. **Native GNU cross-linker** (`gcc-aarch64-linux-gnu` + linker env). The classic
   no-Zig path, BUT needs an arm64 SYSROOT containing all GTK/WebKit/glib dev libs for
   pkg-config to resolve. Assembling/maintaining that sysroot on CI is the real chore —
   exactly what cargo-zigbuild papers over (Zig ships its own multi-arch libc/sysroot).
   So it trades a Zig dep for a sysroot chore.
2. **Native arm64 runner** (`ubuntu-24.04-arm`, now a hosted GitHub runner). Build the
   arm64 binary NATIVELY: no cross-linker, `apt-get install libwebkitgtk-6.0-dev` works
   exactly like the x86_64 job. This is the CLEANEST no-Zig answer for a system-lib
   binary. Cost: a per-arch build matrix and likely dropping GoReleaser's single-shot
   `builder: rust` cross-compile in favour of native `cargo build` per runner, then
   letting GoReleaser assemble/publish. A workflow restructure.
3. **`cross` (Docker)**. Same arm64-GTK sysroot problem as (1), just containerised.
4. **cargo-zigbuild (current)**. Bundles the cross sysroot, so it "just works" for the
   GNU target without assembling arm64 GTK libs. Popular precisely for this reason; it
   is why GoReleaser's rust builder chose it.

## Decision + recommendation

KEEPING cargo-zigbuild is fine and idiomatic (the human chose to keep it). The Zig is a
linker implementation detail, and the one failure it caused (`version: latest` resolving
to the unreleased Zig 0.16.0, 404 on mirrors) is fixed by pinning a real version.

If a truly Zig-FREE desktop release is later wanted, the best fit for werust (a
system-WebKitGTK-linking binary) is **native arm64 runners** (option 2), NOT the GNU
cross-linker — captured here as an ADR-level option (it restructures the desktop leg),
not a quick task.
