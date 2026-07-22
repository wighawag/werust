# Spike: bare `.eth` URL-bar front door (resolve + render `ronan.eth` end to end)

Durable evidence + decisions for task `bare-eth-urlbar-front-door-end-to-end` (spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`, covers stories 1–4). This closes the tracer bullet: a bare `ronan.eth` typed in the URL bar resolves over the trusted RPC and renders the immutable IPFS site through the EXISTING verified `ipfs://` path, honestly labelled "content-verified, name via trusted RPC".

## What was built

The ENS front door lives in the toolkit-free `werust-core` `BrowserShell` (`crates/werust-core/src/lib.rs`), so the SAME core backs the desktop GTK view, the Android edge, and the iOS edge:

- `eth_name_from_entry(entry) -> Option<&str>`: the URL-bar recognition of a bare `.eth` name.
- `BrowserShell::navigate` routes a recognised `.eth` entry to `navigate_ens_name`, which resolves via the existing `ens::resolve` core (unchanged), dispatches by the DECODED contenthash's own type (only `ipfs-ns` is loadable), feeds the resolved `ipfs://<cid>` into the seam's verified `ipfs://` path, flags the load ENS-originated, and keeps the `.eth` name in the bar. Every failure is fail-closed with a legible reason on `ChromeState::last_error`.
- `BrowserShell` now holds a `Box<dyn EthereumProvider>` (default trusted `RpcProvider`; `with_provider` injects a fixture in tests).
- A new `Renderer::mark_ens_origin` seam method (default no-op) + `LoadLifecycle` `ens_origin` flag (`crates/webview-renderer/src/lib.rs`) + `WebViewRenderer` forwarding (`crates/webview-renderer/src/backend.rs`).

## Reproducing

```sh
cargo test -p werust-core --lib            # front-door recognition, resolve+render, fail-closed, posture
cargo test -p webview-renderer --lib       # the ens_origin redirect mechanism + no-leak
cargo test --workspace                     # the whole gate
```

All offline: the front-door tests resolve through a pinned in-process `EthereumProvider` fixture (canned resolver-address + contenthash answers) and drive a simulated verified content path, exactly as the blocking tasks' offline harness style (`fetcher` / `werust-core::ipfs`).

## Drift re-check (per WORK-CONTRACT.md "Drift is a needs-attention signal")

Both blocking tasks are in `work/tasks/done/` and landed as this task assumed; confirmed against current source:

- `ens-namehash-registry-resolver-contenthash-resolution`: `ens::resolve(provider: &dyn EthereumProvider, name) -> Result<DecodedContenthash, ResolutionError>` with `DecodedContenthash::Ipfs { uri, cid }` (the loadable case) and a typed `ResolutionError` taxonomy (unsupported protocols already mapped to `Err(UnsupportedContenthash)`). Consumed as-is; no drift.
- `name-via-trusted-rpc-trust-state`: `TrustPosture::NameViaTrustedRpc` + `ChromeState::is_name_via_trusted_rpc()` + the desktop three-state indicator, plus the `LoadLifecycle::mark_name_via_trusted_rpc` wiring hook. Present as assumed; no drift.
- The `ipfs://` render path's `mark_content_verified` behaviour: `install_ipfs`'s scheme handler (`crates/webview-renderer/src/backend.rs`) still calls `mark_content_verified()` UNCONDITIONALLY on any verified resolution, and the shell reads posture via `refresh_chrome` -> `renderer.trust_posture()`. Confirmed; this is the clash the mechanism below resolves.

## Decisions

- **The exact `.eth` recognition rule (`eth_name_from_entry`).** A bare `.eth` entry is one that: carries NO `://` scheme (an explicit scheme is taken literally, so `ipfs://…` / `https://…eth` are never hijacked, and `ens://` is not required in Phase 1 — spec Settled decisions); ends in `.eth` (case-insensitive) after removing at most one trailing `/` (the "on Enter or a trailing `/`" half of the rule); has a non-empty label before `.eth`; and has no `/` left in the name (a path like `ronan.eth/x` is not a bare name in Phase 1 — the front door resolves a name to a CID, it does not select a sub-path via ENS). Label normalisation/validation is left to the resolver (`ens::namehash` via `ens-normalize`), so this is only cheap URL-bar recognition. **Touches:** the URL-bar Enter path only; it deliberately does NOT introduce an `ens://` scheme or auto-resolve anything merely name-ish.

- **How the `.eth` name stays in the bar while the CID loads (`url_override`).** The shell already tracks a URL-bar string distinct from the underlying load. An ENS load sets `BrowserShell.url_override = Some(name)`; `refresh_chrome` and `pump` show it in place of the backend's `ipfs://<cid>` `current_url` (the lifecycle events carry the CID, which would otherwise overwrite the bar). It is cleared by any navigation that is not the ENS front door (plain navigate / back / forward / reload), so the name never lingers on a later page. No `https://` rewrite, no gateway redirect. **Touches:** `navigate`, `navigate_ens_name`, `go_back`, `go_forward`, `reload`, `pump`, `refresh_chrome`.

- **The mechanism by which the ENS-origin posture WINS over the scheme handler's content-verified mark (the load-bearing trap).** The `ipfs://` scheme handler marks any verified resolution `mark_content_verified()` and knows nothing about ENS. Rather than teach the handler about ENS, the front door signals the ENS origin into the load path via a new seam method `Renderer::mark_ens_origin` (default no-op), which `WebViewRenderer` forwards to a `LoadLifecycle::ens_origin` flag. `mark_content_verified` then REDIRECTS: if `ens_origin` is set it surfaces `NameViaTrustedRpc`, else plain `ContentVerified`. So the handler's SAME unconditional mark yields the honest ENS posture for an ENS-originated load. It stays driven by the REAL load path (only a load whose bytes actually verify gets marked at all — a failed verify never marks), and it does NOT leak: a fresh `begin` resets the flag, so a later plain `ipfs://` or served load is untrusted/plain-verified. The shell calls `mark_ens_origin` AFTER `renderer.navigate` (which resets the flag on `begin`) and only on a genuine `ipfs-ns` resolution. **Alternatives considered:** (a) upgrade the posture shell-side after navigate — rejected: the scheme handler fires asynchronously on the GTK loop AFTER `navigate` returns, so its later `mark_content_verified` would clobber any earlier shell-side mark; (b) have `install_ipfs` check the flag and call `mark_name_via_trusted_rpc()` explicitly — equivalent, but redirecting inside `mark_content_verified` keeps the scheme handler ENS-agnostic (one mark site, no ENS branch in the handler). **Touches:** the `Renderer` seam (new default-no-op method — coherent with the seam's fail-closed default posture), `LoadLifecycle`, `WebViewRenderer`, and the `BrowserShell` front door. The pre-existing direct `LoadLifecycle::mark_name_via_trusted_rpc` from the blocking task is retained (still valid + tested) but is no longer the path the front door drives.

- **`BrowserShell::new` keeps its signature; `with_provider` is the injection seam.** `new(renderer)` now constructs the labelled-default trusted `RpcProvider` internally (mirroring `RpcProvider::new` / `GatewayContentSource::new`), so the desktop binary and both mobile edges compile unchanged; `with_provider(renderer, provider)` points the front door at a specific endpoint or an in-process fixture. **Touches:** the four `BrowserShell::new` call sites keep working; tests use `with_provider`.

- **Reload does not re-resolve the name (Phase 1).** A reload re-loads the backend's current underlying `ipfs://<cid>` and drops the pinned name, so the reloaded page shows honestly as plain content-verified. Re-resolving the name on reload is not an acceptance criterion and is deferred. **Touches:** `BrowserShell::reload` (clears `url_override`).

## Consequence for the mobile edges

The front door lives in the shared core, so Android/iOS also recognise a `.eth` entry and resolve it. Their backends do not implement `trust_posture`/`mark_ens_origin` (they inherit the seam defaults), so the ENS posture is a desktop surface in Phase 1; the mobile edges simply navigate to the resolved `ipfs://<cid>`. This is consistent with the shared-core design and needed no mobile-specific change.
