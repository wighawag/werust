# parse_selector accepts malformed `.class` / `#id` selectors (2026-07-22)

In `crates/native-renderer/src/css.rs`, `parse_selector` validates a bare *type*
selector against an identifier char-set, but the `.class` and `#id` branches only
check `!is_empty()` on the stripped remainder. So `parse_selector(".a > .b")`
returns `Some(Class("a > .b"))` rather than `None` — a class selector containing a
combinator/spaces is accepted (it just never matches any element). Harmless for
the cascade today (no element has such a class), but it means `parse_selector` is
NOT a faithful "is this a supported single selector?" check. Spotted while wiring
the T0 server-floor drift guard (`is_supported_selector`), which therefore does
its own single-token validation instead of trusting `parse_selector`.
