<!-- dorfl-sidecar: item=task:fix-desktop-create-signal-crash-on-blank-links type=task slug=fix-desktop-create-signal-crash-on-blank-links allAnswered=false -->

## Q1

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> The task's diagnosis of the CRASH is correct and confirmed, but its prescribed FIX rests on two premises that are false against real WebKitGTK 2.52.3 / webkit6 0.5.0, verified empirically with a scratch GTK harness (built outside the repo, deleted; no source changed):
>
> FALSE PREMISE 1 - "route the new-window request via connect_decide_policy instead" does NOT cover `window.open`.
> `window.open(url)` NEVER emits a `decide-policy` `PolicyDecisionType::NewWindowAction`. Measured: with only a `connect_decide_policy` handler installed, a `window.open('https://example.org/...')` click produced NO `NewWindowAction` decision at all and the view stayed on the source page (dead link); the same handler DID fire correctly for a `target="_blank"` link click and loaded it in place. WebKit source confirms the mechanism: `LocalDOMWindow::open` -> `WebCore::createWindow` (Source/WebCore/loader/FrameLoader.cpp:5031) -> `Chrome::createWindow` -> the GTK `create` signal (Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp:2896 `webkitWebViewCreateNewPage`), which never passes through `PolicyChecker::checkNewWindowPolicy`. Only the FrameLoader named-target path (a `_blank` link click) calls `checkNewWindowPolicy` (FrameLoader.cpp:1717/3745), which is what produces `NEW_WINDOW_ACTION`. Verified this is not a settings artifact: `window.open` reaches `create` both with and without `javascript_can_open_windows_automatically` (werust's default is unset/false).
> Consequence: implementing the task verbatim (remove `connect_create`, add decide-policy `NewWindowAction`) fixes the `_blank` crash but SILENTLY REGRESSES `window.open(url)` back to the "does nothing" dead-link behaviour of field finding C. That contradicts three durable artifacts that would be left stating something untrue: `docs/adr/0010` ("a `target=_blank` link OR a `window.open(url)` call navigates IN THE CURRENT view"), the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml` (`desktop = implemented`, description names `window.open(url)`), and the shared `renderer::new_window_action` rule + its `window.open`-framed seam tests. The task's acceptance criteria never mention `window.open`, so it is under-specified exactly where it silently drops a shipped capability.
>
> FALSE PREMISE 2 - "The webkit6 binding types connect_create as returning a non-nullable gtk::Widget, so you cannot return NULL through it."
> True of the TYPED wrapper (`webkit6-0.5.0/src/auto/web_view.rs:1690`, `Fn(&Self, &NavigationAction) -> gtk::Widget`), but NOT true of the signal. `view.connect_local("create", false, |args| Some(None::<gtk4::Widget>.to_value()))` returns a real NULL GtkWidget through the raw glib signal API. Measured: it does NOT abort, and it handles BOTH triggers in place - `_blank` -> FINAL uri=https://example.org/opened-in-blank, `window.open` -> FINAL uri=https://example.org/opened-via-window-open. This is exactly the documented contract ("a newly allocated WebKitWebView widget or NULL to propagate the event further"), and it keeps ONE hook covering both cases, matching ADR-0010's recorded desktop mechanism instead of forking it.
>
> CRASH REPRO (confirms the task's diagnosis, for whoever picks this up): current shape, `connect_create` returning `view.clone().upcast::<gtk4::Widget>()`, on a `_blank` click AND on `window.open` -> `HANDLER create target=Some(...)` then `/usr/lib/gcc/.../include/c++/14/optional:482: ... _Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...: Assertion 'this->_M_is_engaged()' failed. Aborted`. So the crash is real and fires on BOTH triggers, not just `_blank`.
>
> WHY I STOPPED RATHER THAN PICKED ONE: the remaining choice is a DESIGN decision that re-decides ADR-0010's recorded desktop mechanism and changes a capability-matrix row, i.e. it touches artifacts outside this task:
>   (a) decide-policy only (as written): fixes `_blank`, SILENTLY REGRESSES `window.open`; ADR-0010 + matrix + spike README must be amended to admit `window.open` is no longer handled on desktop, and the parity claim vs iOS/Android (whose native hooks DO cover `window.open`) breaks.
>   (b) raw `connect_local("create", ...)` returning NULL: fixes the crash AND keeps BOTH `_blank` and `window.open` in place through the SAME single hook, preserving ADR-0010 and the matrix row unchanged; cost is dropping the typed binding for one signal (a raw `connect_local` with a hand-rolled `NavigationAction` arg extraction) and a comment explaining why the typed `connect_create` cannot be used.
>   (c) both: decide-policy for `_blank` + raw-create-NULL for `window.open`. Verified to work, but two hooks for one rule; the `_blank` case is then handled at decide-policy and `create` never fires for it, so the second hook exists solely for `window.open`.
> My measurements favour (b) - one hook, no capability regression, no ADR/matrix churn - but choosing it silently would re-mean the task's explicit "route via decide-policy" instruction and quietly overturn its stated binding constraint, which is precisely the kind of load-bearing mechanism decision that should be ratified, not buried in code.
>
> SUGGESTED RE-SCOPE: keep the same goal (kill the SIGABRT, in-place `_blank`/`window.open`, same `load_uri` so `ipfs://` stays hash-verified and unsupported schemes refused, no second window, bar follows the URL) but replace the prescribed mechanism with: "handle `create` via the RAW glib signal (`connect_local("create", ...)`) returning a NULL Widget - the documented no-new-view answer - because the typed `connect_create` wrapper's non-nullable `gtk::Widget` return is what forced the crashing return-the-existing-view shape. Do NOT use decide-policy as the sole route: `window.open` does not emit `NewWindowAction`." Add an explicit acceptance criterion that BOTH a `target="_blank"` link AND a `window.open(url)` load in place without aborting, and a criterion that `docs/adr/0010` + the `blank-window-open-navigates-in-place` matrix row + `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md` are updated to the corrected mechanism (they currently all describe "returns the EXISTING view widget", the crashing shape). If instead the human prefers (a) or (c), say so explicitly in the task, because (a) requires consciously accepting and documenting a `window.open` regression on desktop.

<!-- q1 fields: id=q1 kind=stuck -->

**Your answer** (write below this line):

## Q2

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> The task's diagnosis of the CRASH is correct and confirmed, but its prescribed FIX rests on two premises that are false against real WebKitGTK 2.52.3 / webkit6 0.5.0, verified empirically with a scratch GTK harness (built outside the repo, deleted; no source changed):
>
> FALSE PREMISE 1 - "route the new-window request via connect_decide_policy instead" does NOT cover `window.open`.
> `window.open(url)` NEVER emits a `decide-policy` `PolicyDecisionType::NewWindowAction`. Measured: with only a `connect_decide_policy` handler installed, a `window.open('https://example.org/...')` click produced NO `NewWindowAction` decision at all and the view stayed on the source page (dead link); the same handler DID fire correctly for a `target="_blank"` link click and loaded it in place. WebKit source confirms the mechanism: `LocalDOMWindow::open` -> `WebCore::createWindow` (Source/WebCore/loader/FrameLoader.cpp:5031) -> `Chrome::createWindow` -> the GTK `create` signal (Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp:2896 `webkitWebViewCreateNewPage`), which never passes through `PolicyChecker::checkNewWindowPolicy`. Only the FrameLoader named-target path (a `_blank` link click) calls `checkNewWindowPolicy` (FrameLoader.cpp:1717/3745), which is what produces `NEW_WINDOW_ACTION`. Verified this is not a settings artifact: `window.open` reaches `create` both with and without `javascript_can_open_windows_automatically` (werust's default is unset/false).
> Consequence: implementing the task verbatim (remove `connect_create`, add decide-policy `NewWindowAction`) fixes the `_blank` crash but SILENTLY REGRESSES `window.open(url)` back to the "does nothing" dead-link behaviour of field finding C. That contradicts three durable artifacts that would be left stating something untrue: `docs/adr/0010` ("a `target=_blank` link OR a `window.open(url)` call navigates IN THE CURRENT view"), the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml` (`desktop = implemented`, description names `window.open(url)`), and the shared `renderer::new_window_action` rule + its `window.open`-framed seam tests. The task's acceptance criteria never mention `window.open`, so it is under-specified exactly where it silently drops a shipped capability.
>
> FALSE PREMISE 2 - "The webkit6 binding types connect_create as returning a non-nullable gtk::Widget, so you cannot return NULL through it."
> True of the TYPED wrapper (`webkit6-0.5.0/src/auto/web_view.rs:1690`, `Fn(&Self, &NavigationAction) -> gtk::Widget`), but NOT true of the signal. `view.connect_local("create", false, |args| Some(None::<gtk4::Widget>.to_value()))` returns a real NULL GtkWidget through the raw glib signal API. Measured: it does NOT abort, and it handles BOTH triggers in place - `_blank` -> FINAL uri=https://example.org/opened-in-blank, `window.open` -> FINAL uri=https://example.org/opened-via-window-open. This is exactly the documented contract ("a newly allocated WebKitWebView widget or NULL to propagate the event further"), and it keeps ONE hook covering both cases, matching ADR-0010's recorded desktop mechanism instead of forking it.
>
> CRASH REPRO (confirms the task's diagnosis, for whoever picks this up): current shape, `connect_create` returning `view.clone().upcast::<gtk4::Widget>()`, on a `_blank` click AND on `window.open` -> `HANDLER create target=Some(...)` then `/usr/lib/gcc/.../include/c++/14/optional:482: ... _Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...: Assertion 'this->_M_is_engaged()' failed. Aborted`. So the crash is real and fires on BOTH triggers, not just `_blank`.
>
> WHY I STOPPED RATHER THAN PICKED ONE: the remaining choice is a DESIGN decision that re-decides ADR-0010's recorded desktop mechanism and changes a capability-matrix row, i.e. it touches artifacts outside this task:
>   (a) decide-policy only (as written): fixes `_blank`, SILENTLY REGRESSES `window.open`; ADR-0010 + matrix + spike README must be amended to admit `window.open` is no longer handled on desktop, and the parity claim vs iOS/Android (whose native hooks DO cover `window.open`) breaks.
>   (b) raw `connect_local("create", ...)` returning NULL: fixes the crash AND keeps BOTH `_blank` and `window.open` in place through the SAME single hook, preserving ADR-0010 and the matrix row unchanged; cost is dropping the typed binding for one signal (a raw `connect_local` with a hand-rolled `NavigationAction` arg extraction) and a comment explaining why the typed `connect_create` cannot be used.
>   (c) both: decide-policy for `_blank` + raw-create-NULL for `window.open`. Verified to work, but two hooks for one rule; the `_blank` case is then handled at decide-policy and `create` never fires for it, so the second hook exists solely for `window.open`.
> My measurements favour (b) - one hook, no capability regression, no ADR/matrix churn - but choosing it silently would re-mean the task's explicit "route via decide-policy" instruction and quietly overturn its stated binding constraint, which is precisely the kind of load-bearing mechanism decision that should be ratified, not buried in code.
>
> SUGGESTED RE-SCOPE: keep the same goal (kill the SIGABRT, in-place `_blank`/`window.open`, same `load_uri` so `ipfs://` stays hash-verified and unsupported schemes refused, no second window, bar follows the URL) but replace the prescribed mechanism with: "handle `create` via the RAW glib signal (`connect_local("create", ...)`) returning a NULL Widget - the documented no-new-view answer - because the typed `connect_create` wrapper's non-nullable `gtk::Widget` return is what forced the crashing return-the-existing-view shape. Do NOT use decide-policy as the sole route: `window.open` does not emit `NewWindowAction`." Add an explicit acceptance criterion that BOTH a `target="_blank"` link AND a `window.open(url)` load in place without aborting, and a criterion that `docs/adr/0010` + the `blank-window-open-navigates-in-place` matrix row + `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md` are updated to the corrected mechanism (they currently all describe "returns the EXISTING view widget", the crashing shape). If instead the human prefers (a) or (c), say so explicitly in the task, because (a) requires consciously accepting and documenting a `window.open` regression on desktop.

<!-- q2 fields: id=q2 kind=stuck -->

**Your answer** (write below this line):

## Q3

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> The task's diagnosis of the CRASH is correct and confirmed, but its prescribed FIX rests on two premises that are false against real WebKitGTK 2.52.3 / webkit6 0.5.0, verified empirically with a scratch GTK harness (built outside the repo, deleted; no source changed):
>
> FALSE PREMISE 1 - "route the new-window request via connect_decide_policy instead" does NOT cover `window.open`.
> `window.open(url)` NEVER emits a `decide-policy` `PolicyDecisionType::NewWindowAction`. Measured: with only a `connect_decide_policy` handler installed, a `window.open('https://example.org/...')` click produced NO `NewWindowAction` decision at all and the view stayed on the source page (dead link); the same handler DID fire correctly for a `target="_blank"` link click and loaded it in place. WebKit source confirms the mechanism: `LocalDOMWindow::open` -> `WebCore::createWindow` (Source/WebCore/loader/FrameLoader.cpp:5031) -> `Chrome::createWindow` -> the GTK `create` signal (Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp:2896 `webkitWebViewCreateNewPage`), which never passes through `PolicyChecker::checkNewWindowPolicy`. Only the FrameLoader named-target path (a `_blank` link click) calls `checkNewWindowPolicy` (FrameLoader.cpp:1717/3745), which is what produces `NEW_WINDOW_ACTION`. Verified this is not a settings artifact: `window.open` reaches `create` both with and without `javascript_can_open_windows_automatically` (werust's default is unset/false).
> Consequence: implementing the task verbatim (remove `connect_create`, add decide-policy `NewWindowAction`) fixes the `_blank` crash but SILENTLY REGRESSES `window.open(url)` back to the "does nothing" dead-link behaviour of field finding C. That contradicts three durable artifacts that would be left stating something untrue: `docs/adr/0010` ("a `target=_blank` link OR a `window.open(url)` call navigates IN THE CURRENT view"), the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml` (`desktop = implemented`, description names `window.open(url)`), and the shared `renderer::new_window_action` rule + its `window.open`-framed seam tests. The task's acceptance criteria never mention `window.open`, so it is under-specified exactly where it silently drops a shipped capability.
>
> FALSE PREMISE 2 - "The webkit6 binding types connect_create as returning a non-nullable gtk::Widget, so you cannot return NULL through it."
> True of the TYPED wrapper (`webkit6-0.5.0/src/auto/web_view.rs:1690`, `Fn(&Self, &NavigationAction) -> gtk::Widget`), but NOT true of the signal. `view.connect_local("create", false, |args| Some(None::<gtk4::Widget>.to_value()))` returns a real NULL GtkWidget through the raw glib signal API. Measured: it does NOT abort, and it handles BOTH triggers in place - `_blank` -> FINAL uri=https://example.org/opened-in-blank, `window.open` -> FINAL uri=https://example.org/opened-via-window-open. This is exactly the documented contract ("a newly allocated WebKitWebView widget or NULL to propagate the event further"), and it keeps ONE hook covering both cases, matching ADR-0010's recorded desktop mechanism instead of forking it.
>
> CRASH REPRO (confirms the task's diagnosis, for whoever picks this up): current shape, `connect_create` returning `view.clone().upcast::<gtk4::Widget>()`, on a `_blank` click AND on `window.open` -> `HANDLER create target=Some(...)` then `/usr/lib/gcc/.../include/c++/14/optional:482: ... _Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...: Assertion 'this->_M_is_engaged()' failed. Aborted`. So the crash is real and fires on BOTH triggers, not just `_blank`.
>
> WHY I STOPPED RATHER THAN PICKED ONE: the remaining choice is a DESIGN decision that re-decides ADR-0010's recorded desktop mechanism and changes a capability-matrix row, i.e. it touches artifacts outside this task:
>   (a) decide-policy only (as written): fixes `_blank`, SILENTLY REGRESSES `window.open`; ADR-0010 + matrix + spike README must be amended to admit `window.open` is no longer handled on desktop, and the parity claim vs iOS/Android (whose native hooks DO cover `window.open`) breaks.
>   (b) raw `connect_local("create", ...)` returning NULL: fixes the crash AND keeps BOTH `_blank` and `window.open` in place through the SAME single hook, preserving ADR-0010 and the matrix row unchanged; cost is dropping the typed binding for one signal (a raw `connect_local` with a hand-rolled `NavigationAction` arg extraction) and a comment explaining why the typed `connect_create` cannot be used.
>   (c) both: decide-policy for `_blank` + raw-create-NULL for `window.open`. Verified to work, but two hooks for one rule; the `_blank` case is then handled at decide-policy and `create` never fires for it, so the second hook exists solely for `window.open`.
> My measurements favour (b) - one hook, no capability regression, no ADR/matrix churn - but choosing it silently would re-mean the task's explicit "route via decide-policy" instruction and quietly overturn its stated binding constraint, which is precisely the kind of load-bearing mechanism decision that should be ratified, not buried in code.
>
> SUGGESTED RE-SCOPE: keep the same goal (kill the SIGABRT, in-place `_blank`/`window.open`, same `load_uri` so `ipfs://` stays hash-verified and unsupported schemes refused, no second window, bar follows the URL) but replace the prescribed mechanism with: "handle `create` via the RAW glib signal (`connect_local("create", ...)`) returning a NULL Widget - the documented no-new-view answer - because the typed `connect_create` wrapper's non-nullable `gtk::Widget` return is what forced the crashing return-the-existing-view shape. Do NOT use decide-policy as the sole route: `window.open` does not emit `NewWindowAction`." Add an explicit acceptance criterion that BOTH a `target="_blank"` link AND a `window.open(url)` load in place without aborting, and a criterion that `docs/adr/0010` + the `blank-window-open-navigates-in-place` matrix row + `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md` are updated to the corrected mechanism (they currently all describe "returns the EXISTING view widget", the crashing shape). If instead the human prefers (a) or (c), say so explicitly in the task, because (a) requires consciously accepting and documenting a `window.open` regression on desktop.

<!-- q3 fields: id=q3 kind=stuck -->

**Your answer** (write below this line):

## Q4

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> The task's diagnosis of the CRASH is correct and confirmed, but its prescribed FIX rests on two premises that are false against real WebKitGTK 2.52.3 / webkit6 0.5.0, verified empirically with a scratch GTK harness (built outside the repo, deleted; no source changed):
>
> FALSE PREMISE 1 - "route the new-window request via connect_decide_policy instead" does NOT cover `window.open`.
> `window.open(url)` NEVER emits a `decide-policy` `PolicyDecisionType::NewWindowAction`. Measured: with only a `connect_decide_policy` handler installed, a `window.open('https://example.org/...')` click produced NO `NewWindowAction` decision at all and the view stayed on the source page (dead link); the same handler DID fire correctly for a `target="_blank"` link click and loaded it in place. WebKit source confirms the mechanism: `LocalDOMWindow::open` -> `WebCore::createWindow` (Source/WebCore/loader/FrameLoader.cpp:5031) -> `Chrome::createWindow` -> the GTK `create` signal (Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp:2896 `webkitWebViewCreateNewPage`), which never passes through `PolicyChecker::checkNewWindowPolicy`. Only the FrameLoader named-target path (a `_blank` link click) calls `checkNewWindowPolicy` (FrameLoader.cpp:1717/3745), which is what produces `NEW_WINDOW_ACTION`. Verified this is not a settings artifact: `window.open` reaches `create` both with and without `javascript_can_open_windows_automatically` (werust's default is unset/false).
> Consequence: implementing the task verbatim (remove `connect_create`, add decide-policy `NewWindowAction`) fixes the `_blank` crash but SILENTLY REGRESSES `window.open(url)` back to the "does nothing" dead-link behaviour of field finding C. That contradicts three durable artifacts that would be left stating something untrue: `docs/adr/0010` ("a `target=_blank` link OR a `window.open(url)` call navigates IN THE CURRENT view"), the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml` (`desktop = implemented`, description names `window.open(url)`), and the shared `renderer::new_window_action` rule + its `window.open`-framed seam tests. The task's acceptance criteria never mention `window.open`, so it is under-specified exactly where it silently drops a shipped capability.
>
> FALSE PREMISE 2 - "The webkit6 binding types connect_create as returning a non-nullable gtk::Widget, so you cannot return NULL through it."
> True of the TYPED wrapper (`webkit6-0.5.0/src/auto/web_view.rs:1690`, `Fn(&Self, &NavigationAction) -> gtk::Widget`), but NOT true of the signal. `view.connect_local("create", false, |args| Some(None::<gtk4::Widget>.to_value()))` returns a real NULL GtkWidget through the raw glib signal API. Measured: it does NOT abort, and it handles BOTH triggers in place - `_blank` -> FINAL uri=https://example.org/opened-in-blank, `window.open` -> FINAL uri=https://example.org/opened-via-window-open. This is exactly the documented contract ("a newly allocated WebKitWebView widget or NULL to propagate the event further"), and it keeps ONE hook covering both cases, matching ADR-0010's recorded desktop mechanism instead of forking it.
>
> CRASH REPRO (confirms the task's diagnosis, for whoever picks this up): current shape, `connect_create` returning `view.clone().upcast::<gtk4::Widget>()`, on a `_blank` click AND on `window.open` -> `HANDLER create target=Some(...)` then `/usr/lib/gcc/.../include/c++/14/optional:482: ... _Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...: Assertion 'this->_M_is_engaged()' failed. Aborted`. So the crash is real and fires on BOTH triggers, not just `_blank`.
>
> WHY I STOPPED RATHER THAN PICKED ONE: the remaining choice is a DESIGN decision that re-decides ADR-0010's recorded desktop mechanism and changes a capability-matrix row, i.e. it touches artifacts outside this task:
>   (a) decide-policy only (as written): fixes `_blank`, SILENTLY REGRESSES `window.open`; ADR-0010 + matrix + spike README must be amended to admit `window.open` is no longer handled on desktop, and the parity claim vs iOS/Android (whose native hooks DO cover `window.open`) breaks.
>   (b) raw `connect_local("create", ...)` returning NULL: fixes the crash AND keeps BOTH `_blank` and `window.open` in place through the SAME single hook, preserving ADR-0010 and the matrix row unchanged; cost is dropping the typed binding for one signal (a raw `connect_local` with a hand-rolled `NavigationAction` arg extraction) and a comment explaining why the typed `connect_create` cannot be used.
>   (c) both: decide-policy for `_blank` + raw-create-NULL for `window.open`. Verified to work, but two hooks for one rule; the `_blank` case is then handled at decide-policy and `create` never fires for it, so the second hook exists solely for `window.open`.
> My measurements favour (b) - one hook, no capability regression, no ADR/matrix churn - but choosing it silently would re-mean the task's explicit "route via decide-policy" instruction and quietly overturn its stated binding constraint, which is precisely the kind of load-bearing mechanism decision that should be ratified, not buried in code.
>
> SUGGESTED RE-SCOPE: keep the same goal (kill the SIGABRT, in-place `_blank`/`window.open`, same `load_uri` so `ipfs://` stays hash-verified and unsupported schemes refused, no second window, bar follows the URL) but replace the prescribed mechanism with: "handle `create` via the RAW glib signal (`connect_local("create", ...)`) returning a NULL Widget - the documented no-new-view answer - because the typed `connect_create` wrapper's non-nullable `gtk::Widget` return is what forced the crashing return-the-existing-view shape. Do NOT use decide-policy as the sole route: `window.open` does not emit `NewWindowAction`." Add an explicit acceptance criterion that BOTH a `target="_blank"` link AND a `window.open(url)` load in place without aborting, and a criterion that `docs/adr/0010` + the `blank-window-open-navigates-in-place` matrix row + `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md` are updated to the corrected mechanism (they currently all describe "returns the EXISTING view widget", the crashing shape). If instead the human prefers (a) or (c), say so explicitly in the task, because (a) requires consciously accepting and documenting a `window.open` regression on desktop.

<!-- q4 fields: id=q4 kind=stuck -->

**Your answer** (write below this line):

## Q5

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> The task's diagnosis of the CRASH is correct and confirmed, but its prescribed FIX rests on two premises that are false against real WebKitGTK 2.52.3 / webkit6 0.5.0, verified empirically with a scratch GTK harness (built outside the repo, deleted; no source changed):
>
> FALSE PREMISE 1 - "route the new-window request via connect_decide_policy instead" does NOT cover `window.open`.
> `window.open(url)` NEVER emits a `decide-policy` `PolicyDecisionType::NewWindowAction`. Measured: with only a `connect_decide_policy` handler installed, a `window.open('https://example.org/...')` click produced NO `NewWindowAction` decision at all and the view stayed on the source page (dead link); the same handler DID fire correctly for a `target="_blank"` link click and loaded it in place. WebKit source confirms the mechanism: `LocalDOMWindow::open` -> `WebCore::createWindow` (Source/WebCore/loader/FrameLoader.cpp:5031) -> `Chrome::createWindow` -> the GTK `create` signal (Source/WebKit/UIProcess/API/glib/WebKitWebView.cpp:2896 `webkitWebViewCreateNewPage`), which never passes through `PolicyChecker::checkNewWindowPolicy`. Only the FrameLoader named-target path (a `_blank` link click) calls `checkNewWindowPolicy` (FrameLoader.cpp:1717/3745), which is what produces `NEW_WINDOW_ACTION`. Verified this is not a settings artifact: `window.open` reaches `create` both with and without `javascript_can_open_windows_automatically` (werust's default is unset/false).
> Consequence: implementing the task verbatim (remove `connect_create`, add decide-policy `NewWindowAction`) fixes the `_blank` crash but SILENTLY REGRESSES `window.open(url)` back to the "does nothing" dead-link behaviour of field finding C. That contradicts three durable artifacts that would be left stating something untrue: `docs/adr/0010` ("a `target=_blank` link OR a `window.open(url)` call navigates IN THE CURRENT view"), the `blank-window-open-navigates-in-place` row in `docs/platform-capability-matrix.toml` (`desktop = implemented`, description names `window.open(url)`), and the shared `renderer::new_window_action` rule + its `window.open`-framed seam tests. The task's acceptance criteria never mention `window.open`, so it is under-specified exactly where it silently drops a shipped capability.
>
> FALSE PREMISE 2 - "The webkit6 binding types connect_create as returning a non-nullable gtk::Widget, so you cannot return NULL through it."
> True of the TYPED wrapper (`webkit6-0.5.0/src/auto/web_view.rs:1690`, `Fn(&Self, &NavigationAction) -> gtk::Widget`), but NOT true of the signal. `view.connect_local("create", false, |args| Some(None::<gtk4::Widget>.to_value()))` returns a real NULL GtkWidget through the raw glib signal API. Measured: it does NOT abort, and it handles BOTH triggers in place - `_blank` -> FINAL uri=https://example.org/opened-in-blank, `window.open` -> FINAL uri=https://example.org/opened-via-window-open. This is exactly the documented contract ("a newly allocated WebKitWebView widget or NULL to propagate the event further"), and it keeps ONE hook covering both cases, matching ADR-0010's recorded desktop mechanism instead of forking it.
>
> CRASH REPRO (confirms the task's diagnosis, for whoever picks this up): current shape, `connect_create` returning `view.clone().upcast::<gtk4::Widget>()`, on a `_blank` click AND on `window.open` -> `HANDLER create target=Some(...)` then `/usr/lib/gcc/.../include/c++/14/optional:482: ... _Optional_base_impl<WebCore::WindowFeatures, ...>::_M_get() ...: Assertion 'this->_M_is_engaged()' failed. Aborted`. So the crash is real and fires on BOTH triggers, not just `_blank`.
>
> WHY I STOPPED RATHER THAN PICKED ONE: the remaining choice is a DESIGN decision that re-decides ADR-0010's recorded desktop mechanism and changes a capability-matrix row, i.e. it touches artifacts outside this task:
>   (a) decide-policy only (as written): fixes `_blank`, SILENTLY REGRESSES `window.open`; ADR-0010 + matrix + spike README must be amended to admit `window.open` is no longer handled on desktop, and the parity claim vs iOS/Android (whose native hooks DO cover `window.open`) breaks.
>   (b) raw `connect_local("create", ...)` returning NULL: fixes the crash AND keeps BOTH `_blank` and `window.open` in place through the SAME single hook, preserving ADR-0010 and the matrix row unchanged; cost is dropping the typed binding for one signal (a raw `connect_local` with a hand-rolled `NavigationAction` arg extraction) and a comment explaining why the typed `connect_create` cannot be used.
>   (c) both: decide-policy for `_blank` + raw-create-NULL for `window.open`. Verified to work, but two hooks for one rule; the `_blank` case is then handled at decide-policy and `create` never fires for it, so the second hook exists solely for `window.open`.
> My measurements favour (b) - one hook, no capability regression, no ADR/matrix churn - but choosing it silently would re-mean the task's explicit "route via decide-policy" instruction and quietly overturn its stated binding constraint, which is precisely the kind of load-bearing mechanism decision that should be ratified, not buried in code.
>
> SUGGESTED RE-SCOPE: keep the same goal (kill the SIGABRT, in-place `_blank`/`window.open`, same `load_uri` so `ipfs://` stays hash-verified and unsupported schemes refused, no second window, bar follows the URL) but replace the prescribed mechanism with: "handle `create` via the RAW glib signal (`connect_local("create", ...)`) returning a NULL Widget - the documented no-new-view answer - because the typed `connect_create` wrapper's non-nullable `gtk::Widget` return is what forced the crashing return-the-existing-view shape. Do NOT use decide-policy as the sole route: `window.open` does not emit `NewWindowAction`." Add an explicit acceptance criterion that BOTH a `target="_blank"` link AND a `window.open(url)` load in place without aborting, and a criterion that `docs/adr/0010` + the `blank-window-open-navigates-in-place` matrix row + `docs/spikes/blank-and-window-open-links-navigate-in-place/README.md` are updated to the corrected mechanism (they currently all describe "returns the EXISTING view widget", the crashing shape). If instead the human prefers (a) or (c), say so explicitly in the task, because (a) requires consciously accepting and documenting a `window.open` regression on desktop.

<!-- q5 fields: id=q5 kind=stuck -->

**Your answer** (write below this line):

## Q6

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQW8D968sEdmeVn4irW1"      }

<!-- q6 fields: id=q6 kind=stuck -->

**Your answer** (write below this line):

## Q7

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQW8D968sEdmeVn4irW1"      }

<!-- q7 fields: id=q7 kind=stuck -->

**Your answer** (write below this line):

## Q8

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQW8D968sEdmeVn4irW1"      }

<!-- q8 fields: id=q8 kind=stuck -->

**Your answer** (write below this line):

## Q9

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQW8D968sEdmeVn4irW1"      }

<!-- q9 fields: id=q9 kind=stuck -->

**Your answer** (write below this line):

## Q10

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQW8D968sEdmeVn4irW1"      }

<!-- q10 fields: id=q10 kind=stuck -->

**Your answer** (write below this line):

## Q11

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQWKxQednhops3jviXb6"      }

<!-- q11 fields: id=q11 kind=stuck -->

**Your answer** (write below this line):

## Q12

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQWKxQednhops3jviXb6"      }

<!-- q12 fields: id=q12 kind=stuck -->

**Your answer** (write below this line):

## Q13

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQWKxQednhops3jviXb6"      }

<!-- q13 fields: id=q13 kind=stuck -->

**Your answer** (write below this line):

## Q14

**'task:fix-desktop-create-signal-crash-on-blank-links' was bounced — how should we proceed?**

> transient infra: {"type":"error","error":{"details":null,"type":"overloaded_error","message":"Overloaded"},"request_id":"req_011CdQWKxQednhops3jviXb6"      }

<!-- q14 fields: id=q14 kind=stuck -->

**Your answer** (write below this line):
