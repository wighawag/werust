//! The `ScriptEngine` seam: the JS-runtime interface.
//!
//! This is a placeholder crate reserving the home for the `ScriptEngine` seam.
//! The plan binds a mature engine first (SpiderMonkey leant); a pure-Rust engine
//! is an aspirational later swap-in. Do NOT write a JS engine first. Not
//! implemented here — see `CONTEXT.md`.

/// Returns the seam's crate name. A trivial anchor until the real `ScriptEngine`
/// trait lands.
#[must_use]
pub fn seam_name() -> &'static str {
    "script-engine"
}

#[cfg(test)]
mod tests {
    use super::seam_name;

    #[test]
    fn seam_name_is_script_engine() {
        assert_eq!(seam_name(), "script-engine");
    }
}
