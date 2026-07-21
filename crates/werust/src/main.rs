//! The `werust` browser binary.
//!
//! Day-one path: open a URL through the [`Renderer`] seam and show the page via
//! the WebKitGTK backend (`webview-renderer`). The binary is the product shell —
//! it owns the GTK window and the main loop — but it drives rendering ONLY
//! through the `dyn Renderer` seam and never calls WebKitGTK directly; the live
//! view is embedded via the seam's opaque [`ViewHandle`]. See `CONTEXT.md` and
//! `docs/adr/0001`.

use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Widget};

use renderer::Renderer;
use webview_renderer::WebViewRenderer;

/// The URL werust opens when none is given on the command line.
const DEFAULT_URL: &str = "https://example.com/";

/// Builds the startup banner shown when the browser launches.
fn banner() -> String {
    format!(
        "werust {} — a Rust web browser (webview backend)",
        env!("CARGO_PKG_VERSION")
    )
}

/// The GTK application id for the shell window.
const APP_ID: &str = "com.github.wighawag.werust";

fn main() -> glib::ExitCode {
    println!("{}", banner());

    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_URL.into());

    let app = Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        if let Err(e) = open_window(app, &url) {
            eprintln!("werust: {e}");
        }
    });
    // Do not treat CLI args as files to open.
    app.run_with_args::<&str>(&[])
}

/// Open the shell window and navigate it to `url` through the [`Renderer`] seam.
///
/// The shell constructs the webview backend, embeds its live view via the seam's
/// opaque [`ViewHandle`] (reconstructed here as a generic `gtk4::Widget` — no
/// WebKitGTK type crosses the seam), and drives navigation through `dyn
/// Renderer`.
fn open_window(app: &Application, url: &str) -> Result<(), renderer::RendererError> {
    let mut renderer: Box<dyn Renderer> = Box::new(WebViewRenderer::new()?);

    // Embed the live, interactive view. The seam hands the shell an opaque
    // pointer to the backend's native view; the shell reconstructs it as a plain
    // GtkWidget to pack into its window without knowing it is a WebKitGTK view.
    let handle = renderer.view_handle();
    // SAFETY: `view_handle()` returns a live GtkWidget pointer owned by the
    // backend for the shell to embed; `from_glib_none` takes a borrowed ref and
    // does not consume ownership. The backend outlives the window (both are held
    // for the run of the loop below).
    let view: Widget = unsafe { glib::translate::from_glib_none(handle.0 as *mut _) };

    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(1024)
        .default_height(768)
        .title("werust")
        .child(&view)
        .build();

    renderer.navigate(url)?;

    // Keep the backend alive for as long as the window is open.
    window.connect_destroy(move |_| {
        let _ = &renderer;
    });
    window.present();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{banner, DEFAULT_URL};

    #[test]
    fn banner_names_werust() {
        assert!(banner().starts_with("werust "));
    }

    #[test]
    fn default_url_is_an_https_url() {
        assert!(DEFAULT_URL.starts_with("https://"));
    }
}
