# werust

A from-scratch, general-purpose web browser in **Rust** for a "post-trusted-server" web: native `ipfs://` resolution, a native Ethereum (EIP-1193) provider and ENS-name resolution, privacy-protecting, local-first, with full compatibility for the normal server web. The domain language, conventions, and architecture entry point live in [CONTEXT.md](CONTEXT.md); decisions in `docs/adr/`; the work system in `work/`.

## Development

Standard cargo workflow: `cargo build`, `cargo test` (the `verify` gate additionally runs `cargo fmt --check` and `cargo clippy`). Desktop builds need the WebKitGTK 6.0 system dev packages; see `.github/workflows/release.yml` for the exact list.

### Private Ethereum RPC endpoint (`WERUST_RPC_URL`)

ENS resolution (`ronan.eth` -> IPFS) goes through a trusted JSON-RPC endpoint whose public, keyless default is `https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161`. To use a private endpoint (your own node, e.g. the `https://your-private-rpc.example.com/` shape) WITHOUT committing its URL:

1. Copy `.env.example` to `.env` (git-ignored) and set `WERUST_RPC_URL`.
2. Run via `source .env && cargo run` (or a `direnv`-style loader — your choice; werust deliberately ships NO runtime `.env` loader).

The variable is read ONCE at session construction, so a change takes effect on relaunch. An unset or empty value falls back to the public default, so a fresh build works with no configuration. The release workflow passes an optional `WERUST_RPC_URL` repository secret through the same name. NEVER commit a real RPC URL to any tracked file.
