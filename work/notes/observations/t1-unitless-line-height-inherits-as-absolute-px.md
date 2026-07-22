# T1 cascade: unitless `line-height` inherits as an absolute px, not as a multiplier

2026-07-22 — Noticed while authoring the T1 server-floor golden fixtures (`t1-server-web-floor-article-and-blog`).

In `crates/native-renderer/src/css.rs`, a unitless `line-height` (e.g. `body { line-height: 1.5 }`) is resolved to an absolute px against the element's own font-size at cascade time (`Length::Em(number)` -> `resolve(font_size)`), then stored + inherited as that absolute px. Per CSS, a unitless `line-height` should inherit the *multiplier* so each descendant recomputes against its own font-size. Observed effect: with `body { font-size: 16px; line-height: 1.5 }`, an `<h1>` at `font-size: 28px` still gets `line_height = 24px` (1.5 * 16), not `42px`. Every run in a document with a unitless body line-height gets the same absolute line height regardless of its font-size (probed on both fixtures).

Out of scope for the floor task (which pins pages + goldens, not the cascade); the fixtures avoid relying on this by leaving `line-height` unset (so lines use the font-size-relative `normal`). Recording so the signal is not lost. Likely a small fix in the T1 core-CSS cascade: carry unitless line-height as an unresolved multiplier through inheritance.
