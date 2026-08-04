# One reload/stop control and a loading spinner: what landed, and how to check it by hand

Task `reload-stop-collapse-and-loading-spinner-core-and-gtk`, spec `chrome-conventional-controls` (stories 8, 9, 10, 14). The judgement calls behind the design are in `DECISIONS.md` beside this file.

## Where things live

- **The derivation** (toolkit-free, display-free): `crates/werust-core/src/lib.rs`, beside `load_progress_visible` / `_fraction` / `_hint`.
  - `reload_stop_control(state) -> ReloadStopControl` — `Stop` while `ChromeState::is_loading()`, `Reload` otherwise. The mode carries `wire_name()`, `label()`, `description()` and `action()` (its `shortcuts::ChromeAction`).
  - `load_spinner_visible(state) -> bool` — exactly `load_progress_visible(state)`, so the spinner and the URL bar's progress fraction report the same load. The URL bar's own rules are unchanged.
- **Carrier 1, the plain-Rust snapshot** the AppKit and Win32 painters read: `crates/desktop-paint/src/lib.rs` — `ChromePaint::reload_stop_control` / `reload_stop_label` / `reload_stop_description` / `spinner_visible`, asserted field-by-field to be the core's own values.
- **Carrier 2, the chrome JSON** the Kotlin and Swift edges decode: `werust_core::chrome_json` — `reloadStopControl` / `reloadStopControlLabel` / `reloadStopControlDescription` / `loadSpinnerVisible`.
- **The painter** (GTK): `crates/werust/src/main.rs`. One `reload_stop` button whose themed icon comes from `reload_stop_icon(mode)` (a resource lookup on the core's decision) and whose tooltip is the core's `description()`; a `Spinner` beside it driven by `load_spinner_visible`. The click performs the mode's own `ChromeAction` through the same `perform_chrome_action` the keyboard uses.
- **The guards**: `crates/werust-core/tests/collapsed_reload_stop_control_shape.rs` (both carriers carry it, the GTK painter derives nothing, back/forward untouched, cancel survives on both routes) plus the display-free unit tests in `werust-core` and `desktop-paint`.
- **The parity row**: `collapsed-reload-stop-and-loading-spinner` in `docs/platform-capability-matrix.toml` (desktop implemented; the four other edges are tracked stubs on their sibling tasks).

## What the toolbar looks like now

`◀  ▶  [⟳ / ✕]  (spinner)  [ URL bar with its progress fraction ]  ⛔?  trust badge  ⋮`

Back and forward stay (desktop keeps them; only the mobile edges drop them, in their own tasks). The reload/stop pair is one button. The spinner's slot is permanently allocated and only its animation + opacity change, so a navigation never shifts the URL bar sideways.

## What a runner measured

`cargo test` covers the derivation and both carriers with no display. The GTK widgets themselves are asserted by `real_collapsed_control_and_spinner_follow_the_derivation_on_a_display` in `crates/werust/src/main.rs`, which is `#[ignore]`d because the gate may have no display. It was RUN for this task under Xvfb and passed:

```sh
xvfb-run -a cargo test -p werust -- --ignored real_collapsed_control_and_spinner
```

It builds the real `Chrome` widgets and asserts that a settled chrome shows the reload icon + "Reload this page" with a still spinner, that an in-flight chrome turns the SAME button into the stop icon + "Stop loading this page" with the spinner running, and that settling again reverses both.

## Manual check (needs a display; no CI runner in this repo watches a spinner)

Run a debug build: `cargo run -p werust`.

1. On a settled page the single control shows the refresh arrow and its tooltip reads "Reload this page"; the spinner is invisible.
2. Start a slow load (a big page, or an `.eth` name that has to resolve first): the SAME button becomes the stop cross with "Stop loading this page", and the spinner turns. The URL bar's progress bar behaves exactly as before.
3. Click that control while the load is in flight: the load is cancelled (status settles, progress clears), and the button turns back into Reload.
4. Repeat step 3 with the keyboard instead: click the page first, then press Escape during a load. Same result, same path.
5. Watch the URL bar's left edge across steps 2 and 3: it does not move when the spinner appears or disappears.
6. During the pre-content window of an `.eth` load (before the backend load starts), the spinner turns but the control still offers Reload: there is no backend load to cancel yet, which is what the URL bar's tooltip already says there.

Honest limits: none of the above is measured by a runner (there is no display leg in this repo), and "the spinner reads as motion" is a claim about pixels no test makes. The four other edges do not have this yet: they are tracked stubs in the parity matrix, and each one is a painter change over a carrier that already holds the values.
