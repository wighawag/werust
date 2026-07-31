# macOS signing/notarization follow-on is named everywhere but has no task file (2026-07-31)

Spotted while landing `macos-release-packaging-leg`. The task body, `windows-release-packaging-leg`, the release workflow header and now the README all point at a macOS signing + notarization FOLLOW-ON ("the macOS analogue of `android-apk-signing`"), but there is no such item in `work/tasks/backlog/`, unlike the Android side, where `android-apk-signing` existed as a real task before anything referenced it. So the artifact that ships today is unsigned with no tracked plan to change that.

Related, from the same build: if that follow-on ever adds notarization, it must check whether Apple accepts the `git describe`-shaped `CFBundleVersion` this leg stamps on non-tag builds (decision 4 in `docs/spikes/macos-release-packaging-leg/README.md`).
