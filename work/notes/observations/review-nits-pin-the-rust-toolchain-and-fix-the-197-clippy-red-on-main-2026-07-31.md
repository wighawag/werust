---
title: review-gate non-blocking nits for 'pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- Ratify the unspecified default profile = 'minimal' in rust-toolchain.toml: any machine or CI leg that lacks 1.97.0 now installs it WITHOUT rust-docs/rust-analyzer, which changes the local dev environment for everyone in the repo, not just the gate.
  (rust-toolchain.toml: profile = 'minimal' (task specified only channel + rustfmt/clippy components))
- Ratify a new cross-task refusal: the shape guard reds verify if ANY .github YAML contains cargo +, rustc +, rustup update, rustup toolchain install, rustup default or rustup override, or three setup-toolchain actions. A future leg that legitimately needs nightly (cargo +nightly fmt, miri, a nightly-only sanitizer) now cannot land without amending the guard. Is that the intended escape hatch, and should it be named in the spike?
  (crates/werust-core/tests/toolchain_pin_shape.rs, FORBIDDEN_RUN_FRAGMENTS / FORBIDDEN_ACTIONS)
- The guard walks .github YAML only, so committed shell scripts that call cargo are unguarded; a cargo +stable added to one of them would silently reopen the drift the pin closes. Worth extending the scan or noting the gap.
  (crates/werust-macos/bundle-app.sh, crates/werust-ios/App/build-rust-lib.sh, docs/spikes/*/typecheck-*-from-linux.sh all invoke cargo/rustup outside the guard's walk of .github)
- Doc-accuracy drift: rust-toolchain.toml says every CI leg pays the toolchain download 'on a cache miss', but no workflow caches ~/.rustup (verify.yml caches ~/.cargo/registry, ~/.cargo/git, target), so the pinned toolchain is fetched on EVERY run of every leg whose image ships a different version. Small but recurring CI cost stated as conditional.
  (rust-toolchain.toml profile comment vs .github/workflows/verify.yml cache paths (lines 45-55))
- A sibling record is now stale: docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md line 49 still states as a live exposure that the toolchain is not pinned and that rustup component add takes whatever the runner has. A one-line closed-by pointer would stop the next agent inheriting a false premise.
  (docs/spikes/verify-lints-test-targets-and-clears-the-existing-debt/README.md:49)
- Acceptance clause 'main verify goes GREEN after this lands' cannot be established pre-merge and is not claimed as measured. Confirm it as the conductor's post-merge step.
  (work/tasks/done/pin-the-rust-toolchain-and-fix-the-197-clippy-red-on-main.md, last acceptance box)
- No '## Decisions' block was found in the PR description available here (only a stale unrelated /tmp/pr-body.md), so the four ratification findings above were hunted rather than ratified from the agent's own list.
  (git log -1 body is the bare conventional-commit subject; no Decisions section)
