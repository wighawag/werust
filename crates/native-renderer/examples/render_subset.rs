//! A headless proof that the native T0 backend renders the v0 subset THROUGH the
//! [`Renderer`] seam.
//!
//! Unlike the webview example this needs no display: the T0 backend paints into an
//! in-memory software [`Surface`](native_renderer::paint::Surface), so the render
//! is fully headless. Run it with:
//!
//! ```sh
//! cargo run -p native-renderer --example render_subset
//! ```
//!
//! It drives the backend only through the `dyn Renderer` seam: it navigates to a
//! self-contained `data:text/html,…` v0-subset document, drains the
//! [`LoadEvent`](renderer::LoadEvent)s off the seam, and once the load reaches
//! [`Finished`](renderer::LoadState::Finished) prints the painted software-text
//! transcript and the surface dimensions. A `Finished` load whose transcript
//! shows the styled subset content is the acceptance-criterion evidence that the
//! native path renders the v0 subset behind the same seam the webview uses.

use native_renderer::NativeRenderer;
use renderer::{LoadEvent, LoadState, Renderer};

fn main() {
    // A small v0-subset document exercising headings, paragraphs, a list, inline
    // emphasis, a link, and an author `<style>` rule — all on the allowlist.
    let html = "\
<html>\
<head><style>.lead{color:#0000ff}</style></head>\
<body>\
<h1>werust T0</h1>\
<p class=\"lead\">The <strong>native</strong> renderer draws the <em>v0 subset</em>.</p>\
<ul><li>tokenize</li><li>tree</li><li>cascade</li><li>flow</li><li>paint</li></ul>\
<p>See <a href=\"https://example.com/\">the seam</a>.</p>\
</body>\
</html>";
    let url = format!("data:text/html,{}", percent_encode(html));

    // Drive the T0 backend through the seam: `navigate`/`poll_event`/`load_state`
    // are the `Renderer` trait methods (we hold the concrete backend so we can
    // also read the painted software surface, which the opaque seam view handle
    // does not carry).
    let mut renderer = NativeRenderer::new();
    // Prove it is driven through the seam trait, not backend-specific calls.
    let via_seam: &mut dyn Renderer = &mut renderer;
    via_seam
        .navigate(&url)
        .expect("navigate the T0 subset document");

    while let Some(event) = via_seam.poll_event() {
        match &event {
            LoadEvent::Started { url } => println!("SEAM started: {url}"),
            LoadEvent::Committed { url } => println!("SEAM committed: {url}"),
            LoadEvent::Finished { .. } => println!("SEAM finished."),
            LoadEvent::Failed { url, reason } => println!("SEAM failed: {url}: {reason}"),
            LoadEvent::UrlChanged { url } => println!("SEAM url changed: {url}"),
        }
    }

    assert_eq!(via_seam.load_state(), LoadState::Finished);

    // The software surface is the native backend's paint output; the seam carries
    // only an opaque view handle, so it is read from the concrete backend.
    let out = renderer.last_render().expect("a render happened");
    println!(
        "SEAM load reached Finished — {}x{} software surface painted by the native path.",
        out.surface.width, out.surface.height
    );
    println!("--- painted software-text transcript (flow order) ---");
    println!("{}", out.surface.transcript());
}

/// Minimal percent-encoding for a `data:` payload (encodes the characters the
/// backend's decoder treats specially plus spaces).
fn percent_encode(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        match b {
            b'%' => out.push_str("%25"),
            b'+' => out.push_str("%2B"),
            b' ' => out.push_str("%20"),
            _ => out.push(b as char),
        }
    }
    out
}
