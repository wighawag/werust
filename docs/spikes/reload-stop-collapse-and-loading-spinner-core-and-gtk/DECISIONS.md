# The collapsed reload/stop control and the loading spinner: the decisions this task baked in

Task `reload-stop-collapse-and-loading-spinner-core-and-gtk`, spec `chrome-conventional-controls`. These are the non-obvious, in-scope judgement calls behind `werust_core::reload_stop_control` / `load_spinner_visible`, the two carriers (`crates/desktop-paint`, `werust_core::chrome_json`) and the GTK wiring in `crates/werust/src/main.rs`. Four sibling edge tasks (`reload-stop-collapse-and-spinner-on-the-macos-window`, `reload-stop-collapse-and-spinner-on-the-windows-chrome`, `android-chrome-collapse-reload-stop-and-drop-history-buttons`, `ios-chrome-collapse-reload-stop-and-drop-history-buttons`) inherit them, so they are recorded rather than buried.

## 1. The CONTROL follows `is_loading`; the SPINNER follows `load_progress_visible`

**Chosen:** `reload_stop_control(state)` returns `Stop` exactly when `ChromeState::is_loading()`, and `load_spinner_visible(state)` is exactly `load_progress_visible(state)` (which is `is_loading() || load_step() != Idle`). So the two are NOT gated on the same predicate, deliberately.

**Why:** the two rules answer different questions. The control offers an ACTION, so it may only appear when that action does something: during the pre-content window (an ENS/IPNS name being resolved before the backend load starts) there is nothing for `Renderer::stop` to cancel, which is precisely why `load_progress_tooltip` already withholds its cancel hint there and why the old Stop button was insensitive then. Promising a cancel that no-ops would be a lie the collapse must not introduce. The spinner only REPORTS, and the case it exists for is exactly that window: the long `ronan.eth` freeze, where `is_loading()` is false, is where a user most needs to see motion. Tying the spinner to `load_progress_visible` also means the two loading SURFACES in the toolbar (the URL bar's fraction and the spinner) are on ONE rule and can never contradict each other.

**Alternatives considered:** (a) both on `is_loading`, which leaves the spinner dark through the longest stall werust has, i.e. it fails the very story it was written for (story 8); (b) both on `load_progress_visible`, which would put the control into Stop mode with nothing to stop, re-introducing the dishonest affordance the tooltip rule was fixed to avoid; (c) a third, spinner-only fact on `ChromeState`, rejected because the spinner is a second PRESENTATION of a fact that already exists, not a new one to track (and a second source of truth is how the mobile twins drifted).

**Touches:** all four sibling edge tasks read both values; nobody re-derives either.

## 2. The mode carries its own painter vocabulary, including the ACTION it performs

**Chosen:** `ReloadStopControl` is an enum (`Reload` / `Stop`) with `wire_name()`, `label()`, `description()` and `action()`, rather than a bare bool ("is the control a stop button?").

**Why:** an edge should ASSIGN values, not branch. The wire name is what crosses to Kotlin/Swift; the label is the glyph the affordance already wore; `description()` is the accessible name (see 3). `action()` is the load-bearing one: it hands back the mode's `shortcuts::ChromeAction`, so the GTK button performs its click through the SAME `perform_chrome_action` the keyboard's Ctrl+R and Escape go through. The toolbar cancel and the keyboard cancel are therefore ONE path and cannot drift; without it, every edge would write its own two-armed `if` mapping the mode back onto a shell method.

**Coherence note (a new concept touching an existing one):** `ChromeAction` was introduced by `shortcut-resolution-in-core-and-the-gtk-edge` as the meaning of an INPUT (a chord or a mouse button). Using it for a toolbar control extends its reach from "what a chord means" to "what a chrome control does", which is what its own doc already says it is ("actions, not implementations… the edge performs it through the existing seam exactly as its toolbar button does"). It is not re-meant, and no new vocabulary was minted beside it. The other toolbar buttons (back/forward/URL entry) are deliberately left calling the shell directly: converting them is a separate, unasked change.

**Alternatives considered:** (a) a `bool is_stop_mode` on the carriers, which pushes the label, the accessible name and the action into each of five edges; (b) exporting three parallel functions (`reload_stop_control_label(state)`, …) in the style of `load_progress_hint`, rejected because they would all re-ask the same one question and the enum keeps them together; (c) exporting a GTK/AppKit icon name from the core, rejected under the ADR-0011 layering that keeps the stylesheet (and the icon theme) in the edge that has one.

## 3. `description()`, not `tooltip()`

**Chosen:** the control's words-form is called its DESCRIPTION ("Reload this page" / "Stop loading this page"), carried as `reloadStopControlDescription`.

**Why:** a desktop painter hangs it on hover, but the mobile edges have no hover at all, and the mobile presentation guard already pins that a hover tooltip is not an acceptable mobile surface (`both_mobile_edges_surface_the_trust_explanation`). Android puts it in `contentDescription`, iOS in `accessibilityLabel`; naming the field `tooltip` would tell those two edges to use a desktop affordance they do not have. `load_progress_tooltip` keeps its name because it really is only a hover surface today (it is on the `desktop-paint` carrier, not the mobile one).

**Touches:** both mobile edge tasks, which should assign this to the platform's accessible-name slot.

## 4. The STOP glyph is reused, and a RELOAD glyph joins it

**Chosen:** `ReloadStopControl::Stop.label()` IS the existing `STOP_AFFORDANCE_LABEL` (`✕`); a new `RELOAD_AFFORDANCE_LABEL` (`⟳`) was added beside it for the other mode.

**Why:** `STOP_AFFORDANCE_LABEL` exists so the ONE progress sentence names the same affordance every edge shows. After the collapse that affordance is the control's Stop MODE, so a second stop glyph would have made the tooltip's "press Stop (✕) to cancel" name something the user is not looking at. Its doc comment was updated to say the label now belongs to a mode rather than a button of its own.

## 5. Where the spinner sits (the layout the sibling edges follow)

**Chosen, on the GTK edge:** back, forward, **[reload/stop control] [spinner]**, URL bar, invalid badge, trust badge, ⋮ menu. The spinner sits immediately AFTER the collapsed control and BEFORE the URL bar, and it is NEVER adjacent to the trust badge.

**Why:** the toolbar's two loading surfaces (the control that stops a load, the spinner that reports one) belong together, and the trust badge is at the opposite end for a reason — a spinner beside it would read as a claim about the load's TRUST, the exact conflation `docs/adr/0012` and the loading-wins trust rule exist to prevent. The URL bar's own progress fraction stays where it is (story 9), so the toolbar reads left-to-right as "act on the load / watch the load / the address, which also shows how far".

**Sub-decision, the spinner's slot is permanent.** Only its `spinning` + opacity follow the derivation; the widget is never hidden. Toggling visibility would re-allocate the toolbar and shove the URL bar sideways on every load start and end — the horizontal twin of the geometry lesson `loading-progress-in-the-url-bar-not-a-banner` learned vertically. Setting opacity explicitly (rather than relying on a theme's `spinner:not(:checked)` rule) keeps it theme-independent.

**Touches:** the four sibling edge tasks, which are asked to record their own spinner placement; this is the shape they should follow unless the platform's own toolbar idiom says otherwise.

## 6. This task does NOT register the new fields in the mobile presentation guard

**Chosen:** `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` (`FACT_FIELDS` / `DERIVED_FIELDS`) is untouched.

**Why:** the guard asserts that BOTH mobile bindings decode and BOTH painters paint every listed derived field. Listing the new fields before the Android and iOS edges consume them would red the gate for three tasks running, i.e. this task could not pass its own gate. This is the EXPAND step; `register-the-new-chrome-fields-in-the-mobile-presentation-guard` (blocked on both mobile edges) is the CONTRACT step and owns the registration. Weakening or special-casing the guard was never an option: it is the mechanism that stopped the Kotlin/Swift chrome twins from returning.

**Consequence, stated plainly:** until that fan-in task lands, these four carrier fields are the only chrome values crossing to mobile without guard coverage. The new guard added here (`collapsed_reload_stop_control_shape.rs`) covers the CORE, both carriers and the GTK edge, not the mobile ones.
