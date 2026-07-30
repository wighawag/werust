---
title: "`werust resolve` follows a mutable name through to the content CID (no flag), and speaks the core's protocol vocabulary instead of its own"
slug: cli-resolve-follows-mutable-names-to-the-cid
blockedBy: []
covers: []
---

## What to build

Origin: two `headless-cli-mode` Gate-2 nits, both ratified by the human on 2026-07-30 (see `work/notes/observations/gate3-headless-cli-mode-2026-07-30.md`).

**1. `resolve` must complete the resolution, with no flag.** Today `werust resolve ronan.eth` prints `ipns://k51qzi5uqu5dlu1ien9spji7pu49mfw97mn0qv4azugqcvenj0dvzq9bgwp1zc`: the ENS contenthash verbatim. That is a reference werust itself cannot open (bare `ipns://` URL-bar entry is still an unbuilt follow-on, `docs/adr/0007` decision 4), while the GUI on the SAME name follows the IPNS record and renders the site. The CLI and the GUI must not disagree about what a name resolves to. The human's decision: no `--follow` flag, no second verb; `resolve` performs the FULL resolution, so an ENS name pointing at an IPNS name is followed through to the actual `ipfs://<cid>`.

**Reuse the path the GUI already walks, do not reimplement it.** `BrowserShell::navigate_ens_name` in `crates/werust-core/src/lib.rs` already does exactly this chain (ENS contenthash decode, then `crate::ipns::resolve_ipns_name` to fetch and CLIENT-VERIFY the signed record before anything is loaded). The CLI should call the same core function, so a record that fails verification is the same fail-closed error in both surfaces. If that logic is currently only reachable through the shell, lifting the name-to-CID part into a callable core function is the right shape, and the GUI should then use it too, so there is ONE resolution path rather than two.

**Do not flatten the trust posture.** A followed IPNS name is `MutableName`, never plain content-verified (`docs/adr/0006`, `docs/adr/0007`): the controller can repoint it at any time. So the output must still say the name was mutable, and the `--json` object must carry BOTH facts (the pointer that was followed AND the CID it currently resolves to) rather than replacing one with the other. A script that pins the CID must be able to see it came from a mutable name.

**2. The output vocabulary comes from the core.** `resolve_output` currently mints `kind` values (`"ipfs"`, `"ipns"`) inline in `crates/werust/src/main.rs`. This repo already centralises wire vocabulary (`werust_core::debug::trust_posture_wire_name` for the chrome JSON) and already names these protocols in core (`ProtoCode::display_name`, the ENSIP-7 `ipfs-ns` / `ipns-ns` spelling). Source the strings from a core helper so a later `fetch` verb cannot fork a second spelling, and so the CLI, the chrome JSON and the debug Network tab keep one vocabulary. Adding a small `werust-core` helper is preferable to reaching for an enum's `Debug`.

## Acceptance criteria

- [ ] `werust resolve <ens-name>` prints the `ipfs://<cid>` the GUI would actually load for that name, including when the ENS contenthash is an `ipns-ns` pointer (the record is fetched and client-verified first, exactly as the GUI does).
- [ ] The IPNS record verification is the SAME core path the GUI uses (one resolution implementation, not a CLI copy); a record that fails verification fails the command fail-closed with the core's own typed reason and exit 1.
- [ ] The mutable-name fact is NOT lost: the human-readable output makes clear the CID came from a mutable name, and `--json` carries both the followed pointer and the resolved CID.
- [ ] An immutable `ipfs-ns` name behaves as it does today (one CID, no record fetch, no extra network call).
- [ ] The `kind` (and any other protocol) strings in the CLI output come from a `werust-core` helper, not from string literals in the binary; the existing ENSIP-7 vocabulary is reused rather than a new spelling minted.
- [ ] Tests: the output formatting stays pinned by pure display-free tests (the existing `resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one` style), plus coverage of the followed-pointer case; network-isolated.
- [ ] `werust --help` and the spike docs describe the new behaviour (resolve completes the resolution; no flag).

## Prompt

> Goal: make `werust resolve <name>` return what the GUI would actually load. Today an ENS name whose contenthash is an `ipns-ns` pointer prints `ipns://<name>`, which werust's own URL bar cannot open, while the GUI follows the record and renders. Follow it through to the `ipfs://<cid>` by calling the SAME core path the GUI walks (`BrowserShell::navigate_ens_name` -> `ipns::resolve_ipns_name`, which fetches and CLIENT-VERIFIES the signed record); if that is only reachable through the shell today, lift the name-to-CID step into a callable core function and have the GUI use it too, so there is ONE resolution path. Keep the trust posture honest: a followed IPNS name is MutableName, never plain verified, so the output must still say the name was mutable and `--json` must carry BOTH the pointer and the CID. Separately, source the output's protocol vocabulary (`kind`) from a `werust-core` helper reusing the ENSIP-7 `ipfs-ns`/`ipns-ns` spelling, instead of the string literals currently minted in `crates/werust/src/main.rs`. No new flag, no new verb, no new dependency.
