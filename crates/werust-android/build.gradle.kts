// The werust Android app module: a real app that links the werust Rust core
// (libwerust_mobile.so) cross-compiled per ABI and packaged into an unsigned
// debug APK. See `docs/spikes/mobile-android-shell-and-static-lib/DECISIONS.md`.
plugins {
    id("com.android.application") version "8.13.2" apply false
    kotlin("android") version "2.0.21" apply false
}
