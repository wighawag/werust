---
title: review-gate non-blocking nits for 'headless-cli-mode' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: headless-cli-mode
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'headless-cli-mode' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify recorded decision 1: for a mutable ipns-ns contenthash, resolve prints the ipns://<name> pointer and deliberately does NOT follow it to a current CID. Consequence worth a human call: the flagship example (werust resolve ronan.eth) emits a reference werust itself cannot open, since bare ipns:// URL-bar entry is still an unbuilt follow-on (docs/adr/0007 decision 4), while the GUI on the same name does follow the record and renders. Ratify, or schedule a --follow/fetch that closes the gap.
  (main.rs:797 (DecodedContenthash::Ipns => format!('ipns://{name}')); docs/spikes/headless-cli-mode/DECISIONS.md decision 1; core front door at werust-core/src/lib.rs:1116 resolves the record before loading.)
- Ratify recorded decision 3: --version, -V and -h are accepted as flag aliases beyond the task's letter (which named only the version verb and --help). These argv values previously fell through as the startup URL. I checked .github/, docs/ and dorfl.json: nothing invoked werust --version, so no scripted behaviour is displaced.
  (main.rs parse_args match arms ('version' | '--version' | '-V', '--help' | '-h'); DECISIONS.md decision 3.)
- Coherence + ratification: --json mints a new machine-readable output contract inline in the binary (keys name/kind/reference, kind values ipfs and ipns) that scripts will pin, but the repo already centralises wire vocabulary (werust_core::debug::trust_posture_wire_name for the chrome JSON) and already names these protocols in core (ProtoCode::display_name, ENSIP-7 ipfs-ns / ipns-ns). Worth deciding whether the kind strings should come from a core helper (or be pinned in the CONTEXT.md glossary) so a later fetch or --follow cannot fork a second spelling. Not recorded in DECISIONS.md.
  (main.rs resolve_output + json_escape; werust-core/src/debug.rs:1046; werust-core/src/contenthash.rs:112.)
- Ratify recorded decision 4 (a malformed KNOWN verb refuses with exit 1; an unknown first argument still opens the GUI) plus one unrecorded micro-decision in the same area: --json is accepted on either side of the name, and a leading --json (werust --json resolve x) silently becomes a GUI URL rather than a usage refusal. Low impact, but it is a user-visible parser default nobody signed off.
  (main.rs parse_args resolve arm and the final catch-all _ => Command::Gui; DECISIONS.md decision 4.)
- Ratify recorded decision 5: no docs/platform-capability-matrix.toml row for the CLI. The reasoning (an invocation mode of the desktop binary, whose ENS-resolution capability already has a row implemented on all three platforms; mobile has no argv entry point so the cells could only read n-a) looks right to me, and the guard only forces cells for rows that exist, so nothing is hidden.
  (DECISIONS.md decision 5; docs/adr/0005 parity guard.)
- Coverage note: the three tests pin the pure seams (parse_args routing, resolve_output formatting incl. the fail-closed unsupported arm and JSON escaping, usage listing). run_resolve itself (RpcProvider::new wiring plus the exit-status mapping) is covered only by the manual transcript in docs/spikes/headless-cli-mode/README.md. Acceptable for 8 lines of glue over a shared constructor, but if the CLI grows a second verb it is worth a seam that lets the provider be injected.
  (main.rs run_resolve; tests are display- and network-free (nothing constructs RpcProvider), so no shared-location pollution risk.)
