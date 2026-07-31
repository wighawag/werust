<!-- dorfl-sidecar: item=task:verify-lints-test-targets-and-clears-the-existing-debt type=task slug=verify-lints-test-targets-and-clears-the-existing-debt allAnswered=false -->

## Q1

**'task:verify-lints-test-targets-and-clears-the-existing-debt' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The landed record is wrong about where the platform halves are linted, and it drops the follow-on the task explicitly asked for. The new note work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md says the only two clippy invocations in the repo are verify.yml and release.yml and that the ~7.5k platform lines are unlinted EVERYWHERE. Both cross-target harnesses already run clippy over exactly those files from Linux: docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh ends in cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples, and docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh runs three cargo clippy invocations against aarch64-apple-darwin (engine --all-targets, window --lib --examples, probe --all-targets). The task said plainly: if the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here. Neither the note nor the spike README mentions the harnesses at all, so the cheapest lever (raise the harnesses to the gate bar of --all-targets plus -D warnings, which they do not use) is invisible and the next agent inherits a false premise. Please correct the note and the README coverage section, and record the harness-parity follow-on. While there, the README says the gate compiles all 18 workspace members; Cargo.toml lists 17. (work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md vs typecheck-windows-from-linux.sh:53 and typecheck-macos-from-linux.sh (cargo clippy x3); the windows harness header even states it is the only place the cfg(windows) halves are linted at all)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:verify-lints-test-targets-and-clears-the-existing-debt' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The landed record is wrong about where the platform halves are linted, and it drops the follow-on the task explicitly asked for. The new note work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md says the only two clippy invocations in the repo are verify.yml and release.yml and that the ~7.5k platform lines are unlinted EVERYWHERE. Both cross-target harnesses already run clippy over exactly those files from Linux: docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh ends in cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples, and docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh runs three cargo clippy invocations against aarch64-apple-darwin (engine --all-targets, window --lib --examples, probe --all-targets). The task said plainly: if the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here. Neither the note nor the spike README mentions the harnesses at all, so the cheapest lever (raise the harnesses to the gate bar of --all-targets plus -D warnings, which they do not use) is invisible and the next agent inherits a false premise. Please correct the note and the README coverage section, and record the harness-parity follow-on. While there, the README says the gate compiles all 18 workspace members; Cargo.toml lists 17. (work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md vs typecheck-windows-from-linux.sh:53 and typecheck-macos-from-linux.sh (cargo clippy x3); the windows harness header even states it is the only place the cfg(windows) halves are linted at all)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:verify-lints-test-targets-and-clears-the-existing-debt' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The landed record is wrong about where the platform halves are linted, and it drops the follow-on the task explicitly asked for. The new note work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md says the only two clippy invocations in the repo are verify.yml and release.yml and that the ~7.5k platform lines are unlinted EVERYWHERE. Both cross-target harnesses already run clippy over exactly those files from Linux: docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh ends in cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples, and docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh runs three cargo clippy invocations against aarch64-apple-darwin (engine --all-targets, window --lib --examples, probe --all-targets). The task said plainly: if the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here. Neither the note nor the spike README mentions the harnesses at all, so the cheapest lever (raise the harnesses to the gate bar of --all-targets plus -D warnings, which they do not use) is invisible and the next agent inherits a false premise. Please correct the note and the README coverage section, and record the harness-parity follow-on. While there, the README says the gate compiles all 18 workspace members; Cargo.toml lists 17. (work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md vs typecheck-windows-from-linux.sh:53 and typecheck-macos-from-linux.sh (cargo clippy x3); the windows harness header even states it is the only place the cfg(windows) halves are linted at all)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:verify-lints-test-targets-and-clears-the-existing-debt' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The landed record is wrong about where the platform halves are linted, and it drops the follow-on the task explicitly asked for. The new note work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md says the only two clippy invocations in the repo are verify.yml and release.yml and that the ~7.5k platform lines are unlinted EVERYWHERE. Both cross-target harnesses already run clippy over exactly those files from Linux: docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh ends in cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples, and docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh runs three cargo clippy invocations against aarch64-apple-darwin (engine --all-targets, window --lib --examples, probe --all-targets). The task said plainly: if the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here. Neither the note nor the spike README mentions the harnesses at all, so the cheapest lever (raise the harnesses to the gate bar of --all-targets plus -D warnings, which they do not use) is invisible and the next agent inherits a false premise. Please correct the note and the README coverage section, and record the harness-parity follow-on. While there, the README says the gate compiles all 18 workspace members; Cargo.toml lists 17. (work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md vs typecheck-windows-from-linux.sh:53 and typecheck-macos-from-linux.sh (cargo clippy x3); the windows harness header even states it is the only place the cfg(windows) halves are linted at all)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):

## Q5

**'task:verify-lints-test-targets-and-clears-the-existing-debt' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - The landed record is wrong about where the platform halves are linted, and it drops the follow-on the task explicitly asked for. The new note work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md says the only two clippy invocations in the repo are verify.yml and release.yml and that the ~7.5k platform lines are unlinted EVERYWHERE. Both cross-target harnesses already run clippy over exactly those files from Linux: docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh ends in cargo xwin clippy -p windows-renderer -p werust-windows --tests --examples, and docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh runs three cargo clippy invocations against aarch64-apple-darwin (engine --all-targets, window --lib --examples, probe --all-targets). The task said plainly: if the cross-target type-check harnesses can cheaply lint them from Linux, note that as the follow-on rather than doing it here. Neither the note nor the spike README mentions the harnesses at all, so the cheapest lever (raise the harnesses to the gate bar of --all-targets plus -D warnings, which they do not use) is invisible and the next agent inherits a false premise. Please correct the note and the README coverage section, and record the harness-parity follow-on. While there, the README says the gate compiles all 18 workspace members; Cargo.toml lists 17. (work/notes/observations/platform-ci-legs-never-run-clippy-2026-07-31.md vs typecheck-windows-from-linux.sh:53 and typecheck-macos-from-linux.sh (cargo clippy x3); the windows harness header even states it is the only place the cfg(windows) halves are linted at all)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q5 fields: id=q5 kind=stuck -->

**Your answer** (write below this line):
