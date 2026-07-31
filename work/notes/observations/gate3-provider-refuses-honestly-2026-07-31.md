---
title: "Gate-3 verdict: provider-refuses-honestly-instead-of-resolving-an-empty-account-list (APPROVE after a requeue) — the provider stops lying to dapps"
date: 2026-07-31
status: open
reviewOf: provider-refuses-honestly-instead-of-resolving-an-empty-account-list
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged after one requeue. `eth_requestAccounts` now REFUSES instead of resolving an empty array, which is what the human's `mandalas.eth` report came down to.

## Criteria, ticked

1. **`eth_requestAccounts` rejects with 4100 and a legible message.** MET. The message is a sentence a dapp can render verbatim: *"werust does not have a wallet yet: it gives this page a read-only Ethereum connection and holds no keys, so there is no account it can authorise."* No code name, no jargon, no stack.
2. **`eth_accounts` still resolves `[]`, and the two now differ.** MET, with the distinction made explicit at the call site: the passive READ grants nothing and `[]` is its conformant answer; the ASK cannot be granted, so it fails.
3. **The 4100-over-4001 trade-off recorded.** MET.
4. **The chain id comes from one source.** MET, and better than the fallback I allowed: rather than merely renaming `STUB_CHAIN_ID`, it is now `pub use crate::ethereum::CHAIN_ID` — a re-export of the chain werust's ENS/RPC backend actually reads. That is the one-source rule applied properly instead of a cosmetic rename.
5. **Both trust-hook smokes still green, qualification bar unchanged.** MET, verified on real runners after merge (`macos-renderer` and `windows-renderer` both success on `main`).
6. **Policy (a) recorded with alternatives and revisit trigger.** MET; (b) is tracked as its own `needsAnswers` task.
7. **The `connect`-emission question answered in writing.** MET, and it corrected me: EIP-1193 makes `connect` a **MUST**, not the SHOULD I assumed in the task. The agent still declined to fire it, with reasoning I accept — the shim installs at document-start so an emission at install reaches an empty listener map; emitting later would be guessing when listeners appeared; mainstream libraries call `request(...)` directly; and the sibling events describe changes a keyless single-chain provider cannot have. Firing a `connect` that dapp authors routinely read as "a wallet connected" would reintroduce exactly the class of small lie this task removed. Recorded as a knowing non-conformance with a revisit trigger, which is the honest way to hold it.

## The requeue

Gate 2 blocked round 1 for a one-line, real problem I verified before acting: the `CHAIN_ID` rename broke the macOS cross-target harness, whose hand-written stand-in core still declared `STUB_CHAIN_ID` while the symlinked real smoke had moved to `CHAIN_ID`. Fixed and re-run.

## Nits — one acted on, two for the human

**Acted on: the harness is STILL red, and this is the third strike.** The stand-in also lacks `trust_pin_action_label` / `trust_pin_action_visible` / `trust_pin_detail`, which `crates/desktop-paint` imports, so the next macOS agent meets an `E0432` before doing anything. Left unrepaired as out of scope, correctly flagged. Three occurrences in two days (the `desktop-paint` extraction, this rename, and now this) is past the point of recording: cut as `typecheck-harness-standin-core-must-not-drift-again`, which fixes it AND makes drift detectable on the Ubuntu gate by symbol comparison rather than by compiling — a stubbed `cargo` can never catch it, which is why all three walked through a green gate.

**For the human:**

- **The chain-id constant rests on a claim that is not quite true.** The doc says ENS resolution targets the MAINNET registry deployment so werust is wired to mainnet by construction — but the ENS registry sits at the SAME address on Sepolia and Goerli, so a `WERUST_RPC_URL` pointed at another chain would resolve `.eth` against THAT chain's registry while the page is still told `0x1`. Mainnet-only is the declared Phase-1 scope, so this is a wording problem today and a real mismatch the moment anyone points the env var elsewhere. Reword, or make the mismatch detectable.
- **The knowing EIP-1193 non-conformance** above (never emitting `connect`, which the spec makes a MUST). Recorded with reasoning and a revisit trigger rather than silently skipped, which I think is right, but it is a deliberate spec deviation and the human should own it.
