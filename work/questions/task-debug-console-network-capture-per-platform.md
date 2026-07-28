<!-- dorfl-sidecar: item=task:debug-console-network-capture-per-platform type=task slug=debug-console-network-capture-per-platform allAnswered=false -->

## Q1

**'task:debug-console-network-capture-per-platform' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - On Android and iOS the MAIN-DOCUMENT row takes a STALE posture, so it contradicts the trust indicator on exactly the ENS/ipfs page the reconciliation was mandated for. Both mobile edges reconcile with entry.with_trust(shell.chrome().trust_posture), but ChromeState.trust_posture is a CACHED snapshot written only by refresh_chrome (werust-core/src/lib.rs:1802). Production order is: navigate -> begin() resets the posture to unverified-origin -> refresh_chrome caches that -> the WebView asks for the document -> resolve_ipfs marks the backend content-verified -> capture_network reads the still-cached unverified-origin. Pump/refresh_chrome only runs later, on didCommit/onPageStarted. So an ENS page's main-document row is stamped unverified-origin while the indicator shows name-via-trusted-rpc (and a plain ipfs page: unverified-origin vs content-verified) - i.e. the reconciliation DOWNGRADES the row below the honest per-request posture it would otherwise have had, and the two surfaces disagree, which is what forward-pointer item 3 and DECISIONS.md Decision 5 forbid. Desktop is correct because it reads the LIVE lifecycle posture (life.borrow().posture() at finished). Fix: have the mobile capture read the live load posture (the backend/renderer posture, or refresh before reading) rather than the cached chrome snapshot. The README's own manual steps (Android step 5, iOS step 6) cannot pass as written. (crates/werust-android/rust/src/lib.rs:625 entry.with_trust(self.with(|s| s.chrome().trust_posture)); crates/werust-ios/rust/src/lib.rs:321-322 if main_frame || self.shell.is_main_frame(url) { entry = entry.with_trust(self.shell.chrome().trust_posture) }; vs crates/webview-renderer/src/backend.rs:791 self.life.borrow().posture())
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:debug-console-network-capture-per-platform' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - On Android and iOS the MAIN-DOCUMENT row takes a STALE posture, so it contradicts the trust indicator on exactly the ENS/ipfs page the reconciliation was mandated for. Both mobile edges reconcile with entry.with_trust(shell.chrome().trust_posture), but ChromeState.trust_posture is a CACHED snapshot written only by refresh_chrome (werust-core/src/lib.rs:1802). Production order is: navigate -> begin() resets the posture to unverified-origin -> refresh_chrome caches that -> the WebView asks for the document -> resolve_ipfs marks the backend content-verified -> capture_network reads the still-cached unverified-origin. Pump/refresh_chrome only runs later, on didCommit/onPageStarted. So an ENS page's main-document row is stamped unverified-origin while the indicator shows name-via-trusted-rpc (and a plain ipfs page: unverified-origin vs content-verified) - i.e. the reconciliation DOWNGRADES the row below the honest per-request posture it would otherwise have had, and the two surfaces disagree, which is what forward-pointer item 3 and DECISIONS.md Decision 5 forbid. Desktop is correct because it reads the LIVE lifecycle posture (life.borrow().posture() at finished). Fix: have the mobile capture read the live load posture (the backend/renderer posture, or refresh before reading) rather than the cached chrome snapshot. The README's own manual steps (Android step 5, iOS step 6) cannot pass as written. (crates/werust-android/rust/src/lib.rs:625 entry.with_trust(self.with(|s| s.chrome().trust_posture)); crates/werust-ios/rust/src/lib.rs:321-322 if main_frame || self.shell.is_main_frame(url) { entry = entry.with_trust(self.shell.chrome().trust_posture) }; vs crates/webview-renderer/src/backend.rs:791 self.life.borrow().posture())
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:debug-console-network-capture-per-platform' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - On Android and iOS the MAIN-DOCUMENT row takes a STALE posture, so it contradicts the trust indicator on exactly the ENS/ipfs page the reconciliation was mandated for. Both mobile edges reconcile with entry.with_trust(shell.chrome().trust_posture), but ChromeState.trust_posture is a CACHED snapshot written only by refresh_chrome (werust-core/src/lib.rs:1802). Production order is: navigate -> begin() resets the posture to unverified-origin -> refresh_chrome caches that -> the WebView asks for the document -> resolve_ipfs marks the backend content-verified -> capture_network reads the still-cached unverified-origin. Pump/refresh_chrome only runs later, on didCommit/onPageStarted. So an ENS page's main-document row is stamped unverified-origin while the indicator shows name-via-trusted-rpc (and a plain ipfs page: unverified-origin vs content-verified) - i.e. the reconciliation DOWNGRADES the row below the honest per-request posture it would otherwise have had, and the two surfaces disagree, which is what forward-pointer item 3 and DECISIONS.md Decision 5 forbid. Desktop is correct because it reads the LIVE lifecycle posture (life.borrow().posture() at finished). Fix: have the mobile capture read the live load posture (the backend/renderer posture, or refresh before reading) rather than the cached chrome snapshot. The README's own manual steps (Android step 5, iOS step 6) cannot pass as written. (crates/werust-android/rust/src/lib.rs:625 entry.with_trust(self.with(|s| s.chrome().trust_posture)); crates/werust-ios/rust/src/lib.rs:321-322 if main_frame || self.shell.is_main_frame(url) { entry = entry.with_trust(self.shell.chrome().trust_posture) }; vs crates/webview-renderer/src/backend.rs:791 self.life.borrow().posture())
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:debug-console-network-capture-per-platform' was bounced — how should we proceed?**

> PR/code review (Gate 2) blocked this work:
> - On Android and iOS the MAIN-DOCUMENT row takes a STALE posture, so it contradicts the trust indicator on exactly the ENS/ipfs page the reconciliation was mandated for. Both mobile edges reconcile with entry.with_trust(shell.chrome().trust_posture), but ChromeState.trust_posture is a CACHED snapshot written only by refresh_chrome (werust-core/src/lib.rs:1802). Production order is: navigate -> begin() resets the posture to unverified-origin -> refresh_chrome caches that -> the WebView asks for the document -> resolve_ipfs marks the backend content-verified -> capture_network reads the still-cached unverified-origin. Pump/refresh_chrome only runs later, on didCommit/onPageStarted. So an ENS page's main-document row is stamped unverified-origin while the indicator shows name-via-trusted-rpc (and a plain ipfs page: unverified-origin vs content-verified) - i.e. the reconciliation DOWNGRADES the row below the honest per-request posture it would otherwise have had, and the two surfaces disagree, which is what forward-pointer item 3 and DECISIONS.md Decision 5 forbid. Desktop is correct because it reads the LIVE lifecycle posture (life.borrow().posture() at finished). Fix: have the mobile capture read the live load posture (the backend/renderer posture, or refresh before reading) rather than the cached chrome snapshot. The README's own manual steps (Android step 5, iOS step 6) cannot pass as written. (crates/werust-android/rust/src/lib.rs:625 entry.with_trust(self.with(|s| s.chrome().trust_posture)); crates/werust-ios/rust/src/lib.rs:321-322 if main_frame || self.shell.is_main_frame(url) { entry = entry.with_trust(self.shell.chrome().trust_posture) }; vs crates/webview-renderer/src/backend.rs:791 self.life.borrow().posture())
> PR/code review (Gate 2) did not reach a unanimous approve across reviewMaxRounds=2 round(s) (a block is terminal and is never re-rolled); forcing needs-attention (never silently merged or looped).

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):
