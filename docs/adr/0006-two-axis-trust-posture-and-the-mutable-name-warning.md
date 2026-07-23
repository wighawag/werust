# Two-axis trust posture, and the `MutableName` warning

Context: werust's chrome trust indicator started with three postures (`UnverifiedOrigin`, `ContentVerified`, `NameViaTrustedRpc`). Resolving a mutable IPNS name (`ipns-name-resolution-and-render`) forced the question the settled two-axis model (`work/notes/observations/trust-posture-two-axes-model-2026-07-22.md`) had already answered: content-verified bytes reached via a MUTABLE name (IPNS, or ENS) must NOT be shown as immutable `ContentVerified`, because the controller can repoint the name to different (still hash-consistent) bytes. We decided to record the model here because a `MutableName` posture that compiles but is placed wrong (or labelled "verified") is a security-relevant honesty failure, and every later name-resolution task inherits this vocabulary.

## The decision

The trust indicator communicates TWO orthogonal axes and shows the MOST IMPORTANT (loudest) applicable warning:

1. **Resolution-trust** (how the name -> CID mapping was learned): a direct `ipfs://<cid>` learns nothing (the hash IS the name); an ENS name Phase-1 learns it over a TRUSTED RPC (`NameViaTrustedRpc`); an IPNS name learns it from a client-verified signed record (no RPC-trust warning).
2. **Mutability** (can the controller repoint the name?): a direct `ipfs://<cid>` is immutable; an ENS name (owner `setContenthash`) and an IPNS name (key holder publishes a new record) are BOTH mutable. We cannot cheaply PROVE a specific ENS name is locked, so mutable is the honest default.

A NEW posture, `TrustPosture::MutableName` ("content-verified, mutable name"), is the honest floor for ANY mutable name whose bytes verified: distinct from `ContentVerified` (honestly weaker: only a direct `ipfs://<cid>` is immutable) and from `UnverifiedOrigin` (the bytes DID verify). It is NEVER labelled "verified".

**Display precedence (loudest wins), made EXPLICIT in code so it never drifts:** `NameViaTrustedRpc` > `MutableName` > `ContentVerified`. A misdirecting RPC is worse than an honest controller repointing, so the RPC-trust warning dominates. Concretely a load carries two independent axis flags (`ens_origin`, `mutable_name`) on the `LoadLifecycle`; when the `ipfs://` scheme handler marks the bytes verified, the posture is computed as: `ens_origin` -> `NameViaTrustedRpc`, else `mutable_name` -> `MutableName`, else `ContentVerified`.

## Consequences

- An ENS `ipns-ns` (or `ipfs-ns`) load is flagged BOTH ENS-originated AND (for ipns-ns) mutable-named, so it shows `NameViaTrustedRpc` today. When Phase 2 (a light client) stops setting `ens_origin`, the SAME load naturally falls back to `MutableName` with NO change to the display rule: the louder warning simply clears. That elegance is the reason the precedence is encoded as a fall-through rather than per-entry-point posture assignment.
- A future "prove immutability" capability (burned keys / NameWrapper fuses / locked resolver) is what would let a SPECIFIC ENS name earn `ContentVerified` instead of `MutableName`; a TOFU pin-and-warn-on-change store (`ipns-tofu-pin-and-warn-on-change`) is the tracked follow-on for warning when a mutable name's blessed CID changes. Neither is built here.
- The posture is threaded exactly like the others: renderer `TrustPosture` -> webview `LoadLifecycle` (the `mark_mutable_name` axis + the `Renderer::mark_mutable_name` seam method) -> core `ChromeState::is_mutable_name` -> the desktop chrome's distinct `trust-mutable-name` badge (never "verified"). Mobile edges do not yet surface the posture over their FFI JSON (unchanged by this task).
