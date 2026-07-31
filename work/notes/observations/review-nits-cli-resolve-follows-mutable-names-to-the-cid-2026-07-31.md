---
title: review-gate non-blocking nits for 'cli-resolve-follows-mutable-names-to-the-cid' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: cli-resolve-follows-mutable-names-to-the-cid
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'cli-resolve-follows-mutable-names-to-the-cid' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify decision 1: for a followed mutable name the CID goes to stdout and the mutability warning goes to STDERR (nothing on stdout marks it). Is stderr enough to satisfy the task clause that the human-readable output makes clear the CID came from a mutable name, for a user who redirects stderr away?
  (crates/werust/src/main.rs run_resolve prints output.note via eprintln then the bare line via println; docs/spikes/cli-resolve-follows-mutable-names-to-the-cid/DECISIONS.md decision 1. Rationale is sound (headless-cli-mode made stdout the RESULT), and --json carries mutable+pointer.)
- Ratify decision 2 and its wider blast radius: the --json object changes shape AND values in one release. kind goes from ipfs/ipns to ipfs-ns/ipns-ns, reference for a mutable name goes from ipns://<name> to ipfs://<cid>, and cid/mutable/pointer are new keys. Any external consumer pinned to the v0.2.9 shape breaks.
  (crates/werust/src/main.rs resolve_output; README.md. The agent checked .github/, docs/ and dorfl.json for consumers and found none; the reference change is the task itself, so batching the kind change with it is defensible but is a shipped-wire break worth a human nod.)
- Ratify decision 5: the headless resolve now reads the user's persisted retrieval-backend setting (via ipns::default_record_source -> retrieval::active_gateway_endpoint) and, for a mutable name, makes a gateway HTTP call. The CLI previously touched only the ENS RPC, so its output now also depends on the settings file (and the settings-dir env lever).
  (crates/werust/src/main.rs run_resolve; crates/werust-core/src/ipns.rs default_record_source. Settings access is READ-only (load_from, no write), so no shared-write isolation is owed; construction makes no network call, so the immutable path stays one CID with zero record fetches (pinned by the fetch counter test).)
- Ratify the new public core surface: module werust_core::name_resolution with resolve_name / resolve_name_with_progress / ResolvedName / NameResolutionError, and progress reported through the chrome's LoadStep rather than a new enum. Every future name-to-content caller (a fetch verb, mobile edges) inherits these names.
  (docs/spikes/cli-resolve-follows-mutable-names-to-the-cid/DECISIONS.md decisions 3 and 4. Coherent with the repo's language: wire_name mirrors debug::trust_posture_wire_name / LoadStep::wire_name / menu, and the module name matches version_resolution and retrieval. No CONTEXT.md glossary term is re-meant.)
- The comment above the new resolution call in navigate_ens_name says the pinned step is kept so a FAILURE still surfaces the stage it failed at, but fail_ens_load clears resolving_step before refresh_chrome, so a failed resolve shows Idle. Worth correcting so a later reader does not build on it.
  (crates/werust-core/src/lib.rs around line 1645 vs fail_ens_load (self.resolving_step = None) and the existing test an_ens_resolution_failure_reports_the_resolving_name_step_and_no_lingering_step, which asserts LoadStep::Idle. The wording is carried over from the pre-existing inline comment; behaviour is unchanged by this diff.)
- Two small accuracy/coverage nits: werust --help says stdout is ALWAYS the bare ipfs://<cid>, which is not true under --json; and the CLI-level fail-closed arm (unsupported contenthash -> stderr + exit 1) lost its direct unit test when resolve_output stopped returning Result.
  (crates/werust/src/main.rs usage() and the replaced test resolve_prints_the_decoded_reference_and_fails_closed_on_an_unsupported_one. The refusal itself is still pinned in core (name_resolution::an_unsupported_contenthash_is_a_named_refusal_not_a_reference), so the loss is the thin print/exit wrapper only.)
