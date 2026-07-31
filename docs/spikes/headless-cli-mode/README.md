# The headless CLI dispatch: map + manual verification

Task: `headless-cli-mode`. Judgement calls: [`DECISIONS.md`](DECISIONS.md).

> **The `resolve` OUTPUT below is superseded** (2026-07-31, task `cli-resolve-follows-mutable-names-to-the-cid`): `resolve` now completes the resolution, so `ronan.eth` prints the `ipfs://<cid>` its client-verified IPNS record points at (with the mutability on stderr and in `--json`) instead of the `ipns://` pointer, and `--json`'s `kind` is now the ENSIP-7 `ipfs-ns`/`ipns-ns` spelling. The dispatch map itself still holds. Current transcript: [`../cli-resolve-follows-mutable-names-to-the-cid/README.md`](../cli-resolve-follows-mutable-names-to-the-cid/README.md).

## What landed

One verb-first dispatch at the top of `crates/werust/src/main.rs`, before any GTK setup:

```
std::env::args().skip(1) -> parse_args -> Command
                                          |- Help    -> println!(usage())  exit 0
                                          |- Version -> println!(banner()) exit 0
                                          |- Usage   -> eprintln!(reason + usage()) exit 1
                                          |- Resolve -> run_resolve(name, json)     exit 0 / 1
                                          `- Gui     -> banner + Application::builder() … (unchanged)
```

`run_resolve` builds a `werust_core::ethereum::RpcProvider::new()` — the SAME endpoint source the GUI shell uses (`WERUST_RPC_URL` when set and non-empty, else the compiled default) — and resolves the name through the core (`werust_core::ens::resolve` when this landed; since `cli-resolve-follows-mutable-names-to-the-cid` it is `werust_core::name_resolution::resolve_name`, the same path the GUI front door walks). It touches NO GTK: no `Application`, no window, not even `gtk::init`, so it runs over ssh and in CI with no display. No new dependencies: the JSON form is `format!` + a local `json_escape`.

## Manual verification (2026-07-30, `werust 0.2.9-8-g828d0b5`, Debian desktop, `DISPLAY` unused)

```
$ cargo run -- resolve ronan.eth
ipns://k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc      # exit 0

$ cargo run -- resolve --json ronan.eth
{"name":"ronan.eth","kind":"ipns","reference":"ipns://k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc"}   # exit 0

$ cargo run -- resolve vitalik.eth
ipfs://bafybeihw3n6rulxprloowr5kdhotje4v63phykialk6crd4djlvnpexapa        # exit 0

$ cargo run -- resolve nonexistent-werust-test-name-xyz.eth
werust: this name has no ENS resolver set                                  # exit 1, on stderr

$ cargo run -- resolve
werust: `resolve` needs an ENS name                                        # exit 1, on stderr
<usage>

$ cargo run -- version
werust 0.2.9-8-g828d0b5 — a Rust web browser (webview backend)             # exit 0

$ cargo run -- --help
<banner + usage listing every subcommand and the GUI default>              # exit 0
```

Both decoded contenthash kinds are covered by a live name: `ronan.eth` is the mutable `ipns-ns` case (reported as the `ipns://` pointer, deliberately not followed — `DECISIONS.md` decision 1) and `vitalik.eth` the immutable `ipfs-ns` case. The fail-closed arms print the ENS core's OWN typed reason, the same text the GUI's error banner shows.

`werust` with no arguments, and `werust <url>`, still print the banner and open the GTK window: that arm of `main` is byte-for-byte the pre-change code, and `parse_args` routing both to `Command::Gui` is pinned by `argv_routes_the_known_subcommands_and_falls_through_to_the_gui`.
