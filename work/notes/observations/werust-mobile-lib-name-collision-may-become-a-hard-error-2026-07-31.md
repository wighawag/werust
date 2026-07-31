# `werust_mobile` output-filename collision, which cargo says may become a hard error

2026-07-31, noticed while building `verify-lints-test-targets-and-clears-the-existing-debt`.

Every `cargo build` of the workspace prints two `output filename collision` warnings: `werust-ios-core` and `werust-android-core` both name their lib target `werust_mobile`, so they fight over `target/debug/libwerust_mobile.{a,rlib}`. Cargo adds "this may become a hard error in the future" (rust-lang/cargo#6313), which would red the gate on a toolchain bump rather than on anything anyone changed. It is a cargo warning, not a lint, so the new `-D warnings` clippy gate does NOT catch or fix it.
