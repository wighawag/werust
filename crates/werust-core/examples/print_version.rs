//! Print werust's ONE version string on stdout, and nothing else.
//!
//! This exists so a PACKAGING SCRIPT can read the version the compiled code
//! reports, instead of computing its own. `crates/werust-macos/bundle-app.sh`
//! stamps the output into `Werust.app`'s `CFBundleVersion`, so the bundle and
//! the ⋮ menu inside it can never disagree.
//!
//! The value is [`werust_core::version`], resolved ONCE at build time by this
//! crate's `build.rs` from `WERUST_VERSION` (the release tag, injected by every
//! release leg), else `git describe --tags --always`, else the Cargo version.
//! Running this example under the SAME environment as the release build
//! therefore yields exactly the string the shipped binary will report.
//!
//! The alternative, re-deriving "tag, else `git describe`" in shell, is
//! precisely the SECOND version source [`werust_core::version`]'s own docs
//! forbid, and the one the Android sibling task
//! `android-apk-version-from-the-release-tag` exists to undo. A readout cannot
//! drift; a re-derivation can. See
//! `docs/spikes/macos-release-packaging-leg/README.md` (decision 1).
//!
//! Deliberately an EXAMPLE and not a binary target: it is build tooling, not a
//! product surface, so it stays out of `werust-core`'s public shape and out of
//! every shipped artifact. werust's user-facing version verb is still
//! `werust version` in the GTK binary (which cannot build on macOS, which is why
//! the packaging script could not just call it).

fn main() {
    println!("{}", werust_core::version());
}
