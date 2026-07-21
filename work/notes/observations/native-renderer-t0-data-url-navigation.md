# T0 native backend navigates only `data:text/html,…` (no network yet)

Date: 2026-07-21
Task: native-renderer-t0-subset-path-behind-seam

## Decision (durable record, linked from the done record)

The T0 `NativeRenderer::navigate` accepts ONLY self-contained `data:text/html,<percent-encoded html>` document URLs and REJECTS every fetch-requiring scheme (`http(s)://`, `ipfs://`) with `RendererError::InvalidUrl`. The seam-free `render_source(&str)` entry point renders a document string directly.

- **Why:** T0 (`docs/conformance-tiers.md`) is the render path only; it has no networking. Server-web `http(s)://` fetching is the `Fetcher` seam's job and `ipfs://` resolution is the content-addressed seam's job (separate tasks/stories 8, 9, 12). Making this backend *claim* to fetch would overlap those tasks and mis-report what T0 can actually do. `data:text/html` is the standard, self-contained way to hand a renderer a document with no fetch, so it is the honest minimal navigable input for the render path.
- **Alternatives considered:** (a) a bespoke fixture-path scheme (e.g. `fixture://name`) — rejected: invents a new concept overlapping the content-addressed resolution seam that stories 11/12 own; (b) accept `http(s)://` and stub the fetch — rejected: would fail-open-claim a capability this backend does not have, the same anti-pattern the trust-hook forward-note warns against.
- **What it touches:** the `Renderer` seam's `navigate` contract (shared with the webview backend, which accepts absolute URLs); the future stories 11/12 (T0 server-web + content-addressed floor) will extend how a T0 load gets its bytes. Those stories layer a fetch/resolution path on top; they do not need `data:` removed.
- **Choice site JSDoc:** `crates/native-renderer/src/backend.rs` (`NativeRenderer` module docs + `navigate`), where the same rationale is recorded inline.

## Related honest-declaration note

Per the task's forward-pointer, `NativeRenderer::trust_hooks()` declares `TrustHooks::none()` (not the fail-open `all()` default): the fixed-subset backend wires neither trust hook, so `renderer::qualify` legitimately reports it as not-yet-qualifying. Asserted in `backend.rs` tests and `tests/subset_render.rs`.
