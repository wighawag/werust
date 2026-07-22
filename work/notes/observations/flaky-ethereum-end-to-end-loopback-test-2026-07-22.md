# Flaky loopback test: ethereum end-to-end eth_call

2026-07-22 — While running `cargo test -p werust-core` during the `ensip7-contenthash-decoder-typed-graceful-errors` task, `ethereum::tests::end_to_end_eth_call_over_the_bound_transport_off_the_network` (in `crates/werust-core/src/ethereum.rs`) failed intermittently (roughly 1 run in ~6), passing on every re-run. It is a `127.0.0.1:0` loopback HTTP fixture test; the flake looks like a timing/accept race in the throwaway `LocalRpcServer`, not a logic bug, and is unrelated to this task's pure decoder. Left untouched per scope.
