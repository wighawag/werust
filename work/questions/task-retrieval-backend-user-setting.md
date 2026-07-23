<!-- dorfl-sidecar: item=task:retrieval-backend-user-setting type=task slug=retrieval-backend-user-setting allAnswered=false -->

## Q1

**'task:retrieval-backend-user-setting' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - On iOS the werust:// settings scheme is never dispatched by the OS edge, so werust://settings is unreachable there, yet the parity matrix marks retrieval-backend iOS as implemented. Add a WKURLSchemeHandler for the werust scheme (and the FFI/Swift routing to apply_settings_request), or flip the iOS cell to stubbed with a linked follow-on task. This is the exact silent-desktop/Android-only class the parity guard exists to forbid. (crates/werust-ios/App/Sources/WKWebViewShellController.swift:107 registers setURLSchemeHandler only forURLScheme: 'ipfs'; there is no werust registration and the iOS FFI only exposes werust_ios_resolve_ipfs. WKWebView will not hand an unregistered custom scheme to any handler, so the Rust-side register_scheme_handler(WERUST_SCHEME,...) in werust-ios/rust/src/lib.rs:232 is dead. docs/platform-capability-matrix.toml sets retrieval-backend ios = implemented claiming 'iOS via WKURLSchemeHandler ... the same mechanism the mobile ipfs:// interception uses' - that mechanism required an explicit setURLSchemeHandler for ipfs which was never added for werust. Acceptance criterion 5 (present on iOS or explicitly stubbed-with-linked-task) is unmet, and the criterion-2 load-path switch cannot be reached via the UI on iOS.)
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):
