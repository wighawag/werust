//! The `Renderer` seam: the wide, hot-swappable rendering-backend interface.
//!
//! This is a placeholder crate that reserves the home for the `Renderer` seam
//! (navigate/reload/stop, live view, input forwarding, load-lifecycle events, a
//! script-message bridge for provider injection, and the `ipfs://` custom-scheme
//! hook). The seam is NOT implemented here — see the domain glossary in
//! `CONTEXT.md` and `docs/adr/0001`.

/// Returns the seam's crate name. A trivial anchor so the crate compiles and is
/// exercised by a test until the real `Renderer` trait lands.
#[must_use]
pub fn seam_name() -> &'static str {
    "renderer"
}

#[cfg(test)]
mod tests {
    use super::seam_name;

    #[test]
    fn seam_name_is_renderer() {
        assert_eq!(seam_name(), "renderer");
    }
}
