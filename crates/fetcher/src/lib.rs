//! The `Fetcher` seam: the networking interface.
//!
//! This is a placeholder crate reserving the home for the `Fetcher` seam — a
//! vetted HTTP+TLS stack (rustls or bound libcurl; NEVER a hand-written TLS)
//! plus a hash-verified content-addressed fetch path. Not implemented here —
//! see `CONTEXT.md` and `docs/adr/0001`.

/// Returns the seam's crate name. A trivial anchor until the real `Fetcher`
/// trait lands.
#[must_use]
pub fn seam_name() -> &'static str {
    "fetcher"
}

#[cfg(test)]
mod tests {
    use super::seam_name;

    #[test]
    fn seam_name_is_fetcher() {
        assert_eq!(seam_name(), "fetcher");
    }
}
