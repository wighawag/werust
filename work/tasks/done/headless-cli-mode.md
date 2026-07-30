---
title: "Desktop binary usable as a CLI (subcommands like `werust resolve ronan.eth`, `werust fetch ipfs://...`) without a GUI"
slug: headless-cli-mode
blockedBy: []
covers: []
---

## What to build

HUMAN REQUEST: the desktop binary (`crates/werust`) is a pure GTK GUI that requires a running display. It would be useful to run it as a headless CLI for scripting, debugging, and environments without a display. The binary already contains ALL the core capabilities (ENS resolution, IPFS content retrieval, hash verification); nothing new needs to be built inside the Rust crates — only a CLI dispatch path that runs without creating a GTK window.

The model is `cargo`'s subcommand dispatch or `git`'s verb-first interface: the binary checks the first argument and routes to the GTK path only when no known subcommand matches (or when no subcommand is given — the default is still "open the browser").

**Proposed CLI surface:**

```
werust                        # launch the GUI (existing behavior)
werust resolve <ens-name>     # resolve an ENS name and print the decoded contenthash
werust resolve --json <name>  # same but JSON output for scripting
werust fetch <ipfs-url>       # fetch an ipfs:// URL, print bytes info
werust version                # print the werust version and exit (--version already works)
werust list-endpoints         # print the configured gateways and RPC endpoints
werust --headless <url>       # open a URL headlessly (no window) — a future stretch goal
```

**Scope: Phase-1 deliverable = `resolve` and `version`.** The default GUI path is untouched. The CLI path runs `gtk::init()` only if a subcommand that needs the GUI is used; `resolve` needs NO gtk at all (it uses `werust_core::ens` and `werust_core::ethereum` directly with an `RpcProvider` built from the same `rpc_endpoint()` default/env source). This is a pure additive change: the core is already compiled, the binary just needs a dispatch table before the GTK `Application` setup.

**Where to start:**

- `crates/werust/src/main.rs`: before the `Application::builder()` block, match `std::env::args()` against known subcommands. If none match, fall through to the existing GTK path.
- `resolve` subcommand: build an `RpcProvider::new()`, call `ens::resolve(name, &provider)`, print the result (or JSON if `--json`). Reuse the EXACT same error formatting the GUI uses — `{e}`.
- `version` subcommand: `println!("{}", banner())` and exit. Already works via `--version` but `werust version` as a subcommand is more discoverable.
- Error handling: a resolution error prints to stderr and exits with code 1 (like a Unix tool).
- Flags: use a SIMPLE hand-rolled argv parser (not clap — adding a dependency for one subcommand dispatch is unnecessary weight). Keep it small: match the first non-executable argument, handle `--json` as a flag before the positional arg, everything else falls through.

**Not in scope:**
- A `--headless` full-browser mode (that is a much bigger undertaking — running WebKitGTK without a display is complex and requires xvfb or offscreen rendering). If the user wants it later, it is a separate task.
- The `fetch` subcommand (the IPFS fetch path exists but surfacing it cleanly through the `fetcher` seam with no display context is a small separate step).
- Auto-completion or man pages.

## Acceptance criteria

- [ ] `werust resolve ronan.eth` prints the resolved contenthash reference to stdout (JSON with `--json`) and exits 0; on error prints to stderr and exits 1.
- [ ] `werust version` prints the same banner the GUI prints (`werust <version> — a Rust web browser (webview backend)`).
- [ ] `werust` (no args) launches the GUI exactly as before (backward-compatible default).
- [ ] `werust --help` prints a usage message listing the available subcommands (and the GUI default).
- [ ] No new dependencies (clap, serde for CLI output — use `println!` and hand-rolled output).
- [ ] Test: a unit test that `main` dispatch routes `resolve`/`version` args correctly (or that `resolve` calls the core function and formats its output).
- [ ] Manual: `cargo run -- resolve ronan.eth` works from a terminal.

## Prompt

> Goal: add a subcommand dispatch to the desktop binary (`crates/werust/src/main.rs`) so that `werust resolve ronan.eth` resolves an ENS name and prints the result without opening a GTK window. The default `werust` (no args) still opens the GUI; minimal hand-rolled argv parsing; no new dependencies; uses the existing `RpcProvider::new()` from `werust-core` — which reads `WERUST_RPC_URL` env or the compiled default, consistent with the GUI.
>
> Where to look: `crates/werust/src/main.rs`. Add the dispatch before the `Application::builder()` block at line ~612 (or rather, before it — `version` exits immediately, `resolve` runs and exits, the fallback continues to the GUI). The `resolve` call is `ens::resolve(name, &provider)` and formats the result with Debug/Display. JSON output with `--json` is optional printing. `version` reuses the existing `banner()` function. Not in scope: `fetch`, `--headless`, clap.
