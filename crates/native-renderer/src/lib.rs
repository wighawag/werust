//! The native Rust renderer: a `Renderer`-seam backend that renders the fixed v0
//! subset in-process — the conformance ladder's **T0** floor
//! (`docs/conformance-tiers.md`, `CONTEXT.md`).
//!
//! This is werust's SECOND rendering backend, beside the WebKitGTK webview, and
//! the first that renders in-process rather than delegating to a system engine. It
//! plugs into the SAME [`Renderer`](renderer::Renderer) seam the webview uses, so
//! the two are hot-swappable, and it is held to the SAME trust-hook qualification
//! gate ([`renderer::qualify`]).
//!
//! # What T0 is (and is not)
//!
//! T0 is a deliberately small, FIXED subset, not a real browser: a naive subset
//! tokenizer, an allowlist tree builder, a real cascade over a handful of
//! properties, block/inline normal-flow layout, and software text. It renders the
//! v0 element/property allowlist, NOT arbitrary documents — that is T1, which
//! swaps html5ever in behind the `Tokenizer | TreeBuilder` seam this crate defines
//! (task `t1-whatwg-parser-html5ever-behind-tokenizer-seam`). T0 is the anchor the
//! higher tiers extend, matching the v0 subset the wezig Zig arm already reached.
//!
//! # The pipeline, stage by stage (each behind a seam)
//!
//! A load runs one pass:
//!
//! 1. **Tokenize** — [`SubsetTokenizer`] over the [`Tokenizer`] seam turns source
//!    into a flat [`Token`](tokenizer::Token) stream.
//! 2. **Tree** — [`AllowlistTreeBuilder`] over the [`TreeBuilder`] seam builds an
//!    allowlist [`Dom`](tree::Dom), dropping anything off the v0 element allowlist.
//! 3. **Cascade** — [`css::cascade`] resolves each element's
//!    [`ComputedStyle`](css::ComputedStyle) over the small T0 property set
//!    (UA sheet + author `<style>` rules by specificity/order + inline `style`).
//! 4. **Layout** — [`layout::layout`] flows the styled tree into positioned
//!    [`TextRun`](layout::TextRun)s under block/inline normal flow.
//! 5. **Paint** — [`paint::paint`] rasterizes the runs into an in-memory software
//!    [`Surface`](paint::Surface) (the software-text stage).
//!
//! The [`Tokenizer`] and [`TreeBuilder`] together are the swap seam T1 grows by:
//! everything downstream consumes the [`Dom`](tree::Dom), so replacing the T0
//! front-end does not touch cascade/layout/paint.
//!
//! # Backend + trust hooks
//!
//! [`NativeRenderer`] wires that pipeline behind the [`Renderer`](renderer::Renderer)
//! seam. It renders self-contained `data:text/html,…` documents (T0 has no network
//! yet — fetching is the `Fetcher` / ipfs tasks' job) and declares its trust hooks
//! HONESTLY as [`none`](renderer::TrustHooks::none): it wires neither provider
//! injection nor `ipfs://` resolution, so [`renderer::qualify`] legitimately
//! reports it as not-yet-qualifying rather than being rubber-stamped by the
//! fail-open default (see `NativeRenderer`'s docs and `docs/adr/0001`).

pub mod backend;
pub mod css;
pub mod layout;
pub mod paint;
pub mod pipeline;
pub mod tokenizer;
pub mod tree;

pub use backend::NativeRenderer;
pub use pipeline::{render_with, RenderOutput, DEFAULT_VIEWPORT_WIDTH};
pub use tokenizer::{SubsetTokenizer, Token, Tokenizer};
pub use tree::{AllowlistTreeBuilder, Dom, Element, Node, TreeBuilder};

/// Returns the backend's name — the stable identifier for the T0 native backend.
#[must_use]
pub fn backend_name() -> &'static str {
    "native-renderer"
}

#[cfg(test)]
mod tests {
    use super::backend_name;

    #[test]
    fn backend_name_is_native_renderer() {
        assert_eq!(backend_name(), "native-renderer");
    }
}
