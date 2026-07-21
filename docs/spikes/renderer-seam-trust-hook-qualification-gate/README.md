# Spike: Renderer-seam trust-hook qualification gate

Durable evidence + design record for task `renderer-seam-trust-hook-qualification-gate`.

## What was built

The trust-hook qualification is now an ENFORCED, checkable property of the `Renderer` seam (`crates/renderer/src/lib.rs`), not a doc comment:

- **`TrustHook`** — the two concrete trust hooks that carry the thesis (`docs/adr/0001`, `CONTEXT.md`): `ProviderInjection` (EIP-1193 provider over the script-message bridge) and `IpfsScheme` (`ipfs://` custom-scheme resolution). `TrustHook::ALL` is the single source of truth for "which hooks qualify a backend".
- **`TrustHooks`** — the checkable capability *value* a backend declares (a set over `TrustHook`), with `all()` / `none()` / `with()` / `and()` / `contains()` / `is_qualifying()` / `missing()`.
- **`Renderer::trust_hooks(&self) -> TrustHooks`** — a required seam method (with a qualifying default) by which a backend reports the trust hooks it can actually satisfy.
- **`qualify(&dyn Renderer) -> Result<(), Disqualified>`** — the GATE: accepts a backend only if its declared `TrustHooks` cover every `TrustHook`; otherwise rejects with a `Disqualified` naming exactly the missing hooks.

The webview backend (`crates/webview-renderer`) passes the gate (it inherits the qualifying default; it wires real hook behaviour via the sibling provider/ipfs tasks). A render-only backend that declares no hook is rejected.

## Why a declared-capability value, not just the mandatory hook methods

The three hook methods (`register_script_message_handler`, `inject_script`, `register_scheme_handler`) are already mandatory on the trait, so *structural* presence is guaranteed for every `impl` — but a render-only backend can STUB them (as the webview legitimately stubs `send_pointer` etc.). Structural presence therefore does NOT distinguish "renders AND can satisfy the hooks" from "renders but cannot". The task's criterion 2 explicitly requires rejecting a backend that "renders but cannot", which is only expressible if a backend can *declare inability*. Hence a checkable capability value (`TrustHooks`) reported through `trust_hooks()` and checked by `qualify()`.

## Conformance tests (the proof)

- `crates/renderer/src/lib.rs`: `qualification_gate_accepts_a_backend_that_declares_both_trust_hooks`, `qualification_gate_rejects_a_render_only_backend` (both hooks reported missing), `qualification_gate_rejects_a_backend_missing_only_one_hook` (one hook is not enough), `trust_hooks_capability_set_reports_membership`.
- `crates/webview-renderer/src/lib.rs`: `webview_backend_passes_the_trust_hook_qualification_gate`, `webview_renderer_does_not_downgrade_its_trust_hook_capability` (guards the real backend from silently going render-only), `a_render_only_backend_on_this_seam_is_rejected`, and `real_webview_backend_qualifies` (`#[ignore]`, display-bound; run with `cargo test -p webview-renderer -- --ignored`).

All non-ignored tests run headlessly and pass under the `verify` gate (`cargo fmt --check && cargo clippy && cargo build && cargo test`).

## Decisions

- **The qualification is a declared-capability value (`TrustHooks`) checked by `qualify`, layered ON TOP of the mandatory hook methods — not a new set of methods.** Alternative considered: rely on the mandatory trait methods alone (compile-time structural). Rejected: a render-only backend can stub the methods, so structural presence cannot reject the "renders but cannot satisfy the hooks" case the task requires. Alternative considered: split the trust hooks into a separate `TrustHooks` trait a backend may or may not implement, using a trait bound as the gate. Rejected: it makes qualification a *compile-time* property, but the seam already carries `dyn Renderer` (the benchmark harness and T0 task evaluate backends as trait objects at run time), and a pass/fail *runtime* gate that names the missing hook is what those consumers need. This choice TOUCHES: the T0 native backend (`native-renderer-t0-subset-path-behind-seam` — "subject to the trust-hook qualification gate") and the benchmark harness (`native-renderer-benchmark-harness-capability-and-trust-hooks` — "trust-hook qualification (pass/fail)"), both of which reuse `qualify` / `trust_hooks`.
- **`Renderer::trust_hooks` defaults to `TrustHooks::all()` (a QUALIFYING default); a backend that renders but cannot satisfy a hook must OVERRIDE it to drop the hook.** This is a user-visible default worth recording (it sets the seam's default posture). Rationale: a backend that implements the hook methods is presumed to satisfy them; the gate's job is to reject a backend that HONESTLY reports it cannot (the render-only case), not to catch a stubbed method lying about itself (asserting real hook *behaviour* is the sibling provider/ipfs tasks' job). Alternative considered: default to `none()` (fail-closed), forcing every backend to opt in. Rejected for this slice because the only real backend today (the webview) satisfies both hooks and the sibling tasks assert the real behaviour; a fail-closed default would add ceremony without catching a real failure mode here. This is reversible: flipping the default to `none()` later only tightens the gate, and every backend's declared set is explicit at its `trust_hooks` override site. TOUCHES the same two downstream tasks above (any new backend they add inherits this default). Documented at the choice site (the `Renderer::trust_hooks` and `TrustHooks::default` doc comments in `crates/renderer/src/lib.rs`).

## Coherence check

`TrustHook` / `TrustHooks` / `qualify` reuse the existing glossary term **trust hooks** (`CONTEXT.md`, `docs/adr/0001`) and the benchmark spec's framing of a "pass/fail qualifying gate, not a graded score" (`work/specs/ready/rust-successor-native-renderer-architecture-benchmark.md`). No existing name is re-meant: `qualify` is new and sits at the seam layer (a property of any `Renderer` backend), which is where both the T0 backend task and the benchmark-harness task expect to reuse it.
