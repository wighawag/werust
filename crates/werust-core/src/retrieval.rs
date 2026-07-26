//! The user-choosable IPFS **retrieval backend** setting: the selector,
//! its persistence, and the internal `werust://settings` page that surfaces it.
//!
//! This module is the toolkit-free heart of the retrieval-backend selector
//! (`retrieval-backend-user-setting`, spec
//! `ens-to-ipfs-resolution-phase1-rpc-skeleton`): it lets the user CHOOSE how
//! werust fetches content-addressed (`ipfs://`) content over the
//! [`ContentRetriever`](fetcher::ContentRetriever) seam the blocking task built.
//! The seam already makes the backend a swap (`DEFAULT_*` + `with_gateway()`,
//! no config subsystem); this module is the user-facing selection, its
//! persistence, and the honest privacy/trust framing (`docs/adr/0001`: a trust
//! choice is a product surface, not a silent internal).
//!
//! It is split so the whole choose -> persist -> switch-the-load-path story is
//! testable WITHOUT a webview, a GTK loop, or the live network, mirroring the
//! [`ipfs`](crate::ipfs) / [`provider`](crate::provider) splits:
//!
//! * [`RetrievalBackendChoice`] is the pure selection value (the default
//!   trustless gateway, a custom gateway/local-node URL, or a not-yet-available
//!   backend), plus [`gateway_endpoint`](RetrievalBackendChoice::gateway_endpoint)
//!   which turns a selectable choice into the gateway base URL the
//!   [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever) is
//!   pointed at — this is the seam swap the choice drives.
//! * [`RetrievalSettings`] is the minimal isolated persistence: it loads/saves a
//!   single small JSON file. Production resolves its directory via the
//!   [`WERUST_SETTINGS_DIR`](SETTINGS_DIR_ENV) lever (the explicit-override seam);
//!   the directory-taking [`load_from`](RetrievalSettings::load_from) /
//!   [`save_to`](RetrievalSettings::save_to) cores let tests isolate persistence
//!   to a scratch directory WITHOUT mutating process-global env. NOT a config
//!   subsystem.
//! * [`settings_page_html`] / [`apply_settings_request`] are the internal
//!   `werust://settings` page: a self-contained HTML page listing the options
//!   with their privacy/trust framing, and the GET handler that persists a
//!   `werust://settings?backend=<kind>[&url=<endpoint>]` selection and re-renders.
//!
//! The concrete wiring — each backend's `install_ipfs` building its
//! [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever) from
//! the persisted choice, and the OS edge serving the `werust://settings` page —
//! lives where the backends live; this module owns the pure logic they delegate
//! to, exercised headlessly by its tests.
//!
//! # Privacy + trust honesty (surfaced, not hidden)
//!
//! The retrieval backend is an EGRESS + trust choice: a public trustless gateway
//! needs no node but SEES every site the user visits (it is a third party the
//! request goes to); a custom/local node keeps that private and self-trusted. The
//! per-block hash verify above the gateway (the seam's whole point) means a
//! gateway cannot serve unverified bytes, but it still observes WHICH content you
//! ask for. The `werust://settings` page states this plainly so the trade-off is
//! legible. The Phase-1/dev default is the labelled public gateway for
//! convenience; the SHIPPED final-release default is a separate release-gate
//! (`retrieval-default-egress-before-final-release` + an ADR) and is NOT decided
//! here.

use serde_json::{json, Value};

use fetcher::DEFAULT_TRUSTLESS_GATEWAY;
use renderer::{RendererError, SchemeRequest, SchemeResponse};

/// The internal scheme the settings page is served under: `werust`.
///
/// `werust://settings` is werust's first internal page. It is resolved through
/// the SAME custom-scheme / request-interception hook the `ipfs://` trust hook
/// uses ([`Renderer::register_scheme_handler`](renderer::Renderer::register_scheme_handler)),
/// so every OS edge (desktop WebKitGTK, iOS `WKURLSchemeHandler`, Android
/// `shouldInterceptRequest`) can serve it with no new IPC. Kept as one constant
/// so the backend that registers the handler and this module agree on the name.
pub const WERUST_SCHEME: &str = "werust";

/// The settings page host: `werust://settings`.
pub const SETTINGS_HOST: &str = "settings";

/// The environment variable that overrides the settings directory.
///
/// This is the test-isolation + explicit-override LEVER (the seam-crate ethos of
/// an explicit `with_*()` override rather than a global): when set, the
/// [`RetrievalSettings`] file is read/written under this directory instead of the
/// OS user-config dir. Tests point it at a scratch directory so they never touch
/// the real settings file (the shared-write rule), and a user/operator can
/// relocate the settings dir with it.
pub const SETTINGS_DIR_ENV: &str = "WERUST_SETTINGS_DIR";

/// The settings file name under the settings directory.
///
/// One small JSON file, one struct — NOT a config subsystem (the task's settled
/// persistence decision). A future setting reuses the SAME directory (e.g. the
/// IPNS-TOFU pin store) rather than forking a second location.
const SETTINGS_FILE: &str = "retrieval.json";

// ---------------------------------------------------------------------------
// The selection value.
// ---------------------------------------------------------------------------

/// The user's chosen IPFS retrieval backend.
///
/// Behind the [`ContentRetriever`](fetcher::ContentRetriever) seam every backend
/// is a swap; this enum is the user-facing CHOICE of which one the `ipfs://` load
/// path uses. Two variants are SELECTABLE at ship time (settled decision 3):
///
/// * [`DefaultGateway`](RetrievalBackendChoice::DefaultGateway) — the labelled
///   public trustless gateway ([`DEFAULT_TRUSTLESS_GATEWAY`]). Needs no node, but
///   is a third-party egress that sees which content you request. The Phase-1/dev
///   default.
/// * [`Custom`](RetrievalBackendChoice::Custom) — a user-supplied gateway or
///   local-node base URL (`http(s)://…`), validated before use. A local node is
///   the private, self-trusted choice.
///
/// The other two are shown on the settings page as "coming soon" and are REFUSED
/// (never silently broken) until their backends exist (Phase-2): delegated
/// routing and an embedded p2p client. They are modelled here so the page can
/// list them and the selector can give a typed not-yet-available refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetrievalBackendChoice {
    /// The default public trustless gateway ([`DEFAULT_TRUSTLESS_GATEWAY`]).
    DefaultGateway,
    /// A user-supplied gateway / local-node base URL (validated `http(s)://`).
    Custom {
        /// The validated gateway/node base URL the retriever is pointed at.
        url: String,
    },
    /// Delegated-routing backend — not yet available (Phase-2 follow-on).
    DelegatedRouting,
    /// Embedded p2p client — not yet available (Phase-2 async follow-on).
    EmbeddedP2p,
}

impl Default for RetrievalBackendChoice {
    /// The Phase-1/dev default is the labelled public trustless gateway (settled
    /// decision 4). The SHIPPED final-release default is NOT decided here — it is
    /// the release-gate `retrieval-default-egress-before-final-release`.
    fn default() -> Self {
        Self::DefaultGateway
    }
}

/// A backend option's availability, so the page and the selector agree on which
/// choices are selectable now vs shown-but-not-yet-available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendAvailability {
    /// Selectable now.
    Available,
    /// Shown as "coming soon"; selecting it is a typed refusal, never silent.
    ComingSoon,
}

/// A typed failure of parsing/validating a backend selection, each cause
/// DISTINCT so the settings page can surface a legible reason (never a silent
/// broken selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceError {
    /// The `backend` kind was not one of the known kinds.
    UnknownKind(String),
    /// A `custom` selection carried no `url` (or an empty one).
    MissingCustomUrl,
    /// A `custom` `url` was not a usable `http(s)://` gateway/node endpoint.
    InvalidCustomUrl(String),
    /// The selected backend is shown but not yet available (delegated-routing /
    /// embedded-p2p): a typed refusal, not a silent break.
    NotYetAvailable(String),
}

impl std::fmt::Display for ChoiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChoiceError::UnknownKind(k) => write!(f, "unknown retrieval backend `{k}`"),
            ChoiceError::MissingCustomUrl => {
                write!(f, "a custom retrieval backend needs a gateway/node URL")
            }
            ChoiceError::InvalidCustomUrl(u) => {
                write!(f, "invalid custom gateway/node URL (need http(s)://): {u}")
            }
            ChoiceError::NotYetAvailable(k) => {
                write!(f, "the `{k}` retrieval backend is not available yet")
            }
        }
    }
}

impl std::error::Error for ChoiceError {}

/// The stable string kind for each backend, used in the persisted JSON and the
/// `werust://settings?backend=<kind>` query.
const KIND_DEFAULT: &str = "default-gateway";
const KIND_CUSTOM: &str = "custom";
const KIND_DELEGATED: &str = "delegated-routing";
const KIND_EMBEDDED: &str = "embedded-p2p";

impl RetrievalBackendChoice {
    /// The stable kind string for this choice (the persisted + query form).
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            RetrievalBackendChoice::DefaultGateway => KIND_DEFAULT,
            RetrievalBackendChoice::Custom { .. } => KIND_CUSTOM,
            RetrievalBackendChoice::DelegatedRouting => KIND_DELEGATED,
            RetrievalBackendChoice::EmbeddedP2p => KIND_EMBEDDED,
        }
    }

    /// Whether this choice is selectable now, or shown-but-coming-soon.
    #[must_use]
    pub fn availability(&self) -> BackendAvailability {
        match self {
            RetrievalBackendChoice::DefaultGateway | RetrievalBackendChoice::Custom { .. } => {
                BackendAvailability::Available
            }
            RetrievalBackendChoice::DelegatedRouting | RetrievalBackendChoice::EmbeddedP2p => {
                BackendAvailability::ComingSoon
            }
        }
    }

    /// The trustless-gateway base URL the
    /// [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever) is
    /// pointed at for this choice, or a [`ChoiceError`] for a not-yet-available
    /// backend.
    ///
    /// This is the seam swap the choice drives: [`DefaultGateway`](RetrievalBackendChoice::DefaultGateway)
    /// yields [`DEFAULT_TRUSTLESS_GATEWAY`]; [`Custom`](RetrievalBackendChoice::Custom)
    /// yields its validated URL; a coming-soon backend is refused (its endpoint
    /// does not exist yet), so a caller never builds a broken retriever from it.
    pub fn gateway_endpoint(&self) -> Result<String, ChoiceError> {
        match self {
            RetrievalBackendChoice::DefaultGateway => Ok(DEFAULT_TRUSTLESS_GATEWAY.to_string()),
            RetrievalBackendChoice::Custom { url } => Ok(url.clone()),
            RetrievalBackendChoice::DelegatedRouting => {
                Err(ChoiceError::NotYetAvailable(KIND_DELEGATED.to_string()))
            }
            RetrievalBackendChoice::EmbeddedP2p => {
                Err(ChoiceError::NotYetAvailable(KIND_EMBEDDED.to_string()))
            }
        }
    }

    /// Build a choice from a `backend` kind + optional `url`, validating a custom
    /// URL and REFUSING a not-yet-available backend with a typed error.
    ///
    /// This is the one gate every selection path goes through (the query handler
    /// and the persistence loader), so a broken/unsupported selection can never
    /// take effect silently: a coming-soon kind is a typed
    /// [`NotYetAvailable`](ChoiceError::NotYetAvailable), a custom URL that is not
    /// an `http(s)://` origin is an [`InvalidCustomUrl`](ChoiceError::InvalidCustomUrl).
    pub fn parse(kind: &str, url: Option<&str>) -> Result<Self, ChoiceError> {
        match kind {
            KIND_DEFAULT => Ok(RetrievalBackendChoice::DefaultGateway),
            KIND_CUSTOM => {
                let raw = url.map(str::trim).unwrap_or("");
                if raw.is_empty() {
                    return Err(ChoiceError::MissingCustomUrl);
                }
                let url = validate_custom_url(raw)?;
                Ok(RetrievalBackendChoice::Custom { url })
            }
            KIND_DELEGATED => Err(ChoiceError::NotYetAvailable(KIND_DELEGATED.to_string())),
            KIND_EMBEDDED => Err(ChoiceError::NotYetAvailable(KIND_EMBEDDED.to_string())),
            other => Err(ChoiceError::UnknownKind(other.to_string())),
        }
    }
}

/// Validate a custom gateway/local-node URL: it must be an `http(s)://` origin
/// with a non-empty host, returned trimmed with any trailing `/` removed (the
/// [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever)
/// tolerates a trailing `/` too, but we normalise here so the persisted + shown
/// value is canonical).
///
/// A gateway or a local node (e.g. `http://127.0.0.1:8080`) is reached over
/// HTTP(S) — the bound [`Fetcher`](fetcher::Fetcher) only speaks `http(s)://` —
/// so any other scheme is refused rather than silently producing a retriever that
/// can never fetch. This is deliberately a cheap origin check, not a liveness
/// probe: the setting records the endpoint, and a dead endpoint surfaces as a
/// load failure at fetch time (fail-closed), not here.
fn validate_custom_url(raw: &str) -> Result<String, ChoiceError> {
    let trimmed = raw.trim();
    let (scheme, rest) = trimmed
        .split_once("://")
        .ok_or_else(|| ChoiceError::InvalidCustomUrl(raw.to_string()))?;
    let scheme_ok = scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https");
    // The host is everything up to the first `/`, `?`, or `#`; it must be
    // non-empty (a bare `http://` names no endpoint).
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if !scheme_ok || host.is_empty() {
        return Err(ChoiceError::InvalidCustomUrl(raw.to_string()));
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

// ---------------------------------------------------------------------------
// Persistence: a minimal isolated JSON settings file.
// ---------------------------------------------------------------------------

/// The persisted retrieval settings: the chosen backend, loaded from / saved to a
/// single small JSON file, isolatable via [`SETTINGS_DIR_ENV`].
///
/// This is deliberately minimal (NOT a config subsystem): one struct, one file,
/// [`load`](RetrievalSettings::load) / [`save`](RetrievalSettings::save). The
/// directory is resolved by [`settings_dir`]; a missing/corrupt file falls back
/// to the [`default`](RetrievalBackendChoice::default) choice rather than failing
/// (a fresh install has no file yet, and a corrupt file must not brick the
/// browser — it re-defaults and the user re-picks).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RetrievalSettings {
    /// The user's chosen retrieval backend.
    pub backend: RetrievalBackendChoice,
}

impl RetrievalSettings {
    /// Load the persisted settings from the settings file, or the default choice
    /// if there is no file (a fresh install), no settings directory, or the file
    /// is unreadable/corrupt.
    ///
    /// Reading is fail-SOFT (unlike a content load, which is fail-closed): a
    /// missing or corrupt settings file must not prevent the browser from
    /// starting — it re-defaults to the labelled public gateway, which the user
    /// can re-select. The settings directory is resolved by [`settings_dir`]
    /// (honouring the [`SETTINGS_DIR_ENV`] lever).
    #[must_use]
    pub fn load() -> Self {
        match settings_dir() {
            Some(dir) => Self::load_from(&dir),
            None => Self::default(),
        }
    }

    /// Load the persisted settings from a SPECIFIC directory (the directory-taking
    /// core [`load`](RetrievalSettings::load) delegates to).
    ///
    /// This is the explicit-directory seam: production resolves the directory from
    /// [`settings_dir`], and tests pass their OWN scratch directory here so they
    /// isolate persistence WITHOUT mutating process-global env (the shared-write
    /// rule, without the env-mutation UB). A missing/corrupt file re-defaults.
    #[must_use]
    pub fn load_from(dir: &std::path::Path) -> Self {
        let path = dir.join(SETTINGS_FILE);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::from_json(&text).unwrap_or_default()
    }

    /// Persist the settings to the settings file, creating the settings directory
    /// if needed. Returns `false` if there is no settings directory (an in-memory
    /// interim: the choice still takes effect this session, it just cannot
    /// persist) or the write failed.
    ///
    /// The settings directory is resolved by [`settings_dir`] (honouring the
    /// [`SETTINGS_DIR_ENV`] lever).
    pub fn save(&self) -> bool {
        match settings_dir() {
            Some(dir) => self.save_to(&dir),
            None => false,
        }
    }

    /// Persist the settings to a SPECIFIC directory (the directory-taking core
    /// [`save`](RetrievalSettings::save) delegates to), creating it if needed.
    /// Returns `false` if the write failed.
    ///
    /// The explicit-directory seam (see [`load_from`](RetrievalSettings::load_from)):
    /// tests write ONLY under their scratch directory and the real settings file is
    /// never touched, with no env mutation.
    pub fn save_to(&self, dir: &std::path::Path) -> bool {
        if std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        std::fs::write(dir.join(SETTINGS_FILE), self.to_json()).is_ok()
    }

    /// Serialize to the persisted JSON wire form.
    ///
    /// A tiny hand-built JSON object (the crate already binds `serde_json` for the
    /// FFI chrome JSON, so no new dependency): `{ "backend": "<kind>", "url": … }`
    /// where `url` is present only for a custom choice.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut obj = serde_json::Map::new();
        obj.insert("backend".into(), json!(self.backend.kind()));
        if let RetrievalBackendChoice::Custom { url } = &self.backend {
            obj.insert("url".into(), json!(url));
        }
        Value::Object(obj).to_string()
    }

    /// Parse the persisted JSON wire form back into settings, validating the
    /// stored choice through [`RetrievalBackendChoice::parse`] (so a persisted
    /// custom URL is re-validated and a persisted coming-soon kind re-defaults
    /// rather than taking effect).
    ///
    /// Returns `None` on a JSON parse error; a well-formed JSON with an
    /// unusable/unknown/coming-soon backend falls back to the default choice
    /// (via [`load`](RetrievalSettings::load)'s `unwrap_or_default`), never a
    /// broken selection.
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let kind = value.get("backend").and_then(Value::as_str)?;
        let url = value.get("url").and_then(Value::as_str);
        let backend = RetrievalBackendChoice::parse(kind, url).unwrap_or_default();
        Some(Self { backend })
    }
}

/// The settings directory: [`SETTINGS_DIR_ENV`] if set (the isolation +
/// override lever), else the OS user-config dir (`$XDG_CONFIG_HOME/werust`, or
/// `$HOME/.config/werust`), else `None` (an in-memory interim: the choice works
/// this session but cannot persist).
///
/// The OS-config resolution is done from standard env vars directly rather than
/// binding a `dirs`-style crate, to keep `werust-core` dependency-light; it
/// covers the desktop/Linux day-one target. A mobile edge that wants a
/// platform-specific location can set [`SETTINGS_DIR_ENV`] from its app sandbox
/// path before creating the session.
#[must_use]
pub fn settings_dir() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os(SETTINGS_DIR_ENV) {
        if !dir.is_empty() {
            return Some(std::path::PathBuf::from(dir));
        }
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(std::path::PathBuf::from(xdg).join("werust"));
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            return Some(
                std::path::PathBuf::from(home)
                    .join(".config")
                    .join("werust"),
            );
        }
    }
    None
}

/// The full path to the settings file, or `None` if there is no settings dir.
#[must_use]
pub fn settings_file_path() -> Option<std::path::PathBuf> {
    settings_dir().map(|dir| dir.join(SETTINGS_FILE))
}

/// The trustless-gateway base URL the `ipfs://` load path should point its
/// [`TrustlessGatewayCarRetriever`](fetcher::TrustlessGatewayCarRetriever) at,
/// derived from the PERSISTED choice (or the default if none is saved).
///
/// This is the one call every backend's `install_ipfs` makes to switch the real
/// load path onto the user's chosen backend: it loads the settings (honouring the
/// [`SETTINGS_DIR_ENV`] lever) and returns the gateway endpoint. A persisted
/// custom URL yields that URL; the default (or a re-defaulted corrupt/coming-soon
/// file) yields [`DEFAULT_TRUSTLESS_GATEWAY`]. So `TrustlessGatewayCarRetriever::
/// with_gateway(HttpFetcher::new(), &active_gateway_endpoint())` retrieves through
/// whatever backend the user picked, and the choice takes effect on the next
/// session/launch.
#[must_use]
pub fn active_gateway_endpoint() -> String {
    endpoint_of(&RetrievalSettings::load())
}

/// The active gateway endpoint from settings loaded out of a SPECIFIC directory
/// (the directory-taking core [`active_gateway_endpoint`] delegates to, so a test
/// can drive the load-path switch off its own scratch dir with no env mutation).
#[must_use]
pub fn active_gateway_endpoint_in(dir: &std::path::Path) -> String {
    endpoint_of(&RetrievalSettings::load_from(dir))
}

/// The gateway endpoint for a settings value, falling back to the default gateway
/// for a (re-defaulted) coming-soon choice.
fn endpoint_of(settings: &RetrievalSettings) -> String {
    settings
        .backend
        .gateway_endpoint()
        .unwrap_or_else(|_| DEFAULT_TRUSTLESS_GATEWAY.to_string())
}

// ---------------------------------------------------------------------------
// The internal `werust://settings` page.
// ---------------------------------------------------------------------------

/// The backend options the settings page lists, in display order, each with a
/// human label + the privacy/trust framing.
struct BackendOption {
    kind: &'static str,
    label: &'static str,
    /// The privacy/trust one-liner shown under the option (legible trade-off).
    framing: &'static str,
    availability: BackendAvailability,
    /// Whether this option needs a custom URL field (only the custom option).
    needs_url: bool,
}

fn backend_options() -> [BackendOption; 4] {
    [
        BackendOption {
            kind: KIND_DEFAULT,
            label: "Default public trustless gateway",
            framing: "Needs no node, but a public gateway is a third party that SEES which sites you visit. Content is still hash-verified, so it cannot serve you fake bytes, but it observes what you ask for.",
            availability: BackendAvailability::Available,
            needs_url: false,
        },
        BackendOption {
            kind: KIND_CUSTOM,
            label: "Custom gateway or local node URL",
            framing: "Point werust at your own gateway or a local IPFS node (http(s)://...). A local node is the private, self-trusted choice: no third party sees your browsing.",
            availability: BackendAvailability::Available,
            needs_url: true,
        },
        BackendOption {
            kind: KIND_DELEGATED,
            label: "Delegated routing (coming soon)",
            framing: "A future backend that discovers providers via delegated routing. Not available yet.",
            availability: BackendAvailability::ComingSoon,
            needs_url: false,
        },
        BackendOption {
            kind: KIND_EMBEDDED,
            label: "Embedded peer-to-peer client (coming soon)",
            framing: "A future built-in p2p client that retrieves content with no third-party gateway at all: the private default werust is working toward. Not available yet.",
            availability: BackendAvailability::ComingSoon,
            needs_url: false,
        },
    ]
}

/// HTML-escape a string for safe inclusion in the settings page (a custom URL is
/// user-supplied, so it must never break out of its attribute/text context).
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Render the `werust://settings` page for the given active [`RetrievalSettings`]
/// and an optional status line (a confirmation or a validation error from a just-
/// applied selection).
///
/// The page lists every backend option with its privacy/trust framing, marks the
/// active one, disables the coming-soon ones, and offers the custom-URL field. It
/// is a self-contained HTML document (no external assets) so it renders identically
/// on every edge, and each selection is a link/GET to
/// `werust://settings?backend=<kind>[&url=<endpoint>]` the same scheme handler
/// applies — no form-POST plumbing needed.
#[must_use]
pub fn settings_page_html(settings: &RetrievalSettings, status: Option<&str>) -> String {
    let active_kind = settings.backend.kind();
    let active_url = match &settings.backend {
        RetrievalBackendChoice::Custom { url } => url.as_str(),
        _ => "",
    };
    let active_endpoint = settings
        .backend
        .gateway_endpoint()
        .unwrap_or_else(|_| "(none: coming-soon backend)".to_string());

    let mut options_html = String::new();
    for opt in backend_options() {
        let is_active = opt.kind == active_kind;
        let active_badge = if is_active {
            " <strong>(active)</strong>"
        } else {
            ""
        };
        let control = match (opt.availability, opt.needs_url) {
            (BackendAvailability::ComingSoon, _) => {
                "<em>coming soon</em>".to_string()
            }
            (BackendAvailability::Available, false) => format!(
                "<a href=\"werust://settings?backend={kind}\">use this</a>",
                kind = opt.kind
            ),
            (BackendAvailability::Available, true) => format!(
                "<form action=\"werust://settings\" method=\"get\">\
                 <input type=\"hidden\" name=\"backend\" value=\"{kind}\">\
                 <input type=\"url\" name=\"url\" placeholder=\"http://127.0.0.1:8080\" value=\"{val}\">\
                 <button type=\"submit\">use this</button>\
                 </form>",
                kind = opt.kind,
                val = escape_html(active_url),
            ),
        };
        options_html.push_str(&format!(
            "<li><h3>{label}{active_badge}</h3><p>{framing}</p><p>{control}</p></li>",
            label = escape_html(opt.label),
            framing = escape_html(opt.framing),
        ));
    }

    let status_html = status
        .map(|s| format!("<p class=\"status\">{}</p>", escape_html(s)))
        .unwrap_or_default();

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\">\
         <title>werust settings — IPFS retrieval backend</title></head>\
         <body>\
         <h1>IPFS retrieval backend</h1>\
         <p>Choose how werust retrieves content-addressed (<code>ipfs://</code>) content. \
         This is an <strong>egress and trust choice</strong>: a public gateway is convenient \
         but sees your browsing; a local node keeps it private. Whatever you pick, every byte \
         is still hash-verified before it renders.</p>\
         {status_html}\
         <p>Active backend: <code>{active_kind}</code> — endpoint: <code>{active_endpoint}</code></p>\
         <ul>{options_html}</ul>\
         <p><small>The shipped default is still being decided (a public gateway is not an \
         acceptable silent default for a privacy browser); see the release-gate follow-on.</small></p>\
         </body></html>",
        active_kind = escape_html(active_kind),
        active_endpoint = escape_html(&active_endpoint),
    )
}

/// Parse the query string of a `werust://settings?...` URI into `(backend, url)`
/// parameters (a minimal `application/x-www-form-urlencoded` decode: `+` -> space
/// and `%XX` -> byte). Returns the raw string values; validation is
/// [`RetrievalBackendChoice::parse`]'s job.
fn parse_settings_query(uri: &str) -> (Option<String>, Option<String>) {
    let query = uri.split_once('?').map(|(_, q)| q).unwrap_or("");
    let mut backend = None;
    let mut url = None;
    for pair in query.split('&').filter(|p| !p.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = url_decode(value);
        match key {
            "backend" => backend = Some(decoded),
            "url" => url = Some(decoded),
            _ => {}
        }
    }
    (backend, url)
}

/// Minimal `x-www-form-urlencoded` value decode (`+` -> space, `%XX` -> byte).
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out.push((hi * 16 + lo) as u8);
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Handle a `werust://settings[?backend=…&url=…]` request: apply any selection
/// (persist it), then render the page.
///
/// This is the pure heart of the settings page the scheme handler delegates to,
/// split out so the whole select -> persist -> re-render loop is testable without
/// a webview. A bare `werust://settings` (no query) just renders the current
/// settings. A `?backend=…` query is parsed + validated through
/// [`RetrievalBackendChoice::parse`]: on success the new choice is persisted
/// (best-effort — an in-memory interim if there is no settings dir) and confirmed;
/// on a validation failure (unknown kind, missing/invalid custom URL, a
/// coming-soon backend) the OLD settings are kept and the typed reason is shown.
/// A non-`settings` `werust://` host is a fail-closed
/// [`RendererError::InvalidUrl`].
///
/// It reads + writes the persisted settings through [`RetrievalSettings::load`] /
/// [`save`](RetrievalSettings::save), so it honours the [`SETTINGS_DIR_ENV`]
/// isolation lever (a test never touches the real file).
pub fn apply_settings_request(request: &SchemeRequest) -> Result<SchemeResponse, RendererError> {
    match settings_dir() {
        Some(dir) => apply_settings_request_in(&dir, request),
        // No settings directory: apply for this session (in-memory) but persist
        // nothing. `save_to` on a non-existent path would fail anyway; route
        // through a throwaway path so the confirmation says "could not persist".
        None => apply_settings_request_in(std::path::Path::new(""), request),
    }
}

/// Handle a `werust://settings[?…]` request against a SPECIFIC settings directory
/// (the directory-taking core [`apply_settings_request`] delegates to).
///
/// The explicit-directory seam (see [`RetrievalSettings::load_from`]): production
/// resolves the directory from [`settings_dir`], and tests pass their own scratch
/// directory so the whole select -> persist -> re-render loop is exercised with no
/// process-global env mutation.
pub fn apply_settings_request_in(
    dir: &std::path::Path,
    request: &SchemeRequest,
) -> Result<SchemeResponse, RendererError> {
    let uri = &request.uri;
    let rest = uri
        .strip_prefix("werust://")
        .ok_or_else(|| RendererError::InvalidUrl(uri.clone()))?;
    // The host is up to the first `/`, `?`, or `#`.
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host != SETTINGS_HOST {
        return Err(RendererError::InvalidUrl(uri.clone()));
    }

    let mut settings = RetrievalSettings::load_from(dir);
    let (backend, url) = parse_settings_query(uri);
    let status = match backend {
        None => None,
        Some(kind) => match RetrievalBackendChoice::parse(&kind, url.as_deref()) {
            Ok(choice) => {
                settings.backend = choice;
                let persisted = !dir.as_os_str().is_empty() && settings.save_to(dir);
                Some(if persisted {
                    format!(
                        "Saved: retrieval backend is now `{}`.",
                        settings.backend.kind()
                    )
                } else {
                    format!(
                        "Selected `{}` for this session (could not persist: no settings directory).",
                        settings.backend.kind()
                    )
                })
            }
            Err(e) => Some(format!("Not changed: {e}")),
        },
    };

    Ok(SchemeResponse::ok(
        "text/html",
        settings_page_html(&settings, status.as_deref()).into_bytes(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory under the OS temp dir, isolated per test, that
    /// removes itself on drop — so a persistence test writes ONLY here, never the
    /// real settings file (the shared-write rule), with no `tempfile` dependency.
    ///
    /// The persistence + apply tests use the DIRECTORY-TAKING core APIs
    /// (`load_from` / `save_to` / `apply_settings_request_in` /
    /// `active_gateway_endpoint_in`) against this scratch dir, so they NEVER mutate
    /// the process-global `WERUST_SETTINGS_DIR` env (which would be a data race /
    /// UB with the other tests running in parallel) and never touch the real file.
    struct ScratchDir {
        path: std::path::PathBuf,
    }

    impl ScratchDir {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "werust-retrieval-test-{tag}-{pid}-{n}",
                pid = std::process::id(),
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    // ---- The selection value + custom-URL validation. --------------------

    #[test]
    fn the_default_choice_is_the_labelled_public_gateway() {
        let choice = RetrievalBackendChoice::default();
        assert_eq!(choice, RetrievalBackendChoice::DefaultGateway);
        // Its endpoint is the seam's default trustless gateway (the Phase-1 dev
        // default), so the load path uses the same gateway the seam ships.
        assert_eq!(
            choice.gateway_endpoint().unwrap(),
            DEFAULT_TRUSTLESS_GATEWAY
        );
    }

    #[test]
    fn a_valid_custom_url_is_accepted_normalised_and_used_as_the_endpoint() {
        // Acceptance: a custom gateway/local-node URL is validated and used as the
        // gateway endpoint the retriever is pointed at.
        let choice = RetrievalBackendChoice::parse(KIND_CUSTOM, Some("http://127.0.0.1:8080/"))
            .expect("a valid http url is accepted");
        assert_eq!(
            choice,
            RetrievalBackendChoice::Custom {
                url: "http://127.0.0.1:8080".to_string()
            },
            "the trailing slash is normalised off"
        );
        assert_eq!(choice.gateway_endpoint().unwrap(), "http://127.0.0.1:8080");
    }

    #[test]
    fn an_invalid_or_missing_custom_url_is_a_typed_refusal_never_silent() {
        // A custom selection with no URL, or a non-http(s) URL, is a distinct
        // typed error the page can surface (never a silently-broken selection).
        assert_eq!(
            RetrievalBackendChoice::parse(KIND_CUSTOM, None),
            Err(ChoiceError::MissingCustomUrl)
        );
        assert_eq!(
            RetrievalBackendChoice::parse(KIND_CUSTOM, Some("   ")),
            Err(ChoiceError::MissingCustomUrl)
        );
        for bad in ["ftp://host/", "not-a-url", "http://", "ipfs://bafycid"] {
            assert!(
                matches!(
                    RetrievalBackendChoice::parse(KIND_CUSTOM, Some(bad)),
                    Err(ChoiceError::InvalidCustomUrl(_))
                ),
                "`{bad}` must be refused as an invalid custom url"
            );
        }
    }

    #[test]
    fn a_coming_soon_backend_is_refused_not_silently_broken() {
        // Acceptance: unavailable backends are clearly not-yet-available, not
        // silently broken — selecting one is a typed NotYetAvailable, and it has
        // no endpoint to build a retriever from.
        for kind in [KIND_DELEGATED, KIND_EMBEDDED] {
            assert!(
                matches!(
                    RetrievalBackendChoice::parse(kind, None),
                    Err(ChoiceError::NotYetAvailable(_))
                ),
                "`{kind}` must be a typed not-yet-available refusal"
            );
        }
        assert!(RetrievalBackendChoice::DelegatedRouting
            .gateway_endpoint()
            .is_err());
        assert!(RetrievalBackendChoice::EmbeddedP2p
            .gateway_endpoint()
            .is_err());
    }

    #[test]
    fn an_unknown_backend_kind_is_a_typed_refusal() {
        assert_eq!(
            RetrievalBackendChoice::parse("nonsense", None),
            Err(ChoiceError::UnknownKind("nonsense".to_string()))
        );
    }

    // ---- Persistence, isolated + shared-write-safe. ----------------------

    #[test]
    fn a_custom_choice_persists_and_reloads_from_the_isolated_file() {
        // Acceptance: the choice persists across launches, and the persistence is
        // isolated to a scratch dir (the directory-taking core API) — the real
        // settings file is NEVER touched (the shared-write rule), with no env
        // mutation.
        let scratch = ScratchDir::new("persist");

        // Save a custom choice.
        let settings = RetrievalSettings {
            backend: RetrievalBackendChoice::Custom {
                url: "http://127.0.0.1:8080".to_string(),
            },
        };
        assert!(
            settings.save_to(&scratch.path),
            "the choice persists to the scratch dir"
        );

        // It wrote ONLY under the scratch dir, and the file exists there.
        let file = scratch.path.join(SETTINGS_FILE);
        assert!(file.is_file(), "the settings file is under the scratch dir");

        // A fresh load (a new "launch") reads the SAME choice back.
        let reloaded = RetrievalSettings::load_from(&scratch.path);
        assert_eq!(reloaded, settings, "the choice survives a reload");
    }

    #[test]
    fn a_missing_or_corrupt_settings_file_falls_back_to_the_default_choice() {
        // A fresh install (no file) or a corrupt file must not brick the browser:
        // it re-defaults to the labelled public gateway, never a broken selection.
        let scratch = ScratchDir::new("missing");

        // No file yet -> default.
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path),
            RetrievalSettings::default()
        );

        // A corrupt file -> default (not a panic, not a broken choice).
        std::fs::create_dir_all(&scratch.path).unwrap();
        std::fs::write(scratch.path.join(SETTINGS_FILE), b"not json {").unwrap();
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path),
            RetrievalSettings::default()
        );

        // A well-formed file naming a coming-soon backend also re-defaults (a
        // persisted choice never re-enables an unavailable backend).
        std::fs::write(
            scratch.path.join(SETTINGS_FILE),
            format!("{{\"backend\":\"{KIND_EMBEDDED}\"}}"),
        )
        .unwrap();
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path),
            RetrievalSettings::default()
        );
    }

    #[test]
    fn the_json_wire_form_round_trips() {
        // The persisted JSON round-trips for both a default and a custom choice.
        let default = RetrievalSettings::default();
        assert_eq!(
            RetrievalSettings::from_json(&default.to_json()).unwrap(),
            default
        );
        let custom = RetrievalSettings {
            backend: RetrievalBackendChoice::Custom {
                url: "https://gw.example".to_string(),
            },
        };
        assert_eq!(
            RetrievalSettings::from_json(&custom.to_json()).unwrap(),
            custom
        );
    }

    // ---- The `werust://settings` page + the apply loop. ------------------

    #[test]
    fn the_settings_page_lists_every_option_with_its_privacy_framing() {
        // Acceptance: the setting is legible about the privacy/trust trade-off,
        // and lists both selectable options plus the coming-soon ones.
        let html = settings_page_html(&RetrievalSettings::default(), None);
        // The privacy/trust framing is present and honest.
        assert!(html.to_lowercase().contains("egress"));
        assert!(html.contains("public gateway"));
        assert!(html.to_lowercase().contains("private"));
        assert!(html.to_lowercase().contains("hash-verified"));
        // Both selectable options + both coming-soon markers are listed.
        assert!(html.contains(KIND_DEFAULT));
        assert!(html.contains(KIND_CUSTOM));
        assert!(html.contains("coming soon"));
        // The default option is marked active.
        assert!(html.contains("(active)"));
    }

    #[test]
    fn a_bare_settings_request_renders_the_current_settings() {
        let scratch = ScratchDir::new("bare");
        let response = apply_settings_request_in(
            &scratch.path,
            &SchemeRequest {
                uri: "werust://settings".to_string(),
            },
        )
        .expect("the settings page renders");
        assert_eq!(response.mime_type, "text/html");
        let html = String::from_utf8(response.body).unwrap();
        assert!(html.contains("IPFS retrieval backend"));
    }

    #[test]
    fn selecting_a_custom_backend_persists_and_confirms_reloads_and_switches_the_load_path() {
        // Acceptance (the end-to-end select -> persist -> reload -> switch loop): a
        // `?backend=custom&url=…` GET validates + persists the choice and confirms
        // it; a fresh load reads it back; and the load path's gateway endpoint is
        // now the custom URL (the choice switches the ACTUAL retriever). Isolated
        // to the scratch dir, no env mutation.
        let scratch = ScratchDir::new("apply-custom");

        let response = apply_settings_request_in(
            &scratch.path,
            &SchemeRequest {
                uri: "werust://settings?backend=custom&url=http%3A%2F%2F127.0.0.1%3A8080"
                    .to_string(),
            },
        )
        .expect("the selection applies");
        let html = String::from_utf8(response.body).unwrap();
        assert!(
            html.contains("Saved:"),
            "the selection is confirmed: {html}"
        );

        // Persisted + reloadable as the custom choice.
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path).backend,
            RetrievalBackendChoice::Custom {
                url: "http://127.0.0.1:8080".to_string()
            }
        );
        // And the load path now points its retriever at the chosen endpoint, not
        // the default gateway: the choice switches the real load path.
        assert_eq!(
            active_gateway_endpoint_in(&scratch.path),
            "http://127.0.0.1:8080"
        );
        assert_ne!(
            active_gateway_endpoint_in(&scratch.path),
            DEFAULT_TRUSTLESS_GATEWAY
        );
    }

    #[test]
    fn selecting_an_invalid_custom_url_keeps_the_old_choice_and_shows_the_reason() {
        // A bad selection does NOT change the persisted choice and surfaces the
        // typed reason (never a silent broken selection).
        let scratch = ScratchDir::new("apply-bad");

        // First set a known-good custom choice.
        apply_settings_request_in(
            &scratch.path,
            &SchemeRequest {
                uri: "werust://settings?backend=custom&url=http%3A%2F%2Flocalhost%3A5001"
                    .to_string(),
            },
        )
        .unwrap();
        // Then try a bad one: it is refused, and the old choice stays persisted.
        let response = apply_settings_request_in(
            &scratch.path,
            &SchemeRequest {
                uri: "werust://settings?backend=custom&url=ftp%3A%2F%2Fnope".to_string(),
            },
        )
        .expect("the page still renders on a bad selection");
        let html = String::from_utf8(response.body).unwrap();
        assert!(
            html.contains("Not changed:"),
            "a bad selection is refused with a reason: {html}"
        );
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path).backend,
            RetrievalBackendChoice::Custom {
                url: "http://localhost:5001".to_string()
            },
            "the old choice is unchanged by a refused selection"
        );
    }

    #[test]
    fn selecting_a_coming_soon_backend_is_refused_with_a_reason() {
        // Selecting a coming-soon backend from the page is refused, not applied.
        let scratch = ScratchDir::new("apply-coming-soon");

        let response = apply_settings_request_in(
            &scratch.path,
            &SchemeRequest {
                uri: format!("werust://settings?backend={KIND_EMBEDDED}"),
            },
        )
        .expect("the page renders");
        let html = String::from_utf8(response.body).unwrap();
        assert!(
            html.contains("Not changed:"),
            "coming-soon is refused: {html}"
        );
        assert_eq!(
            RetrievalSettings::load_from(&scratch.path),
            RetrievalSettings::default(),
            "the choice is unchanged (still the default)"
        );
    }

    #[test]
    fn a_non_settings_werust_host_is_rejected() {
        // Only `werust://settings` is served; another host fails closed.
        let err = apply_settings_request(&SchemeRequest {
            uri: "werust://other".to_string(),
        })
        .expect_err("a non-settings host is rejected");
        assert!(matches!(err, RendererError::InvalidUrl(_)));
        // And a non-werust scheme is rejected too.
        assert!(matches!(
            apply_settings_request(&SchemeRequest {
                uri: "https://example.com/".to_string(),
            }),
            Err(RendererError::InvalidUrl(_))
        ));
    }

    #[test]
    fn a_custom_url_is_html_escaped_on_the_page() {
        // A user-supplied custom URL is echoed into the page's value attribute, so
        // it must be HTML-escaped (no attribute break-out).
        let settings = RetrievalSettings {
            backend: RetrievalBackendChoice::Custom {
                url: "http://x/\"><script>".to_string(),
            },
        };
        let html = settings_page_html(&settings, None);
        assert!(
            !html.contains("\"><script>"),
            "the custom url must be escaped, not injected raw"
        );
    }
}
