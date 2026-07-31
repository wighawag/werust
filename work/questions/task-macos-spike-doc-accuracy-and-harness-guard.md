<!-- dorfl-sidecar: item=task:macos-spike-doc-accuracy-and-harness-guard type=task slug=macos-spike-doc-accuracy-and-harness-guard allAnswered=false -->

## Q1

**'task:macos-spike-doc-accuracy-and-harness-guard' was bounced — how should we proceed?**

> acceptance gate failed (exit 101) on the rebased tip — the failing step was: `cargo fmt --check && cargo clippy && cargo build && cargo test`; its last output was:
>
> test the_blocking_verify_runs_off_the_main_thread_and_completes_on_it ... ok
> test the_ci_smoke_drives_both_trust_hooks_with_a_negative_control ... ok
> test the_backend_implements_the_whole_seam_over_wkwebview ... ok
> test this_task_built_no_chrome ... ok
> test offthread_moved_to_a_shared_toolkit_free_home_and_was_not_copied ... ok
> test the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired ... ok
> test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
>      Running tests/typecheck_harness_guard.rs (target/debug/deps/typecheck_harness_guard-4e563fd4044b12c5)
> running 2 tests
> test the_harnesss_default_scratch_dir_stays_under_a_temp_root ... ok
> test the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root ... FAILED
> failures:
> ---- the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root stdout ----
> thread 'the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root' (1577856) panicked at crates/macos-renderer/tests/typecheck_harness_guard.rs:60:5:
> the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead
> note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
> failures:
>     the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root
> test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.76s
> error: test failed, to rerun pass `-p macos-renderer --test typecheck_harness_guard`

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:macos-spike-doc-accuracy-and-harness-guard' was bounced — how should we proceed?**

> acceptance gate failed (exit 101) on the rebased tip — the failing step was: `cargo fmt --check && cargo clippy && cargo build && cargo test`; its last output was:
>
> test the_blocking_verify_runs_off_the_main_thread_and_completes_on_it ... ok
> test the_ci_smoke_drives_both_trust_hooks_with_a_negative_control ... ok
> test the_backend_implements_the_whole_seam_over_wkwebview ... ok
> test this_task_built_no_chrome ... ok
> test offthread_moved_to_a_shared_toolkit_free_home_and_was_not_copied ... ok
> test the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired ... ok
> test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
>      Running tests/typecheck_harness_guard.rs (target/debug/deps/typecheck_harness_guard-4e563fd4044b12c5)
> running 2 tests
> test the_harnesss_default_scratch_dir_stays_under_a_temp_root ... ok
> test the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root ... FAILED
> failures:
> ---- the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root stdout ----
> thread 'the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root' (1577856) panicked at crates/macos-renderer/tests/typecheck_harness_guard.rs:60:5:
> the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead
> note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
> failures:
>     the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root
> test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.76s
> error: test failed, to rerun pass `-p macos-renderer --test typecheck_harness_guard`

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:macos-spike-doc-accuracy-and-harness-guard' was bounced — how should we proceed?**

> acceptance gate failed (exit 101) on the rebased tip — the failing step was: `cargo fmt --check && cargo clippy && cargo build && cargo test`; its last output was:
>
> test the_blocking_verify_runs_off_the_main_thread_and_completes_on_it ... ok
> test the_ci_smoke_drives_both_trust_hooks_with_a_negative_control ... ok
> test the_backend_implements_the_whole_seam_over_wkwebview ... ok
> test this_task_built_no_chrome ... ok
> test offthread_moved_to_a_shared_toolkit_free_home_and_was_not_copied ... ok
> test the_load_lifecycle_history_bridge_and_scheme_hook_are_really_wired ... ok
> test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
>      Running tests/typecheck_harness_guard.rs (target/debug/deps/typecheck_harness_guard-4e563fd4044b12c5)
> running 2 tests
> test the_harnesss_default_scratch_dir_stays_under_a_temp_root ... ok
> test the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root ... FAILED
> failures:
> ---- the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root stdout ----
> thread 'the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root' (1577856) panicked at crates/macos-renderer/tests/typecheck_harness_guard.rs:60:5:
> the harness deleted a SCRATCH_DIR outside a temp root: it must refuse instead
> note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
> failures:
>     the_harness_refuses_to_delete_a_scratch_dir_outside_a_temp_root
> test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 19.76s
> error: test failed, to rerun pass `-p macos-renderer --test typecheck_harness_guard`

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):
