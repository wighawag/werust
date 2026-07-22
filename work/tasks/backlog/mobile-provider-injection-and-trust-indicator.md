---
title: "Mobile EIP-1193 provider injection + trust-indicator wiring (iOS + Android): end the desktop-only silent no-ops for the provider bridge and trust posture"
slug: mobile-provider-injection-and-trust-indicator
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: [mobile-ipfs-scheme-interception-ios-and-android]
covers: []
---

## What to build

Close two more desktop-only silent no-ops the parity guard surfaced (`docs/platform-capability-matrix.toml`, `work/notes/observations/mobile-provider-and-trust-are-also-silent-no-ops-2026-07-23.md`): on BOTH mobile backends the EIP-1193 provider bridge and the trust indicator are unimplemented.

- **EIP-1193 provider injection:** on `crates/werust-android/rust/src/backend.rs` and `crates/werust-ios/rust/src/backend.rs`, `register_script_message_handler` and `inject_script` are empty `{}` no-ops. Wire them to the real OS-edge script bridge (Android `WebView` `addJavascriptInterface`/`evaluateJavascript` + a message channel; iOS WKWebView `WKScriptMessageHandler` + `WKUserScript`), routing through the SAME `werust-core` provider path desktop uses (`install_provider` / the shared script-message bridge) so the injected EIP-1193 provider behaves identically across platforms.
- **Trust indicator:** neither mobile backend overrides `trust_posture` / `trust_hooks` / `mark_ens_origin` — they inherit the seam defaults (`UnverifiedOrigin`, `TrustHooks::none()`, no-op), so the mobile chrome cannot reflect the real load posture. Wire the mobile shells to read the shared `LoadLifecycle` posture (the same source the desktop chrome reads) and render the trust indicator (content-verified / name-via-trusted-rpc / mutable-name / unverified) in the mobile chrome.

Depends on `mobile-ipfs-scheme-interception-ios-and-android` (which establishes the mobile-webview <-> core resolve wiring these both build on). After this lands, repoint the `eip1193-provider` and `trust-indicator` matrix cells from the ipfs task to THIS task (they are currently linked to the ipfs-scheme task only because it existed first).

## Acceptance criteria

- [ ] The EIP-1193 provider is injected into pages on iOS and Android via the OS-edge script bridge, routed through the SAME `werust-core` provider path as desktop; a page's `window.ethereum` works on mobile.
- [ ] The mobile backends' `register_script_message_handler` / `inject_script` are no longer empty no-ops.
- [ ] The mobile chrome renders the real trust posture (read from the shared `LoadLifecycle`, not the seam-default), matching desktop for the same load — including the `NameViaTrustedRpc` and (once it exists) `MutableName` states.
- [ ] The parity matrix's `eip1193-provider` and `trust-indicator` cells are updated to `implemented` on iOS/Android (and repointed off the ipfs task), and the guard stays green truthfully.
- [ ] Tests prove the provider bridge + trust posture reach/according-from the core on each mobile edge (as far as each harness allows), network-isolated.

## Blocked by

- Blocked by `mobile-ipfs-scheme-interception-ios-and-android` (the mobile-webview <-> `werust-core` wiring these build on).

## Prompt

> Goal: end two more desktop-only silent no-ops on mobile — the EIP-1193 provider bridge and the trust indicator — that the parity guard surfaced. Wire both mobile backends (Android + iOS) to the SAME `werust-core` provider + trust-posture paths desktop uses, so `window.ethereum` works and the trust indicator reflects the real load posture on mobile. Then flip the matrix cells to `implemented` and repoint them off the ipfs-scheme task.
>
> Where to look: `crates/werust-android/rust/src/backend.rs` + `crates/werust-ios/rust/src/backend.rs` (`register_script_message_handler` / `inject_script` are empty `{}`; `trust_posture`/`trust_hooks`/`mark_ens_origin` inherit seam defaults). The desktop precedents are `install_provider` and the shared `LoadLifecycle` posture in `crates/webview-renderer`. The OS edges are Kotlin (`crates/werust-android/app`) and Swift (`crates/werust-ios`). The matrix + guard: `docs/platform-capability-matrix.toml`, `crates/werust-core/tests/platform_capability_parity.rs`, ADR-0005. The two-axis trust model: `work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`.
>
> Done = the provider bridge + trust indicator work on iOS and Android through the shared core paths, the mobile no-ops are gone, the matrix cells are `implemented` and repointed to this task, the guard is green truthfully, and tests prove it per edge. FIRST re-check current reality (the mobile backends, the parity matrix, the mobile-ipfs task's landed wiring) and route to needs-attention on drift. RECORD any non-obvious mobile-bridge decisions durably.
