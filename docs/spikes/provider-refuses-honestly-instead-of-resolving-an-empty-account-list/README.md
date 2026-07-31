# Spike: the injected provider refuses honestly instead of resolving an empty account list

Durable design record for task `provider-refuses-honestly-instead-of-resolving-an-empty-account-list`. Triggered by the human testing `mandalas.eth` against the shipped v0.3.0 build: werust's injected `window.ethereum` "seems to be causing confusion" for the dapp.

Supersedes, in part, the provider defaults recorded in `docs/spikes/eip1193-provider-injection-via-script-bridge/README.md`.

## The defect

`ProviderBridge::answer` answered `eth_accounts` and `eth_requestAccounts` identically, with `Ok(json!([]))`. Every dapp writes:

```js
const [account] = await ethereum.request({ method: 'eth_requestAccounts' })
```

Because the Promise RESOLVES, the dapp takes its SUCCESS path with `account === undefined` — it believes it is connected to nobody, reads `chainId: 0x1`, and proceeds. That is worse than a clean failure: every dapp has an error path for "connect failed", and none has one for "connected to nobody". Resolving an empty array is also not an outcome `eth_requestAccounts` is specified to have; it either yields accounts or fails.

## What changed

- **`crates/werust-core/src/provider.rs` — the two account methods now DIFFER.**
  - `eth_accounts` -> `[]`, unchanged and correct. It is a passive READ of the authorised accounts; there are none, and an empty array is the conformant way to say so.
  - `eth_requestAccounts` -> `Err(ProviderError::unauthorized())`, EIP-1193 **4100 Unauthorized**, carrying the user-visible message: *"werust does not have a wallet yet: it gives this page a read-only Ethereum connection and holds no keys, so there is no account it can authorise."* Dapps commonly render `error.message`, so it is a legible sentence — no code name, no jargon, no stack.
- **`crates/werust-core/src/ethereum.rs` — one chain constant, in the module that owns the chain.** `STUB_CHAIN_ID` is gone. `ethereum::CHAIN_ID` is now the ONE place werust states which chain it is on, and `provider::CHAIN_ID` is a `pub use` re-export of it (one value, two paths, no second constant to drift). Both trust-hook smokes compare against the `werust_core::provider::CHAIN_ID` SYMBOL rather than a literal, so against the REAL core they follow the rename with no behaviour change.
- **`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` — the rename did NOT follow automatically everywhere, and this record said it did.** See the section below; the claim above holds only against the real core, and a first version of this record stated it without that qualifier. That overstatement is exactly how the breakage reached review.
- **Unchanged, deliberately:** the `Renderer` trust-hook bar (`crates/renderer/src/lib.rs`), the `eip1193-provider` parity row (all five cells stay `implemented`), and the shim itself. No part of EIP-6963 is implemented here.

## Tests (the proof)

In `crates/werust-core/src/provider.rs`:

- `request_accounts_is_refused_with_4100_rather_than_resolving_empty` — rejects, code `4100`, and the message is a legible user-facing sentence (mentions the missing wallet, the read-only connection, no keys) with no code name or `EIP` jargon in it.
- `reading_accounts_and_asking_for_accounts_now_differ` — the acceptance criterion stated directly: the READ resolves `[]`, the ASK errors.
- `accounts_reports_no_authorised_accounts` — `eth_accounts -> []` still holds.
- `handle_pushes_the_account_refusal_back_to_the_page` — end to end: the refusal reaches the page as a `__reject(6, { code: 4100, message: … })` push settling the correlated Promise.
- `the_chain_the_page_is_told_is_the_chain_the_backend_reads` — `provider::CHAIN_ID == ethereum::CHAIN_ID`, and the answer to `eth_chainId` is that value: the guard against re-minting a second chain constant here.

In `crates/werust-core/src/ethereum.rs`: `the_chain_werust_reads_is_ethereum_mainnet` pins `CHAIN_ID` to `0x1` beside the mainnet ENS registry address that causes it.

`cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test` green.

## Decisions

- **4100 Unauthorized, not 4001 User Rejected Request.** `4100` is "the requested method and/or account has not been authorized by the user", which is exactly true here, and EIP-1193 explicitly recommends `4100` for an authorization failure ("If the Provider implements any kind of authorization logic, the authors recommend rejecting with a `4100` error"). **Alternative considered: `4001`.** It gives a smoother dapp UX, because many dapps special-case it and silence it as a quiet "cancelled" rather than showing a banner. Rejected: the user rejected nothing, and this project does not tell the page small lies. **Accepted cost, recorded so nobody rediscovers it as a bug:** with `4100`, some dapps will surface a generic error rather than cancelling quietly. TOUCHES: any later wallet work — when a real signer exists, a genuine user refusal becomes `4001`, and the two codes will then mean different things (no wallet vs. wallet said no), which is precisely why `4001` must not be spent now. Documented at the choice site (`ProviderError::unauthorized`).
- **Policy (a) — keep injecting `window.ethereum` and make every path honest — chosen by the human over (b).** (a) keeps the injection: it is one of werust's two TRUST HOOKS (`TrustHook::ProviderInjection`, the `Renderer` qualification bar), asserted by the macOS and Windows trust-hook smokes and marked `implemented` in all five cells of the `eip1193-provider` parity row, and a provider that refuses honestly is strictly better for a dapp than no provider at all. **(b), DEFERRED and tracked separately:** stop squatting `window.ethereum` and announce via EIP-6963 only once a real signer exists. werust implements NO EIP-6963 today (no `announceProvider`, no `eip6963:requestProvider` listener), so legacy detection currently says "a wallet is here" while 6963 enumeration says "no wallets" — the two contradict each other. Moving to (b) changes what the trust hook MEANS, so it needs an **ADR-0001 amendment**, not a code change. Tracked as `provider-eip6963-announcement-and-the-window-ethereum-namespace`; NO part of it is implemented here. **Revisit trigger:** a real signer landing, or the 6963/legacy contradiction being observed to break a dapp. TOUCHES: `docs/adr/0001`, the `Renderer` trust-hook definition, the parity row. Documented at the choice site (the `provider` module doc).
- **The chain id is stated once in `ethereum.rs`, not derived from the RPC endpoint.** `STUB_CHAIN_ID` was renamed and moved rather than made dynamic. **Why not derive it from `WERUST_RPC_URL` / `DEFAULT_RPC_ENDPOINT`:** an endpoint URL carries no chain identity — it says WHICH server serves the chain, not WHICH chain werust is on. **Alternative considered: ask the endpoint (`eth_chainId`) at session construction.** Rejected: it makes what the browser tells a page depend on network reachability, and leaves no honest answer when the endpoint is unreachable; it would also turn a compile-time constant into a runtime value the two trust-hook smokes (and five backends) currently compare against symbolically, trading real qualification evidence for a value that cannot change today anyway. **Why `0x1` is CORRECT and not provisional:** ENS resolution calls the MAINNET ENS registry deployment at its fixed address (`ens::REGISTRY_ADDRESS`), so werust is wired to mainnet by construction — an endpoint serving another chain does not reconfigure the browser, it just makes every `.eth` lookup miss. TOUCHES: `provider::CHAIN_ID` (a re-export, so it follows automatically), both trust-hook smokes and the webview seam test (updated to the new symbol), the macOS type-check harness's hand-written stand-in core (which does NOT follow automatically — see the section below), and any future chain SELECTOR, which arrives with the deferred wallet model and becomes this constant's source. Documented at the choice site (`ethereum::CHAIN_ID`).

## The `connect` event: answered, not fixed

Nothing native ever calls the shim's `emit`, so `connect` is never fired. EIP-1193 makes that a **MUST** (not a SHOULD, as the task assumed), yet it is inert for a provider in this state: the shim is injected at DOCUMENT START, so an emission at install reaches an empty listener map, and emitting later would be a guess at when listeners appeared; the mainstream dapp libraries call `request(...)` directly rather than waiting on `connect`; and the other three events describe changes (`chainChanged`, `accountsChanged`, `disconnect`) a keyless read-only provider on one fixed chain cannot have. Firing it anyway would also risk the same class of small lie this task removes, since dapp authors routinely read `connect` as "a wallet connected". Not fixed: neither clearly correct nor small. Full reasoning, the spec citation and the revisit trigger: `work/notes/findings/eip1193-connect-event-never-emitted-does-not-matter-yet-2026-07-31.md`, plus a summary at the choice site in `provider_shim`'s doc.

## The rename had a THIRD consumer: the macOS type-check harness's stand-in core

This record originally claimed that both trust-hook smokes "follow the rename automatically" because they compare against the symbol. **That is true only against the REAL `werust-core`, and stating it unqualified is how this change reached review broken.** There is a third consumer, and it is a hand-written copy that follows nothing:

`docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh` builds a scratch workspace outside the repo in which `werust-core` is swapped for a tiny API-compatible STAND-IN written inline in the script (the `ring` build-script obstacle documented in the script's header). It then symlinks the REAL `crates/macos-renderer/examples/trust_hooks_smoke.rs` into that workspace and runs `cargo clippy --target aarch64-apple-darwin --all-targets`. So the smoke's `werust_core::provider::CHAIN_ID` resolves against the STAND-IN, whose `provider` module still declared `STUB_CHAIN_ID`. Fixed here, one line (script line ~530).

**Why the Ubuntu gate stayed green through it.** `crates/macos-renderer/tests/typecheck_harness_guard.rs` stubs `cargo` with `exit 0`. The guard proves the harness ASSEMBLES its scratch workspace and refuses an unsafe `SCRATCH_DIR`; it cannot prove the assembly COMPILES, so a symbol rename passes it untouched. That limitation is recorded in `work/notes/observations/macos-typecheck-stand-in-core-drifts-unwatched-2026-07-31.md`, which now also carries the occurrence log this change adds to.

**Verified by RUNNING the harness, not by reading it** (`rustup target add aarch64-apple-darwin`, then the script):

- With the fix: the backend leg — `cargo clippy --target aarch64-apple-darwin --all-targets`, the leg that compiles the smoke reading `CHAIN_ID` — finishes clean.
- Falsification, to prove the one line is what does it: reverting only that line on this branch fails with `error[E0425]: cannot find value CHAIN_ID in module werust_core::provider --> examples/trust_hooks_smoke.rs:222`.
- **Still red, for a PRE-EXISTING and unrelated reason:** the second leg (`cargo clippy -p werust-macos --lib --examples`) fails with `error[E0432]: unresolved imports werust_core::trust_pin_action_label, ..._action_visible, ..._detail` from the real `crates/desktop-paint` source. Confirmed identical on a clean `origin/main` checkout at `ae3cb6f`, i.e. before this task touched anything: the stand-in was never updated when `ipns-tofu-pin-and-warn-on-change` (`e772025`) added those three functions to `werust-core`. Left alone as out of scope for this task and recorded as a third occurrence in the observation note above.

## Coherence check

No new named concept is introduced. `CHAIN_ID` replaces `STUB_CHAIN_ID` in place (one symbol renamed and re-homed, re-exported so every existing consumer path still resolves) and does not collide with any other constant; `ProviderError::unauthorized` sits beside the existing `unsupported` / `parse_error` constructors, at the same layer, named for the EIP-1193 error it carries. "Trust hook", "EIP-1193 provider" and "script-message bridge" are used in their existing `CONTEXT.md` senses, and the trust-hook bar itself is untouched — the change is what the provider SAYS, not what a backend must DO to qualify.
