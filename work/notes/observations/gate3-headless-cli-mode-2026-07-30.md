---
title: "Gate-3 conductor review: headless-cli-mode (APPROVE)"
date: 2026-07-30
status: open
reviewOf: headless-cli-mode
verdict: approve
---

## Verdict: APPROVE

Merged as `c0a1a65` on `origin/main` (drive-tasks, `--allow-backlog --review --merge`, `etherplay/opus-5`). Gate-1 (repo `verify`) and Gate-2 (review gate, 6 non-blocking nits) both green. Unlike the usual desk review, EVERY acceptance criterion here was verifiable headlessly, so I ran the built binary myself rather than trusting the transcript.

## Acceptance criteria, ticked against the merged tree (live transcript)

- [x] **`werust resolve ronan.eth` prints the reference to stdout, exit 0.** Ran it: `ipns://k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc`, exit 0.
- [x] **`--json` prints the machine-readable form.** `{"name":"ronan.eth","kind":"ipns","reference":"ipns://k51qzi…"}`, exit 0.
- [x] **On error, prints to stderr and exits 1.** `werust resolve definitely-not-a-real-name-xyz.eth` gives `werust: this name has no ENS resolver set`, exit 1: the core's OWN typed reason, not a generic failure.
- [x] **`werust version` prints the same banner the GUI prints.** `werust 0.2.9 — a Rust web browser (webview backend)`, and the banner is now suppressed on headless paths so stdout is the RESULT (correct: `$(werust resolve …)` would otherwise be unusable).
- [x] **`werust --help` lists the subcommands and the GUI default.** Verified, including the note about `WERUST_RPC_URL`.
- [x] **`werust` (no args) still launches the GUI.** Not launched on the operator desktop; the dispatch is a pure function of argv and `argv_routes_the_known_subcommands_and_falls_through_to_the_gui` pins empty-argv to `Gui { DEFAULT_URL }` plus the unknown-first-arg fall-through.
- [x] **No new dependencies.** `git diff` over `Cargo.toml`/`Cargo.lock` is EMPTY: no clap, no serde. Output is `format!`, and `--json` escapes via a hand-rolled `json_escape` (argv can contain a quote).
- [x] **Test pins the dispatch.** Three tests: argv routing, `resolve_output` formatting including the fail-closed unsupported arm, and the usage listing.

Design note worth keeping: the whole CLI is a pure `parse_args -> Command` value plus one arm per command, and `run_resolve` touches NO GTK (not even `gtk::init`), which is why it works over ssh. Nice shape.

## Nit triage (6 non-blocking findings)

**Needs your decision, and I hit it live: `resolve` emits a reference werust cannot open.** For a mutable `ipns-ns` contenthash, `resolve` prints the `ipns://<name>` pointer and deliberately does not follow it to a current CID. So the flagship example, `werust resolve ronan.eth`, returns exactly what I got above: an `ipns://` reference that the GUI's URL bar cannot open (bare `ipns://` entry is still an unbuilt follow-on, ADR-0007 decision 4), while the GUI on the SAME name does follow the record and renders the site. The decision itself is defensible (following the record is content retrieval, which is the out-of-scope `fetch` verb) but the user-facing result is incoherent today. Ratify, or schedule a `--follow` / `fetch` that closes it.

**Worth a follow-up: `--json` mints a wire vocabulary inline.** The keys (`name`/`kind`/`reference`) and the `kind` values (`ipfs`, `ipns`) are minted in the binary, but this repo centralises wire vocabulary elsewhere (`werust_core::debug::trust_posture_wire_name` for the chrome JSON) and already names these protocols in core (`ProtoCode::display_name`, the ENSIP-7 `ipfs-ns`/`ipns-ns` spelling). Scripts will pin whatever ships. Deciding now whether `kind` comes from a core helper prevents a later `fetch`/`--follow` forking a second spelling.

**Ratify (I would): flag aliases beyond the task's letter.** `--version`, `-V` and `-h` are accepted alongside the `version` verb and `--help`. The reviewer checked `.github/`, `docs/` and `dorfl.json`: nothing invoked `werust --version`, so no scripted behaviour is displaced, and these argv values previously just became a bogus startup URL.

**Minor parser default nobody signed off:** `--json` is accepted on either side of the name, but a LEADING `--json` (`werust --json resolve x`) falls through the catch-all and silently becomes a GUI URL instead of a usage refusal. Low impact, easy to tighten if you care.

**Accepted as reasoned:** no `docs/platform-capability-matrix.toml` row for the CLI (it is an invocation mode of the desktop binary, whose ENS-resolution capability already has a row; mobile has no argv entry point, so cells could only read `n-a`), and `run_resolve`'s 8 lines of glue being covered only by the manual transcript. If the CLI grows a second verb, inject the provider instead.

## What this unlocks

A display-free way to exercise ENS resolution end to end, which is exactly what was missing when the desktop GUI could not be smoke-tested headlessly earlier in this session.
