# Decisions — release-goreleaser-rust-desktop-and-mobile-artifacts

Durable record of the design choices made wiring the release pipeline (GoReleaser Rust builder for desktop + hand-written mobile jobs; `docs/adr/0002`, spec story 19). Linked from the task done record so a reviewer + the human can ratify or reverse. Current truth remains the code (`.goreleaser.yaml`, `.github/workflows/release.yml`) + ADR-0002; this file only explains the load-bearing choices.

Deliverables:
- `.goreleaser.yaml` — GoReleaser v2 `builder: rust` (cargo-zigbuild) desktop config.
- `.github/workflows/release.yml` — the three-job pipeline (`verify` -> `goreleaser` -> `android-apk` + `ios-simulator-app`), tag path (real release) + `workflow_dispatch` dry-run (snapshot, publishes nothing).
- `crates/werust-core/tests/release_plumbing_shape.rs` — the shape test that runs under the `verify` gate.

## Desktop targets are the two Linux triples the acceptance criteria name (amd64 + arm64), no macOS/Windows desktop

**Chosen:** `.goreleaser.yaml`'s rust build declares exactly `x86_64-unknown-linux-gnu` + `aarch64-unknown-linux-gnu`. No `x86_64-apple-darwin` / `x86_64-pc-windows-*` desktop targets.

**Why:** criterion 1 is verbatim "desktop **Linux** binaries (amd64, arm64) + checksums". The day-one desktop product links **WebKitGTK** (`crates/werust`'s `gtk4` + WebKitGTK deps), which is Linux-first; wezig's desktop floor is likewise Linux. Adding macOS/Windows desktop targets would need their own system-webview backends that do not exist yet, so shipping empty/broken cross-desktop binaries would be worse than parity.

**Alternatives considered:** add darwin/windows desktop targets now — rejected: no webview backend for those OSes yet; that is a later slice, not this task.

**What it touches:** the desktop artifact matrix. If a macOS/Windows desktop backend lands later, extend `builds[].targets` (and the archive `name_template` already keys off `.Os`/`.Arch`, so it absorbs new targets).

## The test seam is a Rust shape test that PARSES the config files, hosted in `werust-core`, using a dev-only `serde_yaml`

**Chosen:** the "test-first" artifact is `crates/werust-core/tests/release_plumbing_shape.rs`: it parses `.goreleaser.yaml` + `.github/workflows/release.yml` as YAML and asserts the shape each acceptance criterion names (builder: rust; the two Linux targets; a checksum block; `changelog.use: git` + conventional-commit groups + no `.changeset/`; the three jobs with their `needs:` gating; `--snapshot` dry-run + `actions/upload-artifact`; `gh release upload` on a tag; `verify` running the four cargo steps before the desktop build). `serde_yaml` is a **dev-dependency of `werust-core` only**.

**Why:** a CI/release-config task has no Rust production code to unit-test; the deliverable IS declarative YAML whose SHAPE is the contract. Parsing + asserting the structure (not string-grepping) is the objective bar that runs inside the always-on pure-Rust `verify` gate (`cargo test`) and breaks if a future edit drifts the shape. `werust-core` hosts it because it is the ONE shared artifact all three release legs build (desktop links it; both mobile legs cross-compile it) and it carries no GTK/SDK deps, so the test needs no extra toolchain. `serde_yaml` is dev-only, so the shipped binary is unaffected.

**Alternatives considered:**
- *A new `crates/release-config` crate just for the test.* Rejected: a whole workspace member for one config test is heavier than a `tests/` file on the crate the release already centres on.
- *String `contains` checks with no YAML parse.* Rejected: brittle (whitespace/key-order/quoting), and would pass on malformed YAML. Parsing catches a broken file too.
- *Shell out to `yq` from the test.* Rejected: makes the always-on gate depend on `yq` being installed, which is not guaranteed and is not the repo's Rust house style.

**What it touches:** `werust-core/Cargo.toml` gains a `[dev-dependencies] serde_yaml`; `Cargo.lock` gains `serde_yaml` + `unsafe-libyaml`. Dev-only: no effect on any shipped artifact.

## `builder: rust` with `tool: cargo` + `command: zigbuild` (cargo-zigbuild) — the Zig-less path

**Chosen:** the desktop build uses GoReleaser's native `builder: rust` driving `cargo zigbuild` (cargo-zigbuild) for the cross-compile, exactly as ADR-0002 frames "swap `builder: zig` for `builder: rust` (via cargo-zigbuild)". The `goreleaser` CI job installs `libwebkit2gtk-4.1-dev` + `libgtk-4-dev`, adds the two Rust targets, and sets up Zig (for cargo-zigbuild) via `mlugg/setup-zig`.

**Why:** this is the whole point of the task — proving the deliberately Zig-less BUILD path (Rust as the single language; cross-compilation by cargo-zigbuild, not `zig build`). The desktop binary links system WebKitGTK, so the toolchain leg installs the GTK/WebKit dev libraries; that is the general-orchestrator advantage ADR-0002 cites over cargo-dist (which assumes a clean Cargo-only workspace).

**What it touches:** the desktop toolchain leg. The `before.hooks` in `.goreleaser.yaml` also `rustup target add` + `cargo install cargo-zigbuild` so a local `goreleaser` run reproduces the CI toolchain.

## Tag path vs dispatch dry-run is branched inside each job with `if:`, not with two separate workflows

**Chosen:** one `release.yml`, triggered by `push: tags: v*` AND `workflow_dispatch`. Each job branches on the trigger: `if: startsWith(github.ref, 'refs/tags/')` runs the real-publish steps (GoReleaser `release --clean`; `gh release upload` for the mobile legs); `if: github.event_name == 'workflow_dispatch'` runs the dry-run steps (GoReleaser `--snapshot --skip=publish`; `actions/upload-artifact` for every leg).

**Why:** wezig keeps the tag path + the dispatch dry-run in one release workflow; the forward-pointer on this task spells out the same three-job shape with `gh release upload` on a tag and `actions/upload-artifact` on the dispatch. One workflow keeps the two paths visibly parallel (a reviewer sees they build the SAME artifacts) and avoids duplicating the three build jobs across two files.

**What it touches:** the whole release workflow. On the dry-run nothing is published (GoReleaser `--snapshot` never touches the forge; the mobile legs upload artifacts, never `gh release upload`), satisfying criterion 4.

## The mobile legs REUSE the landed module contracts verbatim (no reinvention)

**Chosen:** `android-apk` builds with `(cd crates/werust-android && ./gradlew :app:assembleDebug)` and runs `docs/spikes/mobile-android-shell-and-static-lib/check-apk-abis.sh` on the produced `app-debug.apk`; `ios-simulator-app` (on `macos-14`) builds with `BUILD_ONLY=1 crates/werust-ios/build-and-run.sh`, runs `docs/spikes/mobile-ios-shell-and-static-lib/check-app-bundle.sh` on the stable `crates/werust-ios/build/WerustShell.app`, then zips + attaches it.

**Why:** the mobile tasks landed with an explicit "reuse this, do not reinvent" contract (their DECISIONS.md + the forward-pointers on this task): the Gradle task cross-compiles the shared `werust-core` per ABI, and the BUILD-leg checks are where the ABI/bundle acceptance assertions actually EXECUTE (they are deliberately outside the pure-Rust `verify` gate, which lacks the SDK/NDK/Xcode). The release job is exactly the "hand-written mobile job alongside GoReleaser" ADR-0002 describes, so it runs those checks rather than duplicating their logic.

**What it touches:** nothing in the mobile modules — it consumes their existing Gradle task, build script, and check scripts by path. If those move, this workflow's step commands move with them.
