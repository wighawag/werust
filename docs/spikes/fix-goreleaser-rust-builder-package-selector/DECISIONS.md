# Decisions — fix-goreleaser-rust-builder-package-selector

Durable record of the load-bearing choice made fixing the GoReleaser desktop build (task `fix-goreleaser-rust-builder-package-selector`, spec story 19, `docs/adr/0002`). Linked from the task done record so a reviewer + the human can ratify or reverse. Current truth remains the code (`.goreleaser.yaml`) + the guarding test (`crates/werust-core/tests/release_plumbing_shape.rs`); this file only explains the choice.

## The workspace selector is passed via `flags`, and MUST keep `--release` alongside `--package=werust`

**Chosen:** the desktop `builder: rust` build now sets `flags: [--release, --package=werust]`.

**The bug being fixed:** werust is a multi-crate Cargo workspace, so without a package selector GoReleaser's rust builder fails at CI runtime with "you need to specify which workspace to build, please add '--package=[name]'" (the risk flagged at this task's Gate-3). `--package=werust` selects the `werust` binary crate.

**Why `flags` and not a `package:` key:** the GoReleaser v2 rust builder (v2.5+) has NO dedicated package key. Its documented Cargo-workspace mechanism (GoReleaser docs, Builders -> Rust -> Caveats -> Cargo Workspaces) is verbatim "add `-p=[name]` to the `flags` property", and its source `isSettingPackage` only recognises flags prefixed `-p=` or `--package=`. `--package=werust` is the cargo-native long form the error message itself suggests, so it is the spelling used.

**Why `--release` MUST be kept — a deviation from the task's literal example, recorded here so it is visible:** the task body's `What to change` example showed `flags: [--package=werust]` alone. Building exactly that would be WRONG. GoReleaser's rust builder applies its `--release` default ONLY when `flags` is empty (source `WithDefaults`: `if len(build.Flags) == 0 { build.Flags = []string{"--release"} }`), so any explicit `flags` list REPLACES that default. Its `Build` step then copies the built artifact from the hardcoded `target/<triple>/release/` path, so a debug build (no `--release`) would leave nothing at that path and break the release. Adding `--release` preserves the config's pre-existing effective behaviour (it previously had no `flags`, so `--release` was the default in force). This is a factual necessity read straight from the builder source (`internal/builders/rust/build.go`), not a scope expansion.

**Alternatives considered:**
- *`flags: [--package=werust]` alone (the task's literal example).* Rejected: drops `--release`, so cargo builds a debug binary while GoReleaser looks in `target/<triple>/release/` and finds nothing — a broken release. The task's example was illustrative of the selector, not a complete flag list.
- *A `package:` / `dir:` key instead of `flags`.* There is no `package:` key on the rust builder; `dir:` sets the working directory, not the workspace member, and would not disambiguate a single Cargo.toml workspace. `flags` is the only documented mechanism.

**What it touches:** only `.goreleaser.yaml`'s desktop `builds[].flags`. Targets, archives, checksum, changelog, builder, and all mobile/release-workflow wiring are untouched; no Rust production code changed. Guarded by `crates/werust-core/tests/release_plumbing_shape.rs::goreleaser_selects_the_werust_package_in_the_workspace`, which asserts BOTH the package selector AND `--release` are present, so a future edit dropping either turns the pure-Rust `verify` gate red.

**Verification boundary (acceptance criterion 4):** the pure-Rust `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`) does NOT run `goreleaser`, so it proves the YAML SHAPE (via the test above) but cannot prove the CI-runtime build. Full proof is a re-run of the release `workflow_dispatch` snapshot dry-run getting PAST GoReleaser's "building" step and producing the desktop dist — stated here and in the done record; it cannot be shown in this gate.
