//! The native Rust renderer: a `Renderer`-seam backend assembled from the mature
//! pure-Rust stack (html5ever, stylo, taffy, parley/cosmic-text, vello+wgpu).
//!
//! This is a placeholder crate reserving the home for the from-scratch native
//! renderer that grows behind the `Renderer` seam ("webview now, native later").
//! Nothing is implemented here yet — see `CONTEXT.md` and `docs/adr/0001`.

/// Returns the backend's name. A trivial anchor until the native renderer lands.
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
