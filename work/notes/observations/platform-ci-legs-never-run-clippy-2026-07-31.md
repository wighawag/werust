# The macOS and Windows CI legs never run clippy

2026-07-31, noticed while building `verify-lints-test-targets-and-clears-the-existing-debt`.

`.github/workflows/macos-renderer.yml` and `.github/workflows/windows-renderer.yml` build, test and RUN the platform crates on native runners but have no clippy step (the only two clippy invocations in the repo are `verify.yml` and `release.yml`, both Ubuntu). So the ~7.5k lines behind `#[cfg(target_os = "macos")]` / `#[cfg(windows)]` (the two WebView backends, both native windows, both origin probes, and the four load-bearing example smokes) are unlinted EVERYWHERE, not just on the Ubuntu gate. Adding `cargo clippy --all-targets -- -D warnings` (scoped with the same `-p` sets those legs already use) would cover them with no cross-target trick, since the platform half already compiles there. Not done in that task (its scope was the Ubuntu gate); inventory in `docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md`.
