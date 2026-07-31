# Decisions: the headless CLI dispatch (`werust resolve` / `werust version`)

Task: `headless-cli-mode`.
Code: `crates/werust/src/main.rs` (the `Command` enum, `parse_args`, `usage`, `resolve_output`, `json_escape`, `run_resolve`, and the dispatch at the top of `main`), pinned by the display-free tests `argv_routes_the_known_subcommands_and_falls_through_to_the_gui`, `resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one` and `usage_lists_every_subcommand_and_the_gui_default` in the same file.
Manual verification transcript: [`README.md`](README.md).

These are the judgement calls this task bakes in, recorded so the named follow-ons (`fetch`, a `--headless` browse mode) and the reviewer inherit them explicitly rather than re-deriving them from the code.

## Decision 1: `resolve` reports a MUTABLE `ipns-ns` contenthash as `ipns://<name>` and does NOT follow it

> **SUPERSEDED (2026-07-31, task `cli-resolve-follows-mutable-names-to-the-cid`).** The human ratified the opposite at the Gate-2 review of this task (`work/notes/observations/gate3-headless-cli-mode-2026-07-30.md`): the CLI and the GUI must not disagree about what a name resolves to, so `resolve` now performs the FULL resolution and FOLLOWS an `ipns-ns` pointer through its client-verified record to the `ipfs://<cid>` — with no flag and no second verb. The mutability is kept (a stderr note, and `mutable` + `pointer` in `--json`), and the alternative rejected below ((a), "follow the record") is what shipped, via a lifted core function both surfaces call rather than a fetch stack pulled into the binary. See [`../cli-resolve-follows-mutable-names-to-the-cid/DECISIONS.md`](../cli-resolve-follows-mutable-names-to-the-cid/DECISIONS.md). The reasoning below is kept as the record of why it was scoped out first.

`werust resolve ronan.eth` prints `ipns://k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc`, not the CID that name currently points at. The immutable case prints its `ipfs://<cid>` unchanged.

- **What it touches.** The out-of-scope `fetch` subcommand (it is the verb that would want the followed CID), and any script reading `resolve`'s stdout.
- **Why.** The verb is the ENS read: namehash -> registry -> resolver -> ENSIP-7 decode (`werust_core::ens::resolve`), which is exactly what the task scopes ("print the decoded contenthash", `fetch` explicitly not in scope). FOLLOWING an `ipns-ns` pointer is a second, different operation: fetch a signed IPNS record over the untrusted record source and client-verify it (`werust_core::ipns::resolve_ipns_name`, `docs/adr/0007`) — content retrieval, with its own network hop, timeout and failure taxonomy. Flattening it into `resolve` would also LIE about mutability: the two decoded kinds carry different trust postures (`docs/adr/0006`: an `ipns-ns` name is at most `MutableName`, never immutable `ContentVerified`), so reporting both as one `ipfs://` answer would erase the distinction the whole trust model rests on. Printing the `ipns://` pointer keeps `resolve` honest AND total: every loadable contenthash kind has an output.
- **The alternative considered.** (a) Follow the record and print the final `ipfs://<cid>`, matching what the GUI address bar ends up loading — rejected: it pulls the IPNS record source (and the fetch stack) into a verb the task scoped to the ENS read, and it hides the mutable step. (b) Refuse an `ipns-ns` name as "not supported by `resolve`" — rejected: `ens::resolve` returns it as a SUCCESS, and refusing werust's own flagship name (`ronan.eth`, the task's manual acceptance check) would make the subcommand useless for the main case. A later `--follow` flag, or `fetch`, can add the followed form without changing what `resolve` means today.
- **The `ipns://` spelling** reuses the URL-bar scheme string already present in the codebase (`crates/werust-core/src/lib.rs`'s `ipns://k51…` navigation test, `crates/werust-android/rust/src/origin_map.rs`); it is not a new concept minted here. Note the bare-`ipns://` ENTRY surface in the URL bar is still a named follow-on, not built (`docs/adr/0007`, decision 4), so this output is a REFERENCE to paste/script with, not yet something `werust <url>` will open.

## Decision 2: the startup banner is printed by the GUI arm only

`println!("{}", banner())` moved from the top of `main` into the GUI fall-through, after the dispatch. `werust version` prints the same banner explicitly, so nothing the GUI shows was lost.

- **What it touches.** stdout of the new subcommands only; the GUI launch prints the identical line it always did.
- **Why.** A subcommand's stdout is its RESULT. With the banner first, `--json` output is not parseable JSON and `$(werust resolve ronan.eth)` yields the banner plus the reference — the two acceptance criteria ("prints the resolved contenthash reference to stdout", "JSON with `--json`") cannot both hold with a preamble in front.
- **The alternative considered.** Print the banner to stderr for every invocation (keeping one unconditional call). Rejected: it makes every scripted use noisy for no benefit, and stderr is where this CLI's FAILURES go (exit 1), so mixing a success banner in would blur that.

## Decision 3: `--version` / `-V` / `-h` are accepted as flag spellings

`version` is the discoverable verb; `--version` and `-V` are accepted as aliases, and `-h` alongside `--help`.

- **What it touches.** Arguments that previously fell through as a startup URL. Before this change `werust --version` printed the banner and then opened the GUI on the "URL" `--version` (which fails as an invalid entry), so no working behaviour is displaced — the task's premise that "`--version` already works" was only accidentally true (the banner was printed unconditionally, decision 2).
- **Why.** A user reaching for a version types `--version` reflexively; making it mean something else would be a trap, and it is the one flag spelling the task's own text assumed exists.
- **The alternative considered.** The bare `version` verb only, per the letter of the acceptance criteria. Rejected as needlessly surprising, at the cost of one match arm. Deliberately NOT added: any other flag alias (no `-v`, which is conventionally "verbose" and would be a second meaning to unpick later).

## Decision 4: a malformed KNOWN verb refuses; an UNKNOWN first argument still opens the GUI

`werust resolve` (no name), `werust resolve a.eth b.eth` and `werust resolve --nope x` print a specific reason plus the usage message to stderr and exit 1 (`Command::Usage`). But `werust ronan.eth`, `werust --foo`, `werust anything-else` still launch the GUI on that argument, exactly as `env::args().nth(1)` did before.

- **What it touches.** The backward-compatible GUI default (the third acceptance criterion) versus the new refusals; and any future verb, which inherits this rule.
- **Why.** Two different situations. Once the user NAMES a known verb, guessing is worse than refusing: silently opening a browser window because `resolve` was missing its argument would be baffling in a headless environment (and impossible without a display). But an unknown first argument has a meaning that PREDATES the CLI — it is the startup URL — so treating it as a CLI error would break every existing invocation, including `werust ronan.eth`. Hence the parser is deliberately non-greedy: only the named verbs and flags are taken.
- **The alternative considered.** Reject any unrecognised argument (strict, clap-like). Rejected: it breaks the documented `werust <url>` behaviour for zero benefit. Also considered a distinct exit code (2) for a usage error, the BSD convention; kept at 1 because the acceptance criterion names 1 for "on error" and a second code is a surface future verbs would have to honour.

## Decision 5: no platform-capability-matrix row for the CLI

`docs/platform-capability-matrix.toml` (the `verify`-enforced parity guard, `docs/adr/0005`) gets NO new `[[capability]]` block for the headless CLI.

- **What it touches.** The parity guard and the mobile edges (which would each need a cell).
- **Why.** The matrix tracks cross-cutting USER-FACING BROWSER capabilities that a seam no-op can silently ship on one platform only (address bar, `ipfs://` render, ENS resolution, provider, trust indicator). A CLI is not one of those: it is an INVOCATION MODE of the desktop binary, and the capability it exposes (ENS resolution) already has its own row, implemented on all three platforms. iOS and Android ship as apps with no argv entry point at all, so the row could only ever read `n-a` for both and would carry no parity signal. The guard forces cells only for rows present in the file, so omitting it does not hide a gap.
- **The alternative considered.** Add a row with `ios`/`android` = `n-a` for completeness. Rejected as noise that dilutes the matrix's one job (surfacing SILENT single-platform gaps in browser behaviour); recorded here instead so the omission is a decision, not an oversight. If a mobile "run a resolution without the UI" surface is ever wanted, it is a genuine capability and gets a row then.
