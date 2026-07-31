<!-- dorfl-sidecar: item=task:provider-refuses-honestly-instead-of-resolving-an-empty-account-list type=task slug=provider-refuses-honestly-instead-of-resolving-an-empty-account-list allAnswered=false -->

## Q1

**'task:provider-refuses-honestly-instead-of-resolving-an-empty-account-list' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The STUB_CHAIN_ID -> CHAIN_ID rename breaks the macOS-from-Linux typecheck harness: docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh builds a scratch workspace whose stand-in werust-core still declares only 'pub const STUB_CHAIN_ID', symlinks the REAL crates/macos-renderer/examples/trust_hooks_smoke.rs (which now reads werust_core::provider::CHAIN_ID at line 222), and then runs cargo clippy --target aarch64-apple-darwin --all-targets, so the example fails to resolve the symbol. Should the stand-in constant be renamed to CHAIN_ID (a one-line script edit) before this lands? (typecheck-macos-from-linux.sh:530 declares STUB_CHAIN_ID in the fake provider module; :588 symlinks the smoke; :642 runs clippy --all-targets. crates/macos-renderer/tests/typecheck_harness_guard.rs stubs cargo with 'exit 0', so Gate 1 green does not cover this. Same class the repo already burned a task on (work/tasks/done/macos-spike-doc-accuracy-and-harness-guard.md item 0: THE HARNESS IS NOW BROKEN), and the PR record claims both smokes follow the rename automatically, which is true only against the real core.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:provider-refuses-honestly-instead-of-resolving-an-empty-account-list' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The STUB_CHAIN_ID -> CHAIN_ID rename breaks the macOS-from-Linux typecheck harness: docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh builds a scratch workspace whose stand-in werust-core still declares only 'pub const STUB_CHAIN_ID', symlinks the REAL crates/macos-renderer/examples/trust_hooks_smoke.rs (which now reads werust_core::provider::CHAIN_ID at line 222), and then runs cargo clippy --target aarch64-apple-darwin --all-targets, so the example fails to resolve the symbol. Should the stand-in constant be renamed to CHAIN_ID (a one-line script edit) before this lands? (typecheck-macos-from-linux.sh:530 declares STUB_CHAIN_ID in the fake provider module; :588 symlinks the smoke; :642 runs clippy --all-targets. crates/macos-renderer/tests/typecheck_harness_guard.rs stubs cargo with 'exit 0', so Gate 1 green does not cover this. Same class the repo already burned a task on (work/tasks/done/macos-spike-doc-accuracy-and-harness-guard.md item 0: THE HARNESS IS NOW BROKEN), and the PR record claims both smokes follow the rename automatically, which is true only against the real core.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:provider-refuses-honestly-instead-of-resolving-an-empty-account-list' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The STUB_CHAIN_ID -> CHAIN_ID rename breaks the macOS-from-Linux typecheck harness: docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh builds a scratch workspace whose stand-in werust-core still declares only 'pub const STUB_CHAIN_ID', symlinks the REAL crates/macos-renderer/examples/trust_hooks_smoke.rs (which now reads werust_core::provider::CHAIN_ID at line 222), and then runs cargo clippy --target aarch64-apple-darwin --all-targets, so the example fails to resolve the symbol. Should the stand-in constant be renamed to CHAIN_ID (a one-line script edit) before this lands? (typecheck-macos-from-linux.sh:530 declares STUB_CHAIN_ID in the fake provider module; :588 symlinks the smoke; :642 runs clippy --all-targets. crates/macos-renderer/tests/typecheck_harness_guard.rs stubs cargo with 'exit 0', so Gate 1 green does not cover this. Same class the repo already burned a task on (work/tasks/done/macos-spike-doc-accuracy-and-harness-guard.md item 0: THE HARNESS IS NOW BROKEN), and the PR record claims both smokes follow the rename automatically, which is true only against the real core.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:provider-refuses-honestly-instead-of-resolving-an-empty-account-list' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The STUB_CHAIN_ID -> CHAIN_ID rename breaks the macOS-from-Linux typecheck harness: docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh builds a scratch workspace whose stand-in werust-core still declares only 'pub const STUB_CHAIN_ID', symlinks the REAL crates/macos-renderer/examples/trust_hooks_smoke.rs (which now reads werust_core::provider::CHAIN_ID at line 222), and then runs cargo clippy --target aarch64-apple-darwin --all-targets, so the example fails to resolve the symbol. Should the stand-in constant be renamed to CHAIN_ID (a one-line script edit) before this lands? (typecheck-macos-from-linux.sh:530 declares STUB_CHAIN_ID in the fake provider module; :588 symlinks the smoke; :642 runs clippy --all-targets. crates/macos-renderer/tests/typecheck_harness_guard.rs stubs cargo with 'exit 0', so Gate 1 green does not cover this. Same class the repo already burned a task on (work/tasks/done/macos-spike-doc-accuracy-and-harness-guard.md item 0: THE HARNESS IS NOW BROKEN), and the PR record claims both smokes follow the rename automatically, which is true only against the real core.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):
