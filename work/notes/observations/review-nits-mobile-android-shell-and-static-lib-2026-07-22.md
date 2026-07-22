---
title: review-gate non-blocking nits for 'mobile-android-shell-and-static-lib' (Gate 2 approve)
date: 2026-07-22
status: open
reviewOf: mobile-android-shell-and-static-lib
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'mobile-android-shell-and-static-lib' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify: the acceptance phrase 'static lib' is realized as a JNI cdylib (libwerust_mobile.so) packaged per-ABI, not a literal .a, because Android cannot dlopen a .a at runtime. crate-type also emits staticlib for anyone wanting it. Recorded in DECISIONS.md; reasonable and honours the spec/release-job intent.
  (crates/werust-android/rust/Cargo.toml crate-type=[cdylib,staticlib,rlib]; DECISIONS.md 'static lib vs .so' note)
- Ratify: the BUILD-leg APK-ABI assertion is a standalone script (docs/spikes/.../check-apk-abis.sh) run in the release/mobile CI job, NOT part of the Rust verify gate, since Gradle/APK needs the Android SDK+NDK which the pure-Rust gate lacks. Core logic stays covered by cargo test. Recorded and sound.
  (check-apk-abis.sh; DECISIONS.md BUILD-leg section)
- Doc drift: backend.rs says on_page_committed is 'Called from Kotlin onPageCommitVisible', but BrowserActivity.kt actually wires it from onPageStarted. Harmless comment mismatch; behaviour is fine either way. Consider aligning the comment.
  (backend.rs:136 vs BrowserActivity.kt:128-129)
