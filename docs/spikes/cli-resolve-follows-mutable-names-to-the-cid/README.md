# `werust resolve` completes the resolution: map + manual verification

Task: `cli-resolve-follows-mutable-names-to-the-cid`. Judgement calls: [`DECISIONS.md`](DECISIONS.md). Supersedes decision 1 of [`../headless-cli-mode/DECISIONS.md`](../headless-cli-mode/DECISIONS.md).

## What landed

The name-to-CID step was lifted OUT of the browser shell into a callable core module, and both surfaces now call it:

```
crates/werust-core/src/name_resolution.rs
  resolve_name(provider, ipns_source, name)                  -> Result<ResolvedName, NameResolutionError>
  resolve_name_with_progress(…, on_step: FnMut(LoadStep))     the same, reporting the pipeline stage

        ens::resolve (namehash -> registry -> resolver -> ENSIP-7 decode)
            |- ipfs-ns  -> ResolvedName::Immutable { uri, cid }            (no record fetch)
            |- ipns-ns  -> ipns::resolve_ipns_name (fetch + CLIENT-VERIFY the signed record)
            |               -> ResolvedName::Mutable { pointer, uri, cid }
            `- other    -> Err(Ens(UnsupportedContenthash(proto)))         (named refusal)

callers
  BrowserShell::navigate_ens_name  (GUI)      -> resolve_name_with_progress -> load_resolved_content
  run_resolve                      (headless) -> resolve_name              -> resolve_output
```

There is now ONE resolution implementation: the shell keeps only its own half (the load-step pin, feeding the CID into the verified `ipfs://` path, and the trust flagging), and a record that fails verification fails identically in both surfaces.

Output vocabulary: the `kind` field is `ProtoCode::wire_name()` (`ipfs-ns` / `ipns-ns`, the ENSIP-7 / multicodec spelling the decoder already dispatches on), not a literal in the binary. The `ipns://<name>` pointer string is minted once, in `name_resolution`.

No new flag, no new verb, no new dependency (`git diff` over `Cargo.toml` / `Cargo.lock` is empty).

## Manual verification (2026-07-31, `werust 0.2.9-93-gdae00bc`, Debian desktop, `DISPLAY` unused)

```
$ werust resolve ronan.eth
werust: ronan.eth is a MUTABLE name (ipns://k51qzi5uqu5diifcue0h8g3dxnd0vjaaft5h8ocqcfit2th2ulcg4mdjdtjmo5): this is the CID its client-verified IPNS record points at right now, and its controller can repoint it.   # stderr
ipfs://bafybeibaaylr54zrqoduqmew2pacf2foazvttyclldqu53v4nmw7wqoefi                                     # stdout, exit 0

$ werust resolve --json ronan.eth
{"name":"ronan.eth","kind":"ipns-ns","reference":"ipfs://bafybeibaaylr54zrqoduqmew2pacf2foazvttyclldqu53v4nmw7wqoefi","cid":"bafybeibaaylr54zrqoduqmew2pacf2foazvttyclldqu53v4nmw7wqoefi","mutable":true,"pointer":"ipns://k51qzi5uqu5diifcue0h8g3dxnd0vjaaft5h8ocqcfit2th2ulcg4mdjdtjmo5"}   # exit 0

$ werust resolve vitalik.eth                                            # the immutable ipfs-ns case, unchanged
ipfs://bafybeihw3n6rulxprloowr5kdhotje4v63phykialk6crd4djlvnpexapa      # exit 0, nothing on stderr, no record fetch

$ werust resolve --json vitalik.eth
{"name":"vitalik.eth","kind":"ipfs-ns","reference":"ipfs://bafybeihw3n6rulxprloowr5kdhotje4v63phykialk6crd4djlvnpexapa","cid":"bafybeihw3n6rulxprloowr5kdhotje4v63phykialk6crd4djlvnpexapa","mutable":false,"pointer":null}

$ werust resolve nonexistent-werust-test-name-xyz.eth
werust: this name has no ENS resolver set                               # exit 1, stderr, the core's own typed reason
```

`ronan.eth` is the case the whole task exists for: it previously printed `ipns://k51qzi…`, a reference werust's own URL bar cannot open, while the GUI on the same name followed the record and rendered. It now prints the `ipfs://<cid>` the GUI loads — and `$(werust resolve ronan.eth)` is a CID a script can pin, with the mutability still stated.

(The IPNS name above differs from the one in `headless-cli-mode`'s transcript because `ronan.eth`'s contenthash changed in between; that is the ENS record, not a werust behaviour.)

## Where the behaviour is pinned, network-isolated

- `crates/werust-core/src/name_resolution.rs`: a mutable name is followed through a REAL signed record (a minted ed25519 key, an in-process record source) to its CID and carries the pointer; an immutable name resolves with the record source's fetch counter still at **0** (no extra network call); a record signed by a DIFFERENT key fails closed with the IPNS core's typed reason; an unsupported protocol is a named refusal; the reported steps are the chrome's own `ResolvingName` / `FetchingRecord`.
- `crates/werust-core/src/contenthash.rs`: `every_protocol_reports_its_ensip7_multicodec_wire_name`.
- `crates/werust/src/main.rs`: `resolve_prints_the_ipfs_reference_for_an_immutable_name` and `resolve_follows_a_mutable_name_to_the_cid_and_keeps_saying_it_is_mutable` (display-free, pure formatting).
- `crates/werust-core/src/lib.rs`: the existing ENS/IPNS front-door tests are untouched and still green — which is the evidence that the GUI walks the same lifted path.
