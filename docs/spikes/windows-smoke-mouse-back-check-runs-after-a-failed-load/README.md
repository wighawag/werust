# Turning the `windows-renderer` leg green: the mouse-back check, given the history it asks for

Task `windows-smoke-mouse-back-check-runs-after-a-failed-load`, spec `chrome-conventional-controls` (story 7). The two failing checks on `main` were:

```
the mouse's back side button (XBUTTON1), through the real window proc:
  FAIL there is history to go back to after two loads
  FAIL mouse button 4 navigates history back through the shell
window_smoke: FAIL (2 checks)
```

Everything else passed, including the three keyboard checks driven as real messages.

## The confirmed diagnosis

The task's diagnosis was re-verified against the code before anything was touched, and it holds in full. It is a test-SEQUENCING defect; the product is behaving exactly as specified.

- **A failed load commits no document, so it adds no history entry.** The negative control (`ipfs://<tampered-cid>/`) fails inside the scheme route, and that route fails CLOSED: `ResponseSink::fail` (`crates/windows-renderer/src/backend.rs`) sets NO response at all, and the backend disables WebView2's built-in error pages, so "WebView2 shows nothing rather than substituting a document of its own". Nothing commits, so the session list is untouched.
- **A reload replaces the current entry rather than adding one**, so the three F5 checks before it add nothing either.
- Therefore the session list held exactly ONE entry (the verified page) when the section ran, and `can_go_back` was false. It is not a stale flag: it is read straight off the engine on every tick (`ChromeState::can_go_back` <- `BrowserShell::refresh_chrome` <- `Renderer::can_go_back` <- `ICoreWebView2::CanGoBack`), which is the same value the toolbar's Back button takes its enabled state from.
- **The second failure is a consequence of the first, not a second defect.** `perform_chrome_action` (`crates/werust-windows/src/window.rs`) gates `ChromeAction::GoBack` on that same flag and returns early, deliberately, so "a shortcut must not be able to drive a history move the on-screen control refuses". XBUTTON1 was received, correctly did nothing, and the smoke then burned the full 30-second `wait_until` waiting for a navigation that must never happen (the 30-second gap between the two FAIL lines in the CI log).

`perform_chrome_action`, the shortcut resolution and the side-button translation were examined and left ALONE. Nothing about the product changed in this task; the smoke asserted against a precondition it had never established.

The sibling macOS task recorded the same class of defect on its own edge, and explicitly left this one to this task (`docs/spikes/macos-smoke-blur-url-bar-does-not-end-the-field-editor/README.md`, item 3).

## What changed

Only the smoke and its shape guard.

1. **The section establishes its OWN history** (`crates/werust-windows/examples/window_smoke.rs`): it performs two SUCCESSFUL loads of two DIFFERENT verified pages, right there, before it asks whether there is history. It no longer inherits a history from the checks above it, which is what made a re-ordering (or a negative control) able to break it silently. The two loads are ordered so that NEITHER repeats the URL already committed (the window is still showing the verified page from the checks above), because a navigation to the URL already showing is one an engine is entitled to treat as a reload and REPLACE: the second fixture loads first and is what Back must land on.
2. **A second pinned fixture page** (`SECOND_PAGE`), so the two loads are genuinely two documents with two CIDs and two URLs. Going back is therefore visible in the URL bar as a change, not merely as a value that was already there.
3. **`settled_on` / `load_and_settle`** replace "wait for `LoadState::Finished`" wherever a SECOND load is being awaited. A settled previous load already sits at `Finished`, so the state alone returns instantly and reports the page before this one as this one -- the exact shape of failure this task is fixing, one level down.
4. **The failure path is bounded to 10 seconds** instead of 30. That wait can only END by timing out, so its budget is what a regression costs CI.
5. **A source-shape guard** (`crates/werust-windows/tests/windows_window_shape.rs`, `the_mouse_back_check_establishes_the_history_it_asks_for`): the section must load twice, load two different pages, still assert the precondition, still drive the real button, and cap the failure wait at 10 seconds. A SEQUENCE is invisible to the compiler and to every Linux unit test, and the Windows leg can only report FAIL after the fact, so this is the only place it can be pinned -- and it is where the failing test was written first.

The check is NOT weakened: `can_go_back` is still asserted true, and XBUTTON1 must still really move the window back to the first page.

## Decisions

- **A second FIXTURE page rather than a second PATH on the same CID.** The fixture retriever ignores the path, so `ipfs://<cid>/` and `ipfs://<cid>/two.html` would also have made two history entries at zero cost. Rejected: both URLs contain the same CID, so the back assertion (`the URL bar carries the first page's CID`) would pass whether or not the window moved, which is how this check came to prove nothing in the first place. *Touches:* this smoke's fixture only; no production code and no other task.
- **The retriever holds a LIST of `(cid, bytes)` blocks** instead of one field per page. With three pinned blocks, the per-page `else if` chain becomes the same code four times. *Alternative:* two more fields (`second_cid`, `second`), keeping the shape parallel to `crates/windows-renderer/examples/trust_hooks_smoke.rs`, which still pins two. *Touches:* this file only; the production verify inside `retrieve` is unchanged, so the negative control still fails for exactly the reason it did before.
- **10 seconds for the back wait, and 30 for the two setup loads.** The page behind is already fetched and served from memory, so a real back navigation lands in well under a second; the setup loads keep the budget every other load in this smoke gets, because a false FAIL there is worse than a slow one. *Touches:* this smoke only. It is not a product timeout.

## What proves it

- **On the ordinary Ubuntu `verify` gate:** the source-shape guard above. The Win32 half is `#[cfg(windows)]` and is never compiled there, so a sequence is all a Linux runner can judge.
- **Locally, from Linux, without a Windows box:** `docs/spikes/windows-webview2-renderer-backend/typecheck-windows-from-linux.sh` type-checks and lints the changed smoke against `x86_64-pc-windows-msvc` (`cargo xwin clippy … --examples`), clean.
- **On the `windows-renderer` leg, which is the only real evidence:** `cargo run -p werust-windows --example window_smoke`. The leg's push filter includes `crates/werust-windows/**`, so it re-runs on this change.

## Residual risks worth naming

- **A back navigation may be served from the engine's cache**, in which case the scheme route does not re-run and the trust posture is not re-derived. This check asserts the URL bar and the load state, deliberately, not the posture: the verified-posture claim is made by the load checks earlier in the smoke.
- **If WebView2 ever starts committing an error page for a failed custom-scheme main-document request**, the tampered load would begin adding a history entry. The section is unaffected, because it no longer counts on what preceded it -- which is the whole point of the change.
