---
title: "Gate-3 conductor review: scheme-less-entry-https-fallback-and-keep-bar-on-error (APPROVE)"
date: 2026-07-23
status: approved
reviewOf: scheme-less-entry-https-fallback-and-keep-bar-on-error
gate: gate-3-conductor
mergedCommit: 9efd298
---

## Verdict: APPROVE

Conductor Gate-3 pass. Gate-1 + Gate-2 passed before merge (Gate-2 only after an INFRA recovery, see below). Driven in place from backlog via `dorfl do ... --allow-backlog --isolated --review --merge`. Re-verified fmt + the routing tests locally.

## Done-move + landing

- `work/tasks/backlog/scheme-less-entry-https-fallback-and-keep-bar-on-error.md` -> `done/` on origin/main (squash merge `9efd298`).
- Files: shared core (`werust-core/src/lib.rs` +417: `classify_entry`, `EntryRoute`, `invalid_entry` axis, `navigate` routing), desktop (`werust/src/main.rs`), Android (`BrowserActivity.kt`, `WerustCore.kt`, `ffi_json.rs`, `lib.rs`), iOS (`WKWebViewShellController.swift`, `WerustCore.swift`, `ffi_json.rs`, `lib.rs`), capability matrix (+27, a `scheme-less-entry-routing` row implemented on all 3), a DECISIONS.md, gate-2 nits note.

## Acceptance criteria (ticked, matching the human's clarified intent)

- [x] Scheme-less `.eth` -> ENS front door, unchanged (`eth_name_from_entry` peeled off first).
- [x] Scheme-less VALID host -> `https://` prepend (`github.com` -> `https://github.com`); explicit scheme taken literally, no double-scheme, no `ipfs://`/`http://`/`https://` hijack. Routed by `classify_entry` -> `EntryRoute::{ExplicitScheme, HttpsCandidate, Invalid}` in `BrowserShell::navigate`.
- [x] VALID target whose LOAD fails -> normal in-page browser error, bar KEEPS the attempted URL (not reset). The proceeding-navigation path pins the target; a load failure surfaces via `last_error` as before.
- [x] INVALID entry (garbage, not `.eth`, not a plausible host) -> does NOT navigate; surfaces a distinct invalid-URL state and KEEPS the typed text; bar never reset. `fail_invalid_entry` sets the new `ChromeState::invalid_entry` axis and returns `Ok(())`; each edge paints the "invalid URL" badge + red-underlined bar from that one fact.
- [x] Classifier lives in `werust-core` (one shared, unit-tested rule: `classify_entry` + `is_plausible_authority`), conservative + honest (accepts `localhost`/bare-or-port + dotted hosts + optional port/path; rejects empty, whitespace, dotless tokens, userinfo, non-numeric port). The invalid state is a NEW orthogonal axis, not a re-meaning of `last_error` (a load failure) or the trust posture.
- [x] Applied on desktop + mobile (routing in the shared core; badge + red-underline painted per edge from the shared chrome fact); capability matrix row on all 3.
- [x] Tests cover `.eth`->ENS, scheme-less valid->https, explicit->literal, valid-but-failing->in-page error + bar kept, invalid->invalid state + text kept + no navigation + no reset. Network-isolated. (Re-ran `classify_entry_*` + the invalid/https tests locally: green.)

## Conceptual-coherence catch (worth noting)

The agent REMOVED the iOS Swift edge's own `normalizeURL` (which prepended `https://` for a bare host BEFORE the core saw it). That pre-empted the core classifier: a scheme-less garbage entry became `https://garbage` and was routed as an explicit scheme -> a doomed LOAD (a load error) instead of the honest invalid-URL badge. Removing it makes all three edges pass RAW typed text so the ONE core rule decides the route. This is exactly the "one concept (entry routing), one layer (the core front door), not forked per edge" fix the task asked for. Good catch.

## Review-nits triage (Gate-2)

1. IPv6-literal host (`[::1]:8080`, `[2001:db8::1]`) classifies as Invalid (the `split_once(':')` port split + dotted-host rule do not handle brackets). Documented conservative limitation; no realistic user path hits IPv6 literals in this browser today. RATIFIED - a tiny follow-on could add bracketed-IPv6 support if ever needed; not blocking.
2. `https://` (not `http://`) default for a scheme-less host, and `localhost` accepted bare/with-port while other dotless tokens are refused. Recorded UX defaults matching Brave/Chrome/Firefox and the field finding's verbatim intent. RATIFIED.

Neither blocks.

## INFRA recovery this task required (captured for future runs)

Gate-2 failed 3x with `failed to spawn 'git': spawnSync git ENOENT` - the exact ENOENT class the recovery playbook names, but the root cause here was SPECIFIC and worth recording: the build leg (Gate-1) passed every time (agent git calls inherit `process.env`, which has `/usr/bin`), but the Gate-2 review leg spawns `pi` (the review harness) and, when `pi`/`node` resolve to the VOLTA SHIMS (`~/.volta/bin/pi` -> `volta-shim`, `~/.volta/bin/node` = an ancient node 12), the shim re-execs with a volta-managed PATH that drops `/usr/bin`, so dorfl's git spawn inside the review leg fails ENOENT. FIX: keep `/usr/local/bin` BEFORE `~/.volta/bin` on PATH so `pi` resolves to the REAL `/usr/local/bin/pi` (cli.js) and `node` to system `/usr/bin/node` - NO volta shim, NO PATH stripping. The earlier 6 tasks (and T2) worked precisely because that ordering held; my mistake was using `env -i` during recovery, which changed shim resolution and REINTRODUCED the volta pi/node. Lesson: do NOT use `env -i` for dispatch; a plain `export PATH=/usr/bin:...:/usr/local/bin:...:$HOME/.volta/bin` (real pi + system node winning) is the reliable env. No work was lost (no branch was ever pushed on the failed attempts; the build is deterministic and re-ran green).

## Net effect

Scheme-less `github.com` now loads `https://github.com`; a valid-but-unreachable host shows a normal in-page error keeping the URL; garbage shows an invalid-URL badge + red-underline keeping the typed text, never silently resetting the bar - exactly the human's v0.2.3 request. One layer, one rule, all three platforms.
