//! A live, on-a-display proof that the WebKitGTK backend navigates and shows a
//! page THROUGH the [`Renderer`] seam.
//!
//! Run it on a Linux desktop session (needs a display):
//!
//! ```sh
//! cargo run -p webview-renderer --example navigate_and_show -- https://example.com/
//! ```
//!
//! It drives the backend only through the `dyn Renderer` seam: it navigates to
//! the URL, embeds the live view in a window, and drains
//! [`LoadEvent`](renderer::LoadEvent)s off the seam on the GTK loop, printing the
//! load-lifecycle transitions and quitting once the load reaches
//! [`LoadState::Finished`](renderer::LoadState::Finished) or
//! [`Failed`](renderer::LoadState::Failed). Reaching `Finished` with the window
//! showing the page is the acceptance-criterion evidence that a real page is
//! rendered by the system webview behind the seam.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use gtk4::{glib, Application, ApplicationWindow, Widget};

use renderer::{LoadEvent, LoadState, Renderer};
use webview_renderer::WebViewRenderer;

fn main() -> glib::ExitCode {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://example.com/".into());

    let app = Application::builder()
        .application_id("com.github.wighawag.werust.example")
        .build();

    app.connect_activate(move |app| {
        let renderer: Rc<RefCell<Box<dyn Renderer>>> = Rc::new(RefCell::new(Box::new(
            WebViewRenderer::new().expect("webview backend"),
        )));

        let handle = renderer.borrow().view_handle();
        // SAFETY: an embeddable, borrowed GtkWidget pointer from the seam.
        let view: Widget = unsafe { glib::translate::from_glib_none(handle.0 as *mut _) };

        let window = ApplicationWindow::builder()
            .application(app)
            .default_width(1024)
            .default_height(768)
            .title("werust — navigate_and_show")
            .child(&view)
            .build();
        window.present();

        renderer.borrow_mut().navigate(&url).expect("navigate");

        // Drain load-lifecycle events off the seam on the GTK loop and report the
        // transitions; quit once the load settles.
        let app = app.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
            let mut r = renderer.borrow_mut();
            while let Some(event) = r.poll_event() {
                match &event {
                    LoadEvent::Started { url } => println!("SEAM started: {url}"),
                    LoadEvent::Committed { url } => println!("SEAM committed: {url}"),
                    LoadEvent::Finished { url } => println!("SEAM finished: {url}"),
                    LoadEvent::Failed { url, reason } => {
                        println!("SEAM failed: {url}: {reason}");
                    }
                }
            }
            match r.load_state() {
                LoadState::Finished => {
                    println!("SEAM load reached Finished — page shown via the seam.");
                    app.quit();
                    glib::ControlFlow::Break
                }
                LoadState::Failed => {
                    println!("SEAM load Failed.");
                    app.quit();
                    glib::ControlFlow::Break
                }
                _ => glib::ControlFlow::Continue,
            }
        });
    });

    app.run_with_args::<&str>(&[])
}
