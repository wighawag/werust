#!/usr/bin/env bash
# Type-check werust's macOS-only Rust from a LINUX development machine.
#
# WHY THIS EXISTS: `crates/macos-renderer`'s engine half and
# `crates/werust-macos`'s AppKit window half are both
# `#[cfg(target_os = "macos")]`, so the Ubuntu `verify` gate never compiles them
# and a mistake in the Objective-C wiring would only surface on a CI runner, one
# round trip at a time. The `objc2` family is pure Rust (extern declarations,
# no C), so `cargo check --target aarch64-apple-darwin` type-checks all of it on
# Linux WITHOUT a macOS SDK -- as long as nothing in the dependency graph needs a
# C compiler.
#
# WHAT THE STAND-IN CORE DOES AND DOES NOT PROVE: `werust-core` is swapped for a
# tiny API-compatible fake here (see THE ONE OBSTACLE below), so this check
# proves the AppKit/objc2 wiring, NOT that the window agrees with the real core's
# signatures. That agreement is proven where it belongs: `paint.rs` -- the ONLY
# place `werust-macos` touches the chrome derivation -- compiles and is
# unit-tested against the REAL `werust-core` on every Ubuntu `verify` run, and
# the window reads plain `paint` structs. Keep it that way: a window that starts
# calling `werust_core` directly moves itself outside both checks.
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
mkdir -p "$SCRATCH"/{src,examples,fake-core/src,fake-fetcher/src,fake-shared,window}

cat > "$SCRATCH/Cargo.toml" <<EOF
[workspace]
members = ["fake-core", "fake-fetcher", "fake-shared", "window"]

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
pub mod menu {
    // The shared browser MENU: the macOS ⋮ menu and the menu bar are BUILT from
    // this list, so a stand-in must carry its shape (not its content).
    pub const MENU_ITEM_VERSION: &str = "version";
    pub const MENU_ITEM_DEBUG: &str = "debug";
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MenuItemKind {
        Info,
        Action,
    }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MenuItem {
        pub id: String,
        pub label: String,
        pub kind: MenuItemKind,
    }
    pub struct BrowserMenu {
        items: Vec<MenuItem>,
    }
    impl BrowserMenu {
        pub fn new() -> Self {
            BrowserMenu { items: Vec::new() }
        }
        pub fn items(&self) -> &[MenuItem] {
            &self.items
        }
        pub fn item(&self, _id: &str) -> Option<&MenuItem> {
            None
        }
    }
    impl Default for BrowserMenu {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub mod debug {
    use renderer::TrustPosture;
    pub const CAPTURE_BRIDGE: &str = "werustDebug";
    pub const MAX_CONSOLE_ENTRIES: usize = 300;
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum ConsoleLevel {
        #[default]
        Log,
        Info,
        Warn,
        Error,
        Debug,
    }
    impl ConsoleLevel {
        pub const ALL: [ConsoleLevel; 5] = [
            ConsoleLevel::Log,
            ConsoleLevel::Info,
            ConsoleLevel::Warn,
            ConsoleLevel::Error,
            ConsoleLevel::Debug,
        ];
        pub fn wire_name(self) -> &'static str {
            ""
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct ConsoleEntry {
        pub level: ConsoleLevel,
        pub message: String,
        pub source: String,
        pub line: Option<u32>,
        pub timestamp: u64,
    }
    impl ConsoleEntry {
        pub fn new(_level: ConsoleLevel, _message: impl Into<String>) -> Self {
            Self::default()
        }
        pub fn sequence(&self) -> u64 {
            0
        }
        pub fn with_source(self, _source: impl Into<String>) -> Self {
            self
        }
        pub fn with_line(self, _line: u32) -> Self {
            self
        }
    }
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct NetworkEntry {
        pub method: String,
        pub url: String,
        pub scheme: String,
        pub status: Option<u16>,
        pub mime: String,
        pub size: Option<u64>,
        pub trust: TrustPosture,
    }
    impl NetworkEntry {
        pub fn new(_method: impl Into<String>, _url: impl Into<String>) -> Self {
            Self::default()
        }
        pub fn sequence(&self) -> u64 {
            0
        }
        pub fn with_status(self, _status: u16) -> Self {
            self
        }
        pub fn with_mime(self, _mime: impl Into<String>) -> Self {
            self
        }
        pub fn with_size(self, _size: u64) -> Self {
            self
        }
        pub fn with_trust(self, _trust: TrustPosture) -> Self {
            self
        }
    }
    #[derive(Clone, Default)]
    pub struct DebugCapture;
    impl DebugCapture {
        pub fn new() -> Self {
            DebugCapture
        }
        pub fn console(&self) -> Vec<ConsoleEntry> {
            Vec::new()
        }
        pub fn network(&self) -> Vec<NetworkEntry> {
            Vec::new()
        }
        pub fn push_console(&self, _entry: ConsoleEntry) {}
        pub fn push_network(&self, _entry: NetworkEntry) {}
        pub fn clear(&self) {}
    }
    pub const DEBUG_CONSOLE_CSS_CLASSES: &[&str] = &[];
    pub fn console_level_css_class(_level: ConsoleLevel) -> &'static str {
        ""
    }
    pub fn console_source_line(_entry: &ConsoleEntry) -> String {
        String::new()
    }
    pub fn console_row_text(_entry: &ConsoleEntry) -> String {
        String::new()
    }
    pub fn network_status_text(_status: Option<u16>) -> String {
        String::new()
    }
    pub fn network_mime_text(_mime: &str) -> String {
        String::new()
    }
    pub fn network_size_text(_size: Option<u64>) -> String {
        String::new()
    }
    pub fn network_trust_label(_posture: TrustPosture) -> String {
        String::new()
    }
    pub fn network_trust_css_class(_posture: TrustPosture) -> &'static str {
        ""
    }
    pub fn request_trust_posture(_scheme: &str, _verified: bool) -> TrustPosture {
        TrustPosture::UnverifiedOrigin
    }
    pub fn trust_posture_wire_name(_posture: TrustPosture) -> &'static str {
        ""
    }
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum TailPlan {
        Rebuild,
        AppendFrom { drop: usize, from: usize },
        Noop,
    }
    pub fn tail_plan(_sequences: &[u64], _rendered: usize, _last: Option<u64>) -> TailPlan {
        TailPlan::Noop
    }
    pub fn console_shim() -> String {
        String::new()
    }
    pub fn network_shim() -> String {
        String::new()
    }
    pub fn route_capture_message(_capture: &DebugCapture, _body: &str) {}
}

// --- The chrome DERIVATION + the shell (`werust-macos`'s `paint.rs` reads
// these; the real ones are what the Ubuntu gate compiles it against). ---
pub const TRUST_INDICATOR_CSS_CLASSES: &[&str] = &[];
pub const ERROR_BANNER_CSS_CLASSES: &[&str] = &[];
pub const CHROME_CSS_CLASS_SETS: &[&[&str]] = &[];

pub fn version() -> &'static str {
    ""
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChromeState {
    pub url_text: String,
    pub load_state: renderer::LoadState,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub last_error: Option<String>,
    pub trust_posture: renderer::TrustPosture,
    pub invalid_entry: Option<String>,
}

impl ChromeState {
    pub fn is_loading(&self) -> bool {
        false
    }
}

pub fn status_line(_state: &ChromeState) -> String {
    String::new()
}
pub fn error_banner_visible(_state: &ChromeState) -> bool {
    false
}
pub fn error_banner_text(_state: &ChromeState) -> String {
    String::new()
}
pub fn error_banner_css_class(_state: &ChromeState) -> &'static str {
    ""
}
pub fn invalid_entry_badge_visible(_state: &ChromeState) -> bool {
    false
}
pub fn invalid_entry_badge_text(_state: &ChromeState) -> &'static str {
    ""
}
pub fn trust_indicator(_state: &ChromeState) -> &'static str {
    ""
}
pub fn trust_indicator_detail(_state: &ChromeState) -> &'static str {
    ""
}
pub fn trust_indicator_css_class(_state: &ChromeState) -> &'static str {
    ""
}
pub fn load_progress_visible(_state: &ChromeState) -> bool {
    false
}
pub fn load_progress_fraction(_state: &ChromeState) -> f64 {
    0.0
}
pub fn load_progress_hint(_state: &ChromeState) -> &'static str {
    ""
}

pub struct BrowserShell {
    chrome: ChromeState,
    renderer: Box<dyn renderer::Renderer>,
}

impl BrowserShell {
    pub fn new(renderer: Box<dyn renderer::Renderer>) -> Self {
        BrowserShell {
            chrome: ChromeState::default(),
            renderer,
        }
    }
    pub fn with_redirect_sink(self, _redirects: crate::ipfs::RedirectSink) -> Self {
        self
    }
    pub fn with_debug_capture(self, _debug: crate::debug::DebugCapture) -> Self {
        self
    }
    pub fn chrome(&self) -> &ChromeState {
        &self.chrome
    }
    pub fn navigate(&mut self, url: &str) -> Result<(), renderer::RendererError> {
        self.renderer.navigate(url)
    }
    pub fn reload(&mut self) -> Result<(), renderer::RendererError> {
        self.renderer.reload()
    }
    pub fn stop(&mut self) {
        self.renderer.stop()
    }
    pub fn go_back(&mut self) {
        self.renderer.go_back()
    }
    pub fn go_forward(&mut self) {
        self.renderer.go_forward()
    }
    pub fn pump(&mut self) -> bool {
        self.renderer.poll_event().is_some()
    }
    pub fn focus_page(&mut self, focused: bool) {
        self.renderer.set_focus(focused)
    }
    pub fn view_handle(&self) -> renderer::ViewHandle {
        self.renderer.view_handle()
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

# The WINDOW crate (`crates/werust-macos`): the same treatment, as a member of
# the same scratch workspace so it links against the REAL `macos-renderer`
# sources above. Its `paint.rs` is symlinked in too even though the Ubuntu gate
# already compiles it -- the point here is that the AppKit half agrees with the
# paint half, which is only checkable when both are present.
cat > "$SCRATCH/window/Cargo.toml" <<EOF
[package]
name = "werust-macos"
version = "0.0.0"
edition = "2021"

[lib]
name = "werust_macos"
path = "src/lib.rs"

[[example]]
name = "window_smoke"
path = "examples/window_smoke.rs"

[dependencies]
renderer = { path = "$REPO/crates/renderer" }
werust-core = { path = "../fake-core" }
macos-renderer = { path = "..", package = "werust-macos-typecheck" }

[dev-dependencies]
fetcher = { path = "../fake-fetcher" }
webview-shared = { path = "../fake-shared" }
EOF

# Mirror the REAL macOS dependency block of the window crate, so a feature it
# forgot to enable shows up here rather than on the runner.
sed -n "/^\[target\.'cfg(target_os = \"macos\")'\.dependencies\]/,\$p" \
  "$REPO/crates/werust-macos/Cargo.toml" \
  | sed '/^\[dev-dependencies\]/,$d' >> "$SCRATCH/window/Cargo.toml"

mkdir -p "$SCRATCH/window/src" "$SCRATCH/window/examples"
ln -sf "$REPO/crates/werust-macos/src/lib.rs" "$SCRATCH/window/src/lib.rs"
ln -sf "$REPO/crates/werust-macos/src/paint.rs" "$SCRATCH/window/src/paint.rs"
ln -sf "$REPO/crates/werust-macos/src/window.rs" "$SCRATCH/window/src/window.rs"
ln -sf "$REPO/crates/werust-macos/examples/window_smoke.rs" \
  "$SCRATCH/window/examples/window_smoke.rs"

echo "checking the macOS backend + smoke against aarch64-apple-darwin ..."
(cd "$SCRATCH" && cargo clippy --target aarch64-apple-darwin --all-targets)

# The window's UNIT tests are deliberately NOT checked here: they assert against
# the REAL `werust-core` (which is what the Ubuntu gate runs them against), and a
# stand-in core cannot judge them. `--lib --examples` is exactly the AppKit
# surface this harness exists for.
echo "checking the macOS window + smoke against aarch64-apple-darwin ..."
(cd "$SCRATCH" && cargo clippy -p werust-macos --target aarch64-apple-darwin --lib --examples)

# The origin probe has no repo path dependencies, so it checks in place.
echo "checking the macOS origin probe against aarch64-apple-darwin ..."
(cd "$REPO" && cargo clippy -p macos-origin-probe --target aarch64-apple-darwin --all-targets)

echo
echo "OK -- the macOS sources (engine, window, probe) type-check. This is NOT a"
echo "build and it uses a STAND-IN werust-core: the real proof is"
echo ".github/workflows/macos-renderer.yml on the macos-14 runner."
