# Decisions: `resolve` follows a mutable name through to the CID

Task: `cli-resolve-follows-mutable-names-to-the-cid`.
Code: `crates/werust-core/src/name_resolution.rs` (the lifted resolution path), `crates/werust-core/src/contenthash.rs` (`ProtoCode::wire_name`), `crates/werust-core/src/ipns.rs` (`default_record_source`), `crates/werust-core/src/lib.rs` (`BrowserShell::navigate_ens_name`, now a caller), `crates/werust/src/main.rs` (`resolve_output`, `run_resolve`, `usage`).
Manual verification transcript: [`README.md`](README.md).

These are the judgement calls this task bakes in, recorded so the reviewer and any later verb (`fetch`, a bare `ipns://` URL-bar entry) inherit them explicitly.

## Decision 1: stdout stays ONE bare `ipfs://<cid>`; the mutable-name warning goes to stderr

For a followed `ipns-ns` name the CLI prints the CID on stdout and a one-line "this is a MUTABLE name (`ipns://…`), its controller can repoint it" note on **stderr**. `--json` prints no note (the object carries `mutable` + `pointer`).

- **What it touches.** Every script reading `resolve`'s stdout; the acceptance criterion "the human-readable output makes clear the CID came from a mutable name"; any later verb that prints a resolution.
- **Why.** `headless-cli-mode` established (its decision 2) that a subcommand's stdout is its RESULT, so `$(werust resolve ronan.eth)` is directly usable — that is why the startup banner was moved off the headless paths. Annotating the stdout line ("`ipfs://… (via ipns://…)`") or printing two lines would break exactly the property the previous task went out of its way to create, and would do it for a *warning*, which is what stderr is for in this binary already (every failure reason goes there). Two streams means the human sees the mutability and the script still gets a pin-able CID.
- **The alternatives considered.** (a) Suffix/second line on stdout — rejected above. (b) Say nothing in the human form and let `--json` carry the fact alone — rejected: the interactive user is precisely the one who cannot see the JSON, and the trust posture is a product surface (`docs/adr/0001`/`0006`), not a machine-only detail. (c) Also emit the note in `--json` mode — rejected: the object already carries `mutable` and `pointer`, so it would be duplicate noise in the one mode built for scripts.

## Decision 2: the `--json` `kind` becomes the ENSIP-7 spelling (`ipfs-ns` / `ipns-ns`), changing the shipped values

`kind` was `"ipfs"` / `"ipns"`, minted in `crates/werust/src/main.rs`. It is now `ProtoCode::wire_name()`: `"ipfs-ns"` / `"ipns-ns"`.

- **What it touches.** The `--json` wire contract shipped since v0.2.9, so any script pinning `kind == "ipns"`; and every later surface that reports a contenthash protocol (a `fetch` verb, the debug Network tab), which now has one place to read the spelling from.
- **Why.** The task asks for the vocabulary to come from a core helper reusing the ENSIP-7 spelling, and reusing it *means* adopting its values — a core helper that returned the binary's old `"ipfs"`/`"ipns"` would just relocate a second spelling rather than remove it. `ipfs-ns`/`ipns-ns` is also the more honest field: `kind` describes the ENS **contenthash namespace**, not the scheme of the printed reference (which is now always `ipfs://` in both cases), so the old values would actively mislead after this change ("kind: ipns" next to "reference: ipfs://…").
- **The alternative considered.** Keep `"ipfs"`/`"ipns"` and source *those* from core, for wire compatibility. Rejected: the `reference` value changes for a mutable name in this same release anyway (that is the whole task), so a script pinning this object is already being asked to look again; changing both at once is one break, not two. The `resolve` verb is one release old and the repo ships no consumer of it (checked `.github/`, `docs/`, `dorfl.json`).
- **Also decided here:** an `Unknown` protoCode reports the sentinel `"unknown"` from `wire_name()` (the multicodec table has no name for it); its raw hex stays in `DecodedContenthash::reason()`, which is the surface that actually reports an unsupported protocol.

## Decision 3: the lifted path lives in a new `name_resolution` module, not in `ens` or `ipns`

`werust_core::name_resolution::{resolve_name, resolve_name_with_progress, ResolvedName, NameResolutionError}`.

- **What it touches.** The core's module vocabulary; every future caller of "resolve a name to content" (a `fetch` verb, a mobile edge, a Phase-2 trustless backend).
- **Why.** The chain COMPOSES two existing cores that must stay separable: `ens` is the chain read (and knows nothing about IPNS records), `ipns` is the record fetch + verify (and knows nothing about ENS). Putting the composition in either one would make that module depend on the other's domain and give the crate two plausible front doors. The name follows the existing pattern (`version_resolution`, `retrieval`) and matches how `CONTEXT.md` already speaks ("ENS resolution", "IPNS name resolution"): this is *name* resolution, the whole of it.
- **The alternative considered.** A method on `BrowserShell` made public. Rejected: it would keep the CLI needing a renderer to resolve a name, which is the drift this task exists to remove.

## Decision 4: progress is reported with the chrome's own `LoadStep`, not a new progress enum

`resolve_name_with_progress` takes `&mut dyn FnMut(LoadStep)` and emits `ResolvingName`, then `FetchingRecord` for a mutable name.

- **What it touches.** The shell's load-step pin (and therefore the load indicator's phase text on every edge), plus any future caller that wants stage reporting.
- **Why.** The shell pinned exactly those two values inline before this change; a second `NameResolutionStep` enum would be the same two concepts under two names, and the mapping between them would be one more thing to keep in step (the repo's standing complaint about duplicated derivations, `docs/adr/0011`). Only the RESOLUTION stages are emitted — the content fetch belongs to the backend, not to this function.
- **A note on the pin's timing.** The shell now applies the reached step *after* the call returns (the callback writes to a local cell, because the resolution borrows the shell's provider and record source). That is the same state the old inline code left at the same point: resolution is synchronous under `&mut self`, so no caller can observe the shell mid-resolution, and both outcomes (`load_resolved_content` / `fail_ens_load`) clear the step anyway.

## Decision 5: one construction site for the default IPNS record source

`ipns::default_record_source()` now builds the gateway record source (endpoint = the user's chosen retrieval backend, timeouts = the split-out record budget); `BrowserShell::with_provider` and the CLI's `run_resolve` both call it.

- **What it touches.** The retrieval-backend user setting and the record-fetch timeout, which would otherwise have to be restated in the binary.
- **Why.** The whole point of the task is that the two surfaces cannot disagree about what a name resolves to; letting the CLI hand-roll its own record source would reintroduce the divergence one layer down (a different gateway, or the content path's much longer timeout). Copying the shell's construction into `crates/werust/src/main.rs` was the alternative — rejected for exactly that reason.
