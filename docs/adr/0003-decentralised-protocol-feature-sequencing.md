# Decentralised-protocol feature sequencing: framework before dependents, privacy co-designed with subsystems, fingerprinting after no-leak

Context: werust is growing a family of decentralised-web / privacy features (trustless ENS→IPFS via a light client, an embedded Freenet node, SOCKS5h/Tor privacy routing + profiles, Tor-Browser-grade fingerprinting resistance), each captured as a spec under `work/specs/`. We decided the ORDERING CONSTRAINTS between them are architectural, not mere preference, and record them here rather than in a standalone roadmap doc (which would be an un-governed orphan outside the `work/` contract, and would drift from the specs' own `taskedAfter` fields).

The decisions:

1. **The subsystem framework (`gated-protocol-subsystems-consent-and-lazy-activation`) is tasked/built before the heavy backends that plug into it.** Helios (the ENS light client), the embedded Freenet node, and embedded Tor are all `Subsystem`s under that framework (consent + lazy activation + provider mode: embedded/external/gateway). Building it first avoids retrofitting consent/lifecycle/config onto each backend. Encoded as `taskedAfter: [gated-protocol-subsystems-...]` on `embedded-freenet-node-and-scheme` and `privacy-routing-...`.

2. **Privacy routing is co-designed with the subsystem framework, not bolted on.** A private profile (SOCKS5h/Tor) imposes a hard constraint on every subsystem's provider mode: a subsystem that cannot route its own network egress through the active transport must be DISABLED in that profile, never allowed to leak directly (an embedded Freenet/IPFS node or a light-client RPC dialling directly while the webview is Tor'd would deanonymise the user). Hence `privacy-routing` also sits `taskedAfter` the subsystem framework, and the two specs cross-reference each other.

3. **Fingerprinting resistance is sequenced after no-leak routing** (`taskedAfter: [privacy-routing-...]`). They are independent concerns — no-leak = where bytes go; fingerprinting = what the page can observe — and shipping no-leak first is right, but the no-leak work must be built fingerprinting-AWARE (uniform headers, the single bundled font, per-profile isolation) so the follow-on needs no rework. werust promises NO NETWORK LEAK before it promises UNLINKABILITY, and must never imply the latter before it ships.

4. **The trustless-ENS Phase 1 (trusted-RPC skeleton) has no prerequisite** (`taskedAfter: []`): it delivers the `ronan.eth` win self-contained (the RPC skeleton is a cheap, always-on provider), before the framework or the light client exist. It is its own spec `ens-to-ipfs-resolution-phase1-rpc-skeleton` (fully answered, taskable now); its Phase 2-3 follow-on `trustless-ens-to-ipfs-phase2-3-helios-and-hardening` (Helios + IPNS + CCIP-Read) is `taskedAfter` it and stays `needsAnswers` (checkpoint/bootstrap decisions).

Consequences:
- Sequencing lives in each spec's `taskedAfter` frontmatter (resolved against `work/specs/tasked/` residence, per the `work/` contract), NOT in a separate roadmap file. The mutable "what to task next" priority is a human choice at tasking time from the ready pool, deliberately not a tracked artifact.
- These are constraints, not a fixed schedule: priorities can reorder freely as long as the `taskedAfter` arrows hold.
