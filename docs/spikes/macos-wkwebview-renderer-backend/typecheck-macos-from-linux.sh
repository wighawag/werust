#!/usr/bin/env bash
# Type-check werust's macOS-only Rust from a LINUX development machine.
#
# WHY THIS EXISTS: `crates/macos-renderer`'s engine half is
# `#[cfg(target_os = "macos")]`, so the Ubuntu `verify` gate never compiles it and
# a mistake in ~700 lines of Objective-C wiring would only surface on a CI runner,
# one round trip at a time. The `objc2` family is pure Rust (extern declarations,
# no C), so `cargo check --target aarch64-apple-darwin` type-checks all of it on
# Linux WITHOUT a macOS SDK -- as long as nothing in the dependency graph needs a
# C compiler.
#
# THE ONE OBSTACLE: `fetcher` -> `ureq` -> `rustls` -> `ring`, whose build script
# compiles C and fails for an Apple target on Linux ("no such file
# bits/libc-header-start.h"). So this script builds a scratch workspace OUTSIDE
# the repo that symlinks the REAL macOS sources but swaps `werust-core`,
# `fetcher` and `webview-shared` for tiny API-compatible stand-ins. That is enough
# to type-check every `objc2` call, every `define_class!` block and every seam
# signature, which is what the SDK-free check is for.
#
# WHAT IT IS NOT: a build. It does not link against AppKit/WebKit and it cannot
# run a message send. The real proof is `.github/workflows/macos-renderer.yml` on
# the `macos-14` runner; this is the fast local loop that keeps that job from
# being the first place a typo is found.
#
# USAGE (from anywhere):
#     docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh
#
# Requires: `rustup target add aarch64-apple-darwin`.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRATCH="${SCRATCH_DIR:-${TMPDIR:-/tmp}/werust-macos-typecheck}"

if ! rustup target list --installed | grep -q '^aarch64-apple-darwin$'; then
  echo "installing the aarch64-apple-darwin std..." >&2
  rustup target add aarch64-apple-darwin
fi

rm -rf "$SCRATCH"
mkdir -p "$SCRATCH"/{src,examples,fake-core/src,fake-fetcher/src,fake-shared}

cat > "$SCRATCH/Cargo.toml" <<EOF
[workspace]
members = ["fake-core", "fake-fetcher", "fake-shared"]

[package]
name = "werust-macos-typecheck"
version = "0.0.0"
edition = "2021"

[lib]
name = "macos_renderer"
path = "src/lib.rs"

[[example]]
name = "trust_hooks_smoke"
path = "examples/trust_hooks_smoke.rs"

[dependencies]
renderer = { path = "$REPO/crates/renderer" }
webview-shared = { path = "fake-shared" }
werust-core = { path = "fake-core" }
fetcher = { path = "fake-fetcher" }
EOF

# Mirror the REAL macOS dependency block, so a feature this crate forgot to
# enable shows up here rather than on the runner.
sed -n "/^\[target\.'cfg(target_os = \"macos\")'\.dependencies\]/,\$p" \
  "$REPO/crates/macos-renderer/Cargo.toml" \
  | sed '/^\[dev-dependencies\]/,$d' >> "$SCRATCH/Cargo.toml"

cat > "$SCRATCH/fake-fetcher/Cargo.toml" <<EOF
[package]
name = "fetcher"
version = "0.2.9"
edition = "2021"
EOF
cat > "$SCRATCH/fake-fetcher/src/lib.rs" <<'EOF'
// A stand-in for `crates/fetcher`, carrying ONLY the API the macOS backend and
// its smoke touch. It exists to keep `ring` (which cannot cross-compile here) out
// of the graph; it is never linked into anything real.
#[derive(Debug)]
pub enum RetrieveError {
    MissingBlock { cid: String },
    BlockHashMismatch { cid: String },
}
#[derive(Debug)]
pub struct VerifyError;
pub struct RetrievedContent {
    pub bytes: Vec<u8>,
    pub codec: u64,
}
pub trait ContentRetriever {
    fn retrieve(&self, cid: &str, path: &str) -> Result<RetrievedContent, RetrieveError>;
}
pub struct HttpFetcher;
impl HttpFetcher {
    pub fn new() -> Self {
        HttpFetcher
    }
}
impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}
pub struct TrustlessGatewayCarRetriever;
impl TrustlessGatewayCarRetriever {
    pub fn with_gateway(_f: HttpFetcher, _g: &str) -> Self {
        TrustlessGatewayCarRetriever
    }
}
impl ContentRetriever for TrustlessGatewayCarRetriever {
    fn retrieve(&self, _cid: &str, _path: &str) -> Result<RetrievedContent, RetrieveError> {
        unimplemented!()
    }
}
pub fn cid_v1_raw_sha256(_bytes: &[u8]) -> Result<String, VerifyError> {
    Ok(String::new())
}
EOF

cat > "$SCRATCH/fake-core/Cargo.toml" <<EOF
[package]
name = "werust-core"
version = "0.2.9"
edition = "2021"
[dependencies]
renderer = { path = "$REPO/crates/renderer" }
fetcher = { path = "../fake-fetcher" }
EOF
cat > "$SCRATCH/fake-core/src/lib.rs" <<'EOF'
// A stand-in for `crates/werust-core`, carrying ONLY the API the macOS backend
// and its smoke touch. See fake-fetcher for why.
pub mod ipfs {
    use fetcher::ContentRetriever;
    use renderer::{RendererError, SchemeRequest, SchemeResponse};
    pub const IPFS_SCHEME: &str = "ipfs";
    #[derive(Clone, Default)]
    pub struct RedirectSink;
    impl RedirectSink {
        pub fn new() -> Self {
            RedirectSink
        }
        pub fn is_main_frame(&self, _url: &str) -> bool {
            false
        }
    }
    pub fn resolve_ipfs_request<R: ContentRetriever>(
        _r: &R,
        _request: &SchemeRequest,
        _sink: &RedirectSink,
    ) -> Result<SchemeResponse, RendererError> {
        unimplemented!()
    }
}
pub mod retrieval {
    use renderer::{RendererError, SchemeRequest, SchemeResponse};
    pub const WERUST_SCHEME: &str = "werust";
    pub fn active_gateway_endpoint() -> String {
        String::new()
    }
    pub fn apply_settings_request(
        _request: &SchemeRequest,
    ) -> Result<SchemeResponse, RendererError> {
        unimplemented!()
    }
}
pub mod provider {
    use renderer::ScriptMessage;
    pub const PROVIDER_BRIDGE: &str = "werustProvider";
    pub const STUB_CHAIN_ID: &str = "0x1";
    pub struct ProviderBridge;
    impl ProviderBridge {
        pub fn new() -> Self {
            ProviderBridge
        }
    }
    impl Default for ProviderBridge {
        fn default() -> Self {
            Self::new()
        }
    }
    pub fn provider_shim() -> String {
        String::new()
    }
    pub fn route_provider_message(
        _bridge: &ProviderBridge,
        _message: &ScriptMessage,
        _respond: &mut dyn FnMut(String),
    ) {
    }
}
EOF

# `webview-shared` is checked as its REAL source (it is the moved, shared code),
# just re-pointed at the stand-in core/fetcher.
cat > "$SCRATCH/fake-shared/Cargo.toml" <<EOF
[package]
name = "webview-shared"
version = "0.2.9"
edition = "2021"
[dependencies]
renderer = { path = "$REPO/crates/renderer" }
werust-core = { path = "../fake-core" }
fetcher = { path = "../fake-fetcher" }
EOF
ln -sfn "$REPO/crates/webview-shared/src" "$SCRATCH/fake-shared/src"

# The REAL macOS sources, by symlink, so this always checks what is committed.
ln -sf "$REPO/crates/macos-renderer/src/backend.rs" "$SCRATCH/src/backend.rs"
ln -sf "$REPO/crates/macos-renderer/src/pure.rs" "$SCRATCH/src/pure.rs"
ln -sf "$REPO/crates/macos-renderer/examples/trust_hooks_smoke.rs" \
  "$SCRATCH/examples/trust_hooks_smoke.rs"
cat > "$SCRATCH/src/lib.rs" <<'EOF'
pub mod pure;
#[cfg(target_os = "macos")]
mod backend;
#[cfg(target_os = "macos")]
pub use backend::{MacosRenderer, OffThreadResolve};
EOF

echo "checking the macOS backend + smoke against aarch64-apple-darwin ..."
(cd "$SCRATCH" && cargo clippy --target aarch64-apple-darwin --all-targets)

# The origin probe has no repo path dependencies, so it checks in place.
echo "checking the macOS origin probe against aarch64-apple-darwin ..."
(cd "$REPO" && cargo clippy -p macos-origin-probe --target aarch64-apple-darwin --all-targets)

echo
echo "OK -- the macOS sources type-check. This is NOT a build: the real proof is"
echo ".github/workflows/macos-renderer.yml on the macos-14 runner."
