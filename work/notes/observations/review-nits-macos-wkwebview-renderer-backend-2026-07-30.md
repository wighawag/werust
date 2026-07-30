---
title: review-gate non-blocking nits for 'macos-wkwebview-renderer-backend' (Gate 2 approve)
date: 2026-07-30
status: open
reviewOf: macos-wkwebview-renderer-backend
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'macos-wkwebview-renderer-backend' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY: the verifying ipfs:// route spawns one raw std::thread per intercepted request (no pool, no cap), while the WebKitGTK sibling uses gio::spawn_blocking. DECISIONS.md 7 records WHERE completions are applied (poll_event) but not this. A page with many subresources spawns one OS thread each. Ratify, or bound it before the sibling window task drives real sites.
  (crates/macos-renderer/src/backend.rs:231 std::thread::spawn per Route::OffThread start; probe run recorded 4 handler_uris for one canned page)
- RATIFY: a scheme registered after the WKWebView is realised is dropped with only a stderr line, never intercepted and never reported through the seam. Recorded in DECISIONS.md 3 with the reasoning (the trait returns unit and must not widen), but it is a new silent-ish refusal the sibling macos-appkit-window-and-chrome will inherit. Ratify the contract of register-every-scheme-before-first-navigate.
  (crates/macos-renderer/src/backend.rs:855 attach_scheme_bridge eprintln + early return)
- RATIFY: expected.json case A secure_context moved from the predicted false to the measured true, with the reason written into the provenance line rather than silently overwritten. This is the first exercise of the probe re-record contract and sets the precedent for both WebKit shells. It decides no mechanism, so it looks right, but a human should ratify the precedent.
  (docs/spikes/macos-wkwebview-renderer-backend/expected.json recorded field; DECISIONS.md 9)
- Claim-vs-reality: the README says the leg runs on pull requests when the backend, the probe or the recorded verdict changes, but the workflow pull_request path filter lists only the three crates and the workflow file. A PR that changes ONLY docs/spikes/macos-wkwebview-renderer-backend (a re-stamped expected.json) will not run the macos-14 leg. The push-to-main filter does include that path, so it self-corrects on merge; the README sentence is what is wrong.
  (.github/workflows/macos-renderer.yml pull_request.paths vs README section The recorded verdict, and re-running it)
- Coverage locality: README step 2 describes the 5 webview-shared tests as the lifecycle and off-thread-boundary tests, but crates/webview-shared/src/lifecycle.rs carries ZERO tests (the 5 are 3 offthread + 2 validate_url). The LoadLifecycle state-machine tests stayed behind in the gtk4/webkit6-bound webview-renderer, so cargo test -p webview-shared on macOS never exercises the moved state machine. Coverage still exists on the Ubuntu gate, so this is locality plus an overstated sentence, not a hole.
  (grep -c '#[test]' crates/webview-shared/src/lifecycle.rs = 0; old tests remain in crates/webview-renderer/src/lib.rs)
- Coherence nit for a later rename, not this task: crates/macos-renderer is platform-named while crates/webview-renderer is technology-named for what is now only the WebKitGTK backend, and the genuinely generic home is crates/webview-shared. With two system-webview backends the generic-sounding name is misleading, and a future WebView2 crate inherits the muddle. Worth pinning the naming rule before the Windows backend lands.
  (crates/ listing: webview-renderer, webview-shared, macos-renderer, native-renderer)
- Footgun in the committed dev harness: typecheck-macos-from-linux.sh does rm -rf on a caller-supplied SCRATCH_DIR with no guard that the path is under a temp root. Default is safe; an operator export of SCRATCH_DIR to a working directory is destructive.
  (docs/spikes/macos-wkwebview-renderer-backend/typecheck-macos-from-linux.sh:33-40)
