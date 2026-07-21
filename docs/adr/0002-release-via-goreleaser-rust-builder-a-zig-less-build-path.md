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
