//! The T0 native backend: the render pipeline wired behind the [`Renderer`] seam.
//!
//! [`NativeRenderer`] is werust's SECOND rendering backend (beside the WebKitGTK
//! webview), and the first that renders in-process rather than delegating to a
//! system engine. It plugs into the SAME [`Renderer`] seam the webview uses, so
//! the two are hot-swappable: the shell holds a `dyn Renderer` and neither the
//! shell nor the seam knows which backend is behind it.
//!
//! # What it renders, and how a load gets its bytes
//!
//! It renders the fixed v0 subset (`docs/conformance-tiers.md` T0) through the
//! [`render_with`](crate::pipeline::render_with) pipeline. At T0 there is no
//! networking yet (the server-web `http(s)://` fetch is the `Fetcher` seam's job,
//! and `ipfs://` resolution is the content-addressed seam's — separate tasks), so
//! this backend navigates only self-contained document sources it does not need a
//! network for:
//!
//! * `data:text/html,<url-encoded html>` — an inline document, the standard
//!   self-contained way to hand a renderer a document with no fetch. This is what
//!   the shell and the render tests use to exercise the T0 path today.
//! * any other scheme (including `http(s)://` and `ipfs://`) is rejected with
//!   [`RendererError::InvalidUrl`]: this backend does not fetch, and claiming to
//!   would overlap the fetcher / ipfs tasks and mis-report what T0 can do.
//!
//! [`render_source`](NativeRenderer::render_source) renders a document string
//! directly, bypassing URL handling — the seam-free entry point the pipeline tests
//! and the higher-tier fixtures drive.
//!
//! # Trust hooks: declared HONESTLY as none
//!
//! This backend is held to the SAME trust-hook qualification gate as the webview
//! (`renderer::qualify`). It renders a fixed subset; it does NOT wire EIP-1193
//! provider injection or `ipfs://` scheme resolution to real behaviour. So it
//! overrides [`trust_hooks`](Renderer::trust_hooks) to declare
//! [`TrustHooks::none()`](renderer::TrustHooks::none) — NOT the fail-open `all()`
//! default. `qualify` therefore legitimately reports it as not-yet-qualifying,
//! which is the truth: a fixed-subset renderer is not yet a full trust-carrying
//! backend. Declaring `all()` here would defeat the thesis the gate encodes (see
//! the task's forward-pointer note and `docs/adr/0001`).

use std::collections::VecDeque;

use renderer::{
    KeyEvent, LoadEvent, LoadState, PointerEvent, Renderer, RendererError, SchemeHandler,
    ScriptMessageHandler, ScrollDelta, TrustHooks, ViewHandle,
};

use crate::pipeline::{render_with, RenderOutput, DEFAULT_VIEWPORT_WIDTH};
use crate::tokenizer::SubsetTokenizer;
use crate::tree::AllowlistTreeBuilder;

/// A [`Renderer`] backed by the in-process T0 native render pipeline.
///
/// Construct with [`NativeRenderer::new`]. `navigate` renders a self-contained
/// `data:text/html,…` document synchronously (there is no network at T0) and
/// drives the same [`LoadState`]/[`LoadEvent`] lifecycle surface as any backend,
/// so the shell treats it exactly like the webview. The last render's
/// [`RenderOutput`] is available via [`last_render`](NativeRenderer::last_render)
/// for inspection and for the software surface a windowing layer would blit.
#[derive(Default)]
pub struct NativeRenderer {
    tokenizer: SubsetTokenizer,
    tree_builder: AllowlistTreeBuilder,
    viewport_width: f32,
    state: LoadState,
    url: Option<String>,
    events: VecDeque<LoadEvent>,
    last_render: Option<RenderOutput>,
}

impl NativeRenderer {
    /// Create a T0 native backend at the default viewport width.
    #[must_use]
    pub fn new() -> Self {
        NativeRenderer {
            viewport_width: DEFAULT_VIEWPORT_WIDTH,
            ..NativeRenderer::default()
        }
    }

    /// Create a T0 native backend rendering at `viewport_width` px.
    #[must_use]
    pub fn with_viewport_width(viewport_width: f32) -> Self {
        NativeRenderer {
            viewport_width,
            ..NativeRenderer::default()
        }
    }

    /// Render a document `source` directly, bypassing URL handling.
    ///
    /// This is the seam-free entry point: it runs the full T0 pipeline
    /// (tokenize → allowlist tree → cascade → flow → software text) on `source`
    /// and returns the [`RenderOutput`]. The backend also stores it as the
    /// [`last_render`](NativeRenderer::last_render); `navigate` is a thin wrapper
    /// that decodes a `data:` URL and calls this.
    pub fn render_source(&mut self, source: &str) -> &RenderOutput {
        let output = render_with(
            &self.tokenizer,
            &self.tree_builder,
            source,
            self.viewport_width,
        );
        self.last_render.insert(output)
    }

    /// The output of the most recent render, if any.
    #[must_use]
    pub fn last_render(&self) -> Option<&RenderOutput> {
        self.last_render.as_ref()
    }

    /// Drive the (synchronous) load of `url` through the lifecycle: Started →
    /// Committed → Finished on success, emitting the matching events.
    fn load(&mut self, url: &str, source: &str) {
        self.url = Some(url.to_string());
        self.state = LoadState::Started;
        self.events.push_back(LoadEvent::Started {
            url: url.to_string(),
        });
        self.render_source(source);
        self.state = LoadState::Committed;
        self.events.push_back(LoadEvent::Committed {
            url: url.to_string(),
        });
        self.state = LoadState::Finished;
        self.events.push_back(LoadEvent::Finished {
            url: url.to_string(),
        });
    }
}

/// Decode a `data:text/html,…` URL into its HTML source.
///
/// Supports the plain (percent-encoded) form `data:text/html,<encoded>`; the
/// `;base64` form is not part of the T0 subset. Returns `None` for any non-`data`
/// URL or a `data:` URL that is not `text/html`.
fn decode_data_html(url: &str) -> Option<String> {
    let rest = url.strip_prefix("data:")?;
    let (meta, payload) = rest.split_once(',')?;
    // Accept `text/html` (optionally with parameters); reject other media types.
    let media = meta.split(';').next().unwrap_or("");
    if !media.is_empty() && media != "text/html" {
        return None;
    }
    if meta.contains("base64") {
        return None;
    }
    Some(percent_decode(payload))
}

/// Percent-decode a `data:` URL payload (`%NN` → byte), treating the result as
/// UTF-8. Invalid escapes are left verbatim.
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(bytes[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Renderer for NativeRenderer {
    fn navigate(&mut self, url: &str) -> Result<(), RendererError> {
        // T0 has no network: the only navigable source is a self-contained
        // `data:text/html,…` document. Any fetch-requiring scheme is rejected —
        // fetching is the `Fetcher` / ipfs tasks' job, not this backend's.
        let Some(source) = decode_data_html(url) else {
            return Err(RendererError::InvalidUrl(url.to_string()));
        };
        self.load(url, &source);
        Ok(())
    }

    fn reload(&mut self) -> Result<(), RendererError> {
        let url = self
            .url
            .clone()
            .ok_or_else(|| RendererError::Backend("nothing to reload".into()))?;
        self.navigate(&url)
    }

    fn stop(&mut self) {
        if self.state.is_loading() {
            self.state = LoadState::Idle;
        }
    }

    fn load_state(&self) -> LoadState {
        self.state
    }

    fn current_url(&self) -> Option<String> {
        self.url.clone()
    }

    fn poll_event(&mut self) -> Option<LoadEvent> {
        self.events.pop_front()
    }

    fn view_handle(&self) -> ViewHandle {
        // The T0 backend paints into an in-memory software [`Surface`] rather than
        // owning a native window widget; there is no OS view pointer to hand out
        // yet (a windowing layer that blits the surface is a later concern). A
        // null handle signals "no embeddable native view"; the surface itself is
        // reached through [`last_render`](NativeRenderer::last_render).
        ViewHandle(std::ptr::null_mut())
    }

    fn send_pointer(&mut self, _event: PointerEvent) {
        // T0 renders a static document; there is no interactive hit-testing yet.
    }

    fn send_key(&mut self, _event: KeyEvent) {
        // T0 renders a static document; no input model yet.
    }

    fn send_scroll(&mut self, _delta: ScrollDelta) {
        // T0 produces a full-height surface; scrolling is the shell's concern.
    }

    fn set_focus(&mut self, _focused: bool) {
        // No interactive view to focus at T0.
    }

    fn register_script_message_handler(&mut self, _name: &str, _handler: ScriptMessageHandler) {
        // A structural seam method every backend has. The T0 backend runs no
        // scripts, so it never invokes the handler — and it declares no provider
        // trust hook (see `trust_hooks`), so `qualify` does not credit it for this.
    }

    fn inject_script(&mut self, _script: &str) {
        // T0 runs no scripts; injection is a no-op here (T3 wires a ScriptEngine).
    }

    fn register_scheme_handler(&mut self, _scheme: &str, _handler: SchemeHandler) {
        // A structural seam method. The T0 backend does not resolve custom schemes
        // (no networking yet), so it never invokes the handler — and it declares no
        // ipfs trust hook, so `qualify` does not credit it for this either.
    }

    fn trust_hooks(&self) -> TrustHooks {
        // HONEST declaration (task forward-pointer note, `docs/adr/0001`): the T0
        // fixed-subset backend wires NEITHER trust hook to real behaviour, so it
        // declares none rather than the fail-open `all()` default. `qualify` then
        // legitimately reports it as not-yet-qualifying — the truth for a
        // subset renderer that is not yet a full trust-carrying backend.
        TrustHooks::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use renderer::{qualify, TrustHook};

    /// A minimal v0-subset document as a `data:text/html,…` URL.
    fn data_url(html: &str) -> String {
        // Percent-encode the two characters that would break the payload for our
        // decoder's purposes is unnecessary here (the decoder passes plain bytes
        // through); we only encode spaces to exercise the decode path.
        format!("data:text/html,{}", html.replace(' ', "%20"))
    }

    #[test]
    fn navigate_renders_a_data_html_document_through_the_lifecycle() {
        let mut r = NativeRenderer::new();
        assert_eq!(r.load_state(), LoadState::Idle);

        r.navigate(&data_url("<p>hello world</p>"))
            .expect("data:text/html is navigable at T0");

        assert_eq!(r.load_state(), LoadState::Finished);
        assert_eq!(
            r.poll_event(),
            Some(LoadEvent::Started {
                url: r.current_url().unwrap()
            })
        );
        // The document was actually rendered by the native path.
        let transcript = r.last_render().unwrap().surface.transcript();
        assert!(transcript.contains("hello"));
        assert!(transcript.contains("world"));
    }

    #[test]
    fn navigate_rejects_fetch_requiring_schemes() {
        // T0 does not fetch: http(s):// and ipfs:// are rejected here (they belong
        // to the Fetcher / ipfs tasks), without starting a load.
        let mut r = NativeRenderer::new();
        for url in ["https://example.com/", "http://x/", "ipfs://cid/index.html"] {
            let err = r
                .navigate(url)
                .expect_err("fetch-requiring scheme rejected");
            assert_eq!(err, RendererError::InvalidUrl(url.to_string()));
        }
        assert_eq!(r.load_state(), LoadState::Idle);
        assert!(r.last_render().is_none());
    }

    #[test]
    fn render_source_runs_the_pipeline_directly() {
        let mut r = NativeRenderer::new();
        let out = r.render_source("<h1>Title</h1>");
        assert!(out.surface.transcript().contains("Title[b]"));
    }

    #[test]
    fn reload_re_renders_the_current_document() {
        let mut r = NativeRenderer::new();
        assert!(r.reload().is_err(), "nothing to reload before a navigate");
        r.navigate(&data_url("<p>x</p>")).unwrap();
        // Drain the first load's events.
        while r.poll_event().is_some() {}
        r.reload().expect("reload re-navigates the current url");
        assert_eq!(r.load_state(), LoadState::Finished);
    }

    #[test]
    fn stop_returns_lifecycle_to_settled_when_loading() {
        let mut r = NativeRenderer::new();
        // A synchronous load finishes immediately, so stop after it is a no-op;
        // drive the state to a loading state directly to exercise stop.
        r.state = LoadState::Started;
        r.stop();
        assert_eq!(r.load_state(), LoadState::Idle);
    }

    #[test]
    fn decodes_percent_encoded_data_url() {
        let mut r = NativeRenderer::new();
        r.navigate("data:text/html,%3Cp%3Ehi%3C%2Fp%3E").unwrap();
        assert!(r.last_render().unwrap().surface.transcript().contains("hi"));
    }

    #[test]
    fn native_backend_declares_no_trust_hooks_honestly() {
        // The T0 backend wires NEITHER trust hook, so it declares none — not the
        // fail-open all() default. This is the honest declaration the task's
        // forward-pointer note requires.
        let r = NativeRenderer::new();
        assert_eq!(r.trust_hooks(), TrustHooks::none());
    }

    #[test]
    fn native_backend_is_subject_to_and_currently_fails_the_qualification_gate() {
        // Held to the SAME gate as the webview: because it honestly declares no
        // trust hook, `qualify` reports it as not-yet-qualifying, naming BOTH
        // missing hooks. This is correct: a fixed-subset renderer is not yet a
        // full trust-carrying backend. (When it later wires the hooks, it will
        // pass the same gate with no seam change.)
        let r = NativeRenderer::new();
        let err = qualify(&r).expect_err("a subset renderer does not yet qualify");
        assert_eq!(
            err.missing,
            vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme]
        );
    }

    #[test]
    fn native_backend_plugs_into_the_renderer_seam_as_dyn() {
        // Hot-swappable: the shell holds a `dyn Renderer` and does not know which
        // backend is behind it. Prove the T0 backend satisfies that object-safe
        // seam exactly like the webview does.
        let mut backend: Box<dyn Renderer> = Box::new(NativeRenderer::new());
        backend.navigate(&data_url("<p>seam</p>")).unwrap();
        assert_eq!(backend.load_state(), LoadState::Finished);
    }
}
