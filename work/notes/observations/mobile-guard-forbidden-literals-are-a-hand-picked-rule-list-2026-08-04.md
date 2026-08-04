# The mobile guard's forbidden-literal list is driven by a HAND-PICKED set of core rules

2026-08-04, spotted while landing `reload-stop-collapse-and-loading-spinner-core-and-gtk`.

`every_derived_string()` in `crates/werust-core/tests/mobile_chrome_presentation_shape.rs` is exhaustive over the ENUM AXES (`TrustPosture::ALL`, `LoadStep::ALL`, kept complete by compile-time checks) but it names the RULES it drives by hand (`trust_indicator`, `trust_indicator_detail`, `load_progress_hint`, `trust_pin_action_label`, `invalid_entry_badge_text`). A NEW presentation rule's strings therefore join the forbidden list only if someone remembers to add it: the new `ReloadStopControl::label()` / `description()` strings ("⟳", "Reload this page", "Stop loading this page") are not on it today, so a mobile edge could hardcode one and the guard would stay green.

Sequencing-wise that is expected for the expand step (the fields are deliberately unregistered until `register-the-new-chrome-fields-in-the-mobile-presentation-guard` lands), but that fan-in task's acceptance mentions only `FACT_FIELDS` / `DERIVED_FIELDS`, not this list, so the literal half could be missed. Not fixed here (out of scope, and the guard must not be touched by this task).
