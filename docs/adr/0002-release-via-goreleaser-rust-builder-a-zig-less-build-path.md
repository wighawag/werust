---
status: accepted
---

# Release + cross-compile via GoReleaser's Rust builder (a deliberately Zig-less build path)

werust cuts releases with **GoReleaser** using its native **`builder: rust`**
(GoReleaser v2.5+, which cross-compiles via `cargo-zigbuild`), mirroring wezig's
GoReleaser-driven pipeline but swapping `builder: zig` for `builder: rust`. A tag
push (`v*`) cuts one GitHub Release carrying the desktop binaries + checksums +
a conventional-commit changelog, with the **mobile artifacts (Android APK, iOS
simulator `.app`) built by hand-written jobs alongside it** — parity with wezig's
`release.yml` is the bar, mobile included.

## Why

Two reasons, and the second is the load-bearing one:

1. **Continuity is the project's thesis.** werust ports wezig's
   language-independent learning and changes only the *language*. Release
   orchestration IS language-independent, so keeping GoReleaser lets wezig's
   already-proven pipeline — crucially its **bespoke mobile jobs** (APK + iOS
   sim `.app`, built from real app modules, not spikes) — port near-verbatim and
   keeps the two arms comparable.
2. **The build toolchain choice is itself part of the experiment.** In wezig,
   Zig does double duty: the renderer language AND the cross-compile toolchain
   (GoReleaser's `builder: zig` runs `zig build`). werust deliberately tests a
   **Zig-less build path** — Rust as the single language, with cross-compilation
   handled by `builder: rust` / cargo-zigbuild — to see whether dropping Zig from
   the toolchain is simpler, given we are *already* committed to interacting with
   Rust for the renderer. If the Rust build path is worse than wezig's Zig one at
   reaching mobile parity, that is a valid finding of the same reversible
   experiment the thesis (`0001`) frames.

## Considered options

- **`dist` (formerly `cargo-dist`).** The Rust-native purpose-built tool: reads
  `Cargo.toml`, generates its own `release.yml`, produces first-class installers
  (shell/PowerShell/Homebrew/npm). **Rejected as the primary** because (a) it is
  opinionated around a clean Cargo-workspace release, whereas werust links
  **system libraries (WebKitGTK)** and must ship **non-Cargo mobile artifacts**
  (Android/iOS) — exactly the bespoke jobs a general orchestrator hosts more
  naturally than a Cargo-shaped generator; (b) it does not advance the
  Zig-less-parity-with-wezig comparison, which a shared GoReleaser shape does;
  and (c) post-axodotdev it is effectively single-maintainer. It remains the
  better choice IF mobile is dropped and first-class desktop installers become
  the priority — revisit then.
- **Plain `cargo` + a hand-written `release.yml`.** Maximum control, but throws
  away wezig's proven release shape and the port-parity that makes the two arms
  comparable.

## Consequences

- The `.goreleaser.yaml` and `release.yml` are ported from wezig's, swapping the
  Zig builder for `builder: rust` and the Zig-lib cross-compile steps in the
  mobile jobs for their Rust equivalents (cargo cross-compile of the static lib
  each mobile app links). Set these up when the first shippable binary exists.
- GoReleaser's Rust builder is younger/second-class vs its Go support; accept the
  occasional rough edge as the cost of the general-orchestrator + parity benefits.
- The changelog is generated FROM git history, so the **conventional-commit
  convention is load-bearing** (see `CONTEXT.md` Conventions) — no per-change
  changeset files.

## Update: desktop is native x86_64-only (Zig-less in the FULL sense), mobile decoupled

The desktop leg no longer uses **any** Zig, not even as a cross-linker. It builds
only `x86_64-unknown-linux-gnu`, NATIVELY (`builder: rust` with `command: build`
= plain `cargo build`, driving the host system linker). Desktop arm64 Linux is
**dropped**; arm64 now lives only on the mobile side (Android NDK / iOS Xcode).
The two mobile jobs are also **decoupled** from the desktop leg.

**Why (the load-bearing reason).** GoReleaser's rust builder cross-compiles with
`cargo-zigbuild`, which uses `zig cc` as the linker. `zig cc` does **not** search
the host's system library paths, so it cannot link the desktop binary's system
**WebKitGTK/GTK/glib** even with `libwebkitgtk-6.0-dev` installed — every dry-run
failed with `error: unable to find dynamic system library 'glib-2.0'` (e.g. run
29902579536). Zig-as-linker is the wrong tool for a binary that dynamically links
system libraries. The **native system linker already links WebKitGTK fine** (the
`verify` job + local builds prove it), so the desktop leg uses it directly. This
supersedes the earlier framing that read Zig-less as "Zig-less for the
language/renderer, but a Zig linker is fine": for a system-lib-linking desktop
binary the Zig linker is not fine, and the desktop path is now Zig-less end to end.
(arm64 desktop Linux would need an arm64 GTK/WebKit sysroot or a native arm64
runner — an ADR-level restructure, not worth it for a desktop-Linux-first project;
see `work/notes/observations/why-zig-in-release-and-rust-native-alternatives-2026-07-22.md`.)

**Mobile decoupled from desktop.** `android-apk` and `ios-simulator-app` used to
`needs: goreleaser` purely for ORDERING (so a tag's Release existed to upload
into). That coupling let a desktop link failure block the APK/`.app`. Both now
`needs: verify` (independent of the desktop leg). On a tag, each mobile job
idempotently `gh release create "$TAG" --generate-notes 2>/dev/null || true`
before `gh release upload --clobber` — a Release-EXISTENCE guarantee, NOT a
desktop-build dependency. The `workflow_dispatch` snapshot dry-run is unchanged:
it still builds every artifact and uploads workflow artifacts, publishing nothing.

Implemented by task `fix-release-native-x86-desktop-and-decouple-mobile` (human
chose B+C).
