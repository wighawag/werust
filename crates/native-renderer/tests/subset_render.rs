//! Integration-level render assertion for the T0 native path.
//!
//! Unlike the per-module unit tests (which check one stage each), this drives the
//! WHOLE public surface end to end: a v0-subset document navigated through the
//! [`Renderer`] seam, rendered by the native T0 pipeline, asserted against the
//! painted software-text transcript. It is the "small render assertion" the task's
//! acceptance criteria call for, exercising the crate exactly as an external
//! consumer (the browser shell) would.

use native_renderer::{NativeRenderer, RenderOutput};
use renderer::{qualify, LoadState, Renderer, TrustHook, TrustHooks};

/// Build a `data:text/html,…` URL for a subset document (encoding just the bytes
/// the backend's decoder treats specially, plus spaces).
fn data_url(html: &str) -> String {
    let mut payload = String::new();
    for b in html.bytes() {
        match b {
            b'%' => payload.push_str("%25"),
            b'+' => payload.push_str("%2B"),
            b' ' => payload.push_str("%20"),
            _ => payload.push(b as char),
        }
    }
    format!("data:text/html,{payload}")
}

/// Render a document through the seam and return the T0 pipeline output.
fn render_through_seam(html: &str) -> RenderOutput {
    let mut backend = NativeRenderer::new();
    {
        // Drive navigation through the object-safe seam, as the shell does.
        let seam: &mut dyn Renderer = &mut backend;
        seam.navigate(&data_url(html))
            .expect("subset document navigable");
        assert_eq!(seam.load_state(), LoadState::Finished);
    }
    backend.last_render().expect("a render happened").clone()
}

#[test]
fn renders_a_full_v0_subset_document_via_the_seam() {
    let out = render_through_seam(
        "<html><head><style>.lead{color:#008000}</style></head><body>\
         <h1>Title</h1>\
         <p class=\"lead\">Intro <strong>bold</strong> and <em>italic</em> and <a>link</a>.</p>\
         <ul><li>one</li><li>two</li></ul>\
         </body></html>",
    );

    let transcript = out.surface.transcript();

    // Headings are bold (UA sheet); inline emphasis carries through the cascade;
    // the link is underlined; list items each become their own block line.
    assert!(
        transcript.contains("Title[b]"),
        "heading is bold: {transcript}"
    );
    // The `.lead` paragraph is green (#008000), so its inline emphasis carries the
    // inherited colour in the transcript alongside its style mark; the link keeps
    // the UA link blue (#0000ee).
    assert!(
        transcript.contains("bold[b#008000]"),
        "strong bold: {transcript}"
    );
    assert!(
        transcript.contains("italic[i#008000]"),
        "em italic: {transcript}"
    );
    assert!(
        transcript.contains("link[u#0000ee]"),
        "a underlined: {transcript}"
    );
    assert!(transcript.contains("one"));
    assert!(transcript.contains("two"));

    // The author `.lead` rule coloured the intro paragraph green (#008000).
    let intro = out
        .layout
        .runs
        .iter()
        .find(|r| r.text.contains("Intro"))
        .expect("the intro run");
    assert_eq!(
        intro.style.color,
        native_renderer::css::Color { r: 0, g: 128, b: 0 }
    );

    // A non-trivial software surface was painted (positioned, coloured cells).
    assert!(out.surface.width > 0 && out.surface.height > 0);
    let painted_non_white = (0..out.surface.height).any(|y| {
        (0..out.surface.width).any(|x| out.surface.pixel(x, y) != Some([255, 255, 255, 255]))
    });
    assert!(
        painted_non_white,
        "the subset document rasterized to pixels"
    );
}

#[test]
fn native_backend_is_held_to_the_same_trust_hook_gate_as_the_webview() {
    // Same seam, same gate: the T0 backend HONESTLY declares no trust hook, so the
    // shared `qualify` gate reports it as not-yet-qualifying (naming both missing
    // hooks) rather than fail-open rubber-stamping it.
    let backend = NativeRenderer::new();
    assert_eq!(backend.trust_hooks(), TrustHooks::none());
    let err = qualify(&backend).expect_err("a fixed-subset renderer does not yet qualify");
    assert_eq!(
        err.missing,
        vec![TrustHook::ProviderInjection, TrustHook::IpfsScheme]
    );
}

#[test]
fn native_and_webview_are_hot_swappable_behind_one_seam() {
    // The shell holds `dyn Renderer`; the T0 backend slots in exactly where the
    // webview would. Prove it drives a full load lifecycle through the boxed seam.
    let mut renderer: Box<dyn Renderer> = Box::new(NativeRenderer::new());
    renderer
        .navigate(&data_url("<p>swappable</p>"))
        .expect("navigate through the boxed seam");
    assert_eq!(renderer.load_state(), LoadState::Finished);
    assert!(renderer.current_url().is_some());
}
