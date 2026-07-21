//! The `werust` browser binary.
//!
//! Greenfield skeleton: this only prints a startup banner and exits cleanly. The
//! `Renderer`, `Fetcher`, and `ScriptEngine` seams live in their own placeholder
//! crates (`renderer`, `native-renderer`, `fetcher`, `script-engine`) and are not
//! implemented yet — see `CONTEXT.md` and `docs/adr/0001`.

/// Builds the startup banner shown when the browser launches.
fn banner() -> String {
    format!(
        "werust {} — a Rust web browser (skeleton)",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() {
    println!("{}", banner());
}

#[cfg(test)]
mod tests {
    use super::banner;

    #[test]
    fn banner_names_werust() {
        assert!(banner().starts_with("werust "));
    }
}
