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
//! 1. **Parse** — a [`Parser`](parser::Parser) turns source HTML into a
//!    [`ParsedDocument`](parser::ParsedDocument): the render [`Dom`](tree::Dom)
//!    plus the document's author CSS. At T0 this is the [`SubsetParser`] (the
//!    naive [`SubsetTokenizer`] + [`AllowlistTreeBuilder`], dropping anything off
//!    the v0 allowlist); at T1 it is [`Html5everParser`], a real WHATWG parser
//!    that keeps every element.
//! 3. **Cascade** — [`css::cascade`] resolves each element's
//!    [`ComputedStyle`](css::ComputedStyle) over the small T0 property set
//!    (UA sheet + author `<style>` rules by specificity/order + inline `style`).
//! 4. **Shape + Layout** — [`layout::layout`] flows the styled tree into positioned
//!    [`TextRun`](layout::TextRun)s under block/inline normal flow, measuring each
//!    word with the real [`Shaper`](shape::Shaper) (parley) so widths + line
//!    heights come from real font metrics (T1 Latin/LTR shaping).
//! 5. **Paint** — [`paint::paint`] rasterizes the shaped runs into an in-memory
//!    software [`Surface`](paint::Surface) (the software-text stage).
//!
//! The [`Parser`](parser::Parser) seam (the whole HTML front-end — the
//! `Tokenizer | TreeBuilder` seam of the conformance ladder) is the swap point T1
//! grows by: everything downstream consumes the [`Dom`](tree::Dom), so replacing
//! the T0 subset front-end with html5ever does not touch cascade/layout/paint.
//! The T0 [`Tokenizer`] and [`TreeBuilder`] traits stay the subset
//! implementation, composed behind [`SubsetParser`].
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
pub mod benchmark;
pub mod css;
pub mod html5ever_parser;
pub mod layout;
pub mod paint;
pub mod parser;
pub mod pipeline;
pub mod shape;
pub mod tokenizer;
pub mod tree;
pub mod wpt_meter;

pub use backend::NativeRenderer;
pub use benchmark::{
    declared_candidate, score_measured_candidate, score_page_checklist, ArmSignals,
    BenchmarkReport, Candidate, CandidateReport, CandidateScoring, CapabilityScore, ChecklistPage,
    PageResult, TrustHookScore, VsWezigMeter,
};
pub use html5ever_parser::Html5everParser;
pub use parser::{ParsedDocument, Parser, SubsetParser};
pub use pipeline::{render_with, RenderOutput, DEFAULT_VIEWPORT_WIDTH};
pub use shape::{ShapedRun, Shaper};
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
