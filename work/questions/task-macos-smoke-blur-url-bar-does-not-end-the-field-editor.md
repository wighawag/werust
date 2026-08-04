<!-- dorfl-sidecar: item=task:macos-smoke-blur-url-bar-does-not-end-the-field-editor type=task slug=macos-smoke-blur-url-bar-does-not-end-the-field-editor allAnswered=false -->

## Q1

**'task:macos-smoke-blur-url-bar-does-not-end-the-field-editor' was bounced — how should we proceed?**

> The task's diagnosis is incomplete at a load-bearing point, and acceptance criterion 3 as written is unachievable by the sanctioned fix.
>
> FALSE PREMISE: the task states that with a correct blur the failing check would pass ("if the editor survives the blur then shortcut_focus returns UrlBar, Escape resolves to RevertUrlBar, the bar reverts, and the check fails exactly as observed"). The converse is false. `crates/werust-macos/examples/window_smoke.rs:379-391` asserts `window.url_text() == typed` immediately after `press_key(Escape)`, with no pump in between, and `press_key -> sendEvent -> claim_key -> perform_chrome_action` is fully synchronous. On the PAGE branch, `ChromeAction::Stop` (`crates/werust-macos/src/window.rs:1052-1055`) calls `shell.stop()` and then `self.refresh_chrome()`; `refresh_chrome` (`window.rs:926-932`) always calls `Chrome::apply`, which at `window.rs:385-388` overwrites the URL field with `paint.url_text` whenever it differs, and `ChromePaint.url_text` is verbatim `ChromeState::url_text` (`crates/desktop-paint/src/lib.rs:303`), i.e. the `believed` URL. So Stop itself rewrites the bar from `typed` to `believed` before the check reads it.
>
> CONSEQUENCE 1: the assertion is unreachable in BOTH focus states, so the check is not discriminating at all: `ChromeAction::RevertUrlBar` (`window.rs:1056-1070`) also leaves the bar showing `believed`. Both branches produce identical observable text.
>
> CONSEQUENCE 2: there is therefore NO evidence that `blur_url_bar` is broken. The observed single failure is fully explained by the Stop repaint alone. Landing the `endEditingFor` fix would leave the `macos-renderer` leg RED and would teach nothing, which is the exact failure mode CONTEXT.md's "a CI-measurable criterion needs its CI leg" convention exists to avoid.
>
> WHY I AM NOT RESOLVING THIS MYSELF: the only two ways to make criterion 3 true are (a) change production repaint behaviour so a blurred, user-typed bar survives a chrome repaint - a cross-edge, user-visible product decision, since the GTK edge behaves identically (`crates/werust/src/main.rs:296-300` and `1089-1090`), and nothing in this spec asks for it; or (b) rewrite the assertion, which criteria 2/7 and the prompt explicitly reserve ("Do not weaken the assertion... this check is the whole safety net"). Both are load-bearing and unverifiable from Linux.
>
> SUGGESTED RE-SCOPE (single change, needs the criterion relaxed by a human):
> 1. Keep criterion 1: `blur_url_bar` -> `NSWindow::endEditingFor(None)` plus `makeFirstResponder` whose BOOL is honoured, not discarded.
> 2. Keep criterion 2 and make it the PRIMARY check: assert `window.reported_focus() == Focus::Page` directly after `blur_url_bar()`. This is the only assertion in the smoke that actually tests focus reporting for this case, and it is unaffected by the repaint.
> 3. REPLACE criterion 3's symptom assertion (`url_text() == typed`) with the EFFECT assertion the Windows smoke already uses (`crates/werust-windows/examples/window_smoke.rs:697-713`): with the page focused, Escape must CANCEL an in-flight load (observed at the shell / the pinned fixture), and with the bar focused, Escape must revert the edit and must NOT cancel the load. That is strictly stronger and genuinely discriminating, so it does not weaken the safety net; it just is not a change I may make unilaterally under criterion 7.
> 4. `shortcut_focus` appears correct for a real user (a real click into the page moves the first responder and ends the field editor), so no production focus-reporting change is proposed.

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:macos-smoke-blur-url-bar-does-not-end-the-field-editor' was bounced — how should we proceed?**

> The task's diagnosis is incomplete at a load-bearing point, and acceptance criterion 3 as written is unachievable by the sanctioned fix.
>
> FALSE PREMISE: the task states that with a correct blur the failing check would pass ("if the editor survives the blur then shortcut_focus returns UrlBar, Escape resolves to RevertUrlBar, the bar reverts, and the check fails exactly as observed"). The converse is false. `crates/werust-macos/examples/window_smoke.rs:379-391` asserts `window.url_text() == typed` immediately after `press_key(Escape)`, with no pump in between, and `press_key -> sendEvent -> claim_key -> perform_chrome_action` is fully synchronous. On the PAGE branch, `ChromeAction::Stop` (`crates/werust-macos/src/window.rs:1052-1055`) calls `shell.stop()` and then `self.refresh_chrome()`; `refresh_chrome` (`window.rs:926-932`) always calls `Chrome::apply`, which at `window.rs:385-388` overwrites the URL field with `paint.url_text` whenever it differs, and `ChromePaint.url_text` is verbatim `ChromeState::url_text` (`crates/desktop-paint/src/lib.rs:303`), i.e. the `believed` URL. So Stop itself rewrites the bar from `typed` to `believed` before the check reads it.
>
> CONSEQUENCE 1: the assertion is unreachable in BOTH focus states, so the check is not discriminating at all: `ChromeAction::RevertUrlBar` (`window.rs:1056-1070`) also leaves the bar showing `believed`. Both branches produce identical observable text.
>
> CONSEQUENCE 2: there is therefore NO evidence that `blur_url_bar` is broken. The observed single failure is fully explained by the Stop repaint alone. Landing the `endEditingFor` fix would leave the `macos-renderer` leg RED and would teach nothing, which is the exact failure mode CONTEXT.md's "a CI-measurable criterion needs its CI leg" convention exists to avoid.
>
> WHY I AM NOT RESOLVING THIS MYSELF: the only two ways to make criterion 3 true are (a) change production repaint behaviour so a blurred, user-typed bar survives a chrome repaint - a cross-edge, user-visible product decision, since the GTK edge behaves identically (`crates/werust/src/main.rs:296-300` and `1089-1090`), and nothing in this spec asks for it; or (b) rewrite the assertion, which criteria 2/7 and the prompt explicitly reserve ("Do not weaken the assertion... this check is the whole safety net"). Both are load-bearing and unverifiable from Linux.
>
> SUGGESTED RE-SCOPE (single change, needs the criterion relaxed by a human):
> 1. Keep criterion 1: `blur_url_bar` -> `NSWindow::endEditingFor(None)` plus `makeFirstResponder` whose BOOL is honoured, not discarded.
> 2. Keep criterion 2 and make it the PRIMARY check: assert `window.reported_focus() == Focus::Page` directly after `blur_url_bar()`. This is the only assertion in the smoke that actually tests focus reporting for this case, and it is unaffected by the repaint.
> 3. REPLACE criterion 3's symptom assertion (`url_text() == typed`) with the EFFECT assertion the Windows smoke already uses (`crates/werust-windows/examples/window_smoke.rs:697-713`): with the page focused, Escape must CANCEL an in-flight load (observed at the shell / the pinned fixture), and with the bar focused, Escape must revert the edit and must NOT cancel the load. That is strictly stronger and genuinely discriminating, so it does not weaken the safety net; it just is not a change I may make unilaterally under criterion 7.
> 4. `shortcut_focus` appears correct for a real user (a real click into the page moves the first responder and ends the field editor), so no production focus-reporting change is proposed.

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):
