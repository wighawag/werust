# The contract step: the decisions this task baked in

Task `register-the-new-chrome-fields-in-the-mobile-presentation-guard`, spec `chrome-conventional-controls` (stories 8 + 10). The task is deliberately tiny (three list entries), so these are the two judgement calls a reviewer would otherwise have to reverse-engineer, plus the one thing that was deliberately NOT registered.

## 1. `reloadStopControl`, the mode's WIRE NAME, is deliberately NOT registered

**Chosen:** `DERIVED_FIELDS` gains `reloadStopControlLabel`, `reloadStopControlDescription` and `loadSpinnerVisible` — three of the four chrome-JSON keys the expand step added. The fourth, `reloadStopControl` (the mode's stable lower-case wire name, `"reload"` / `"stop"`), stays out of both lists, and its VALUES stay off the forbidden-literal list.

**Why:** `DERIVED_FIELDS` is not a table of contents for the carrier; it is the list of fields every mobile edge must DECODE and PAINT. Registering the wire name would demand that both edges read a value whose only tempting consumer is the `when`/`switch` this whole collapse deletes, and each edge's own guard already FORBIDS reading it (`no_kotlin_conditional_decides_the_control_mode_or_the_spinner` and its Swift twin assert the painter never touches `chrome.reloadStopControl`). The guard would then require exactly what its siblings forbid. Both mobile edges independently reached the same conclusion and skipped the same key, each recording it beside its decode block, precisely so this fan-in inherited one shape rather than an argument (`docs/spikes/android-chrome-collapse-reload-stop-and-drop-history-buttons/DECISIONS.md` §4).

Its values are not forbidden literals for the mirror-image reason `"loading"` is exempt today: a wire name is CARRIER VOCABULARY, like a JSON key, not a presentation string. Forbidding `"reload"` / `"stop"` as substrings would also trip on legitimate key literals such as `"reloadStopControlLabel"`.

**Alternatives considered:** (a) register it in `DERIVED_FIELDS` and make both edges decode it into an unread property — rejected: a field decoded only to satisfy a guard is dead wiring, and it puts the branch-tempting value in reach on both edges; (b) register it in `FACT_FIELDS` so the literal scan knows the key — rejected as a re-meaning of that list, which is documented as "the `ChromeState` fields, in the wire vocabulary" and the wire name is a DERIVED value, not a fact; (c) add a third list for "carried but deliberately unconsumed" — rejected as restructuring the guard, which this task is explicitly forbidden to do, for one member.

**Touches:** the desktop/GTK painters (which read the mode as a value, not over JSON) are unaffected; the two per-edge mobile guards keep owning the "never branch on the wire name" half. A future edge that genuinely needs the mode as a VALUE would revisit this, and the place to argue it is here.

## 2. The two per-edge sequencing assertions are INVERTED, not deleted

**Chosen:** `the_mobile_presentation_guard_field_lists_are_not_registered_here` in both `crates/werust-android/rust/tests/…` and `crates/werust-ios/rust/tests/collapsed_control_and_dropped_history_buttons_shape.rs` becomes `the_mobile_presentation_guard_registers_the_fields_this_edge_consumes`, asserting the same `CONSUMED_DERIVED_FIELDS` are PRESENT as exact `DERIVED_FIELDS` literals in the central guard.

**Why it had to change at all:** those assertions were the MIGRATE-step hold — they exist to red the gate if a registration crept in before both edges consumed the fields. This task IS the registration, so they fire by design; a green gate is impossible without touching them. That is a mechanical consequence of the sequence, not a scope expansion, and it was observed red before being changed.

**Why inverted rather than deleted:** the coupling they express is worth keeping, just in the other direction. Post-registration the cheap wrong move is the OPPOSITE one: an edge breaks, someone deletes the `DERIVED_FIELDS` entry to make the central guard green, and the field silently loses its cross-edge protection with nothing red. The inverted assertion catches exactly that, from the edge that paints the field. It is not duplicate coverage: the central guard asserts "every registered field is consumed by both edges", the inverted one asserts "every field this edge consumes is registered".

**Sub-decision:** the check demands an EXACT literal match (`literal == field`), not `contains`. These field names now also appear inside the central guard's own comments, and a comment is not a registration; the pre-inversion check used `contains` because it was asserting absence, where the loose direction was the safe one.

**Alternatives considered:** (a) delete the tests and their `CONSUMED_DERIVED_FIELDS` lists — rejected: it spends a live coupling for nothing and leaves the list unused; (b) leave them and relax the central guard so registration does not trip them — rejected outright, that is the "do not weaken the guard" line this task exists on the right side of.

**Touches:** two files owned by already-landed sibling tasks, and their spike docs, which name the old test. Those references were updated in place with a pointer to this task rather than rewritten.

## 3. The forbidden-literal half is driven from `ReloadStopControl::ALL`, not hand-listed

**Chosen:** `every_derived_string()` gains a loop over `ReloadStopControl::ALL` pushing `label()` and `description()`, rather than four hardcoded strings.

**Why:** it is the shape the surrounding code already uses for `TrustPosture::ALL` and `LoadStep::ALL`, and `ReloadStopControl::ALL` carries the same compile-time exhaustiveness check, so a THIRD mode (or a reworded description, or a re-drawn glyph) joins the forbidden list with no one remembering to. Hardcoding `"⟳"`, `"✕"`, `"Reload this page"`, `"Stop loading this page"` in a test would also be a fifth copy of strings this repo has already watched drift.

**Not fixed here:** the list of RULES `every_derived_string()` drives is still hand-picked, so the next new presentation rule is unguarded until someone extends it. Out of scope and already captured in `work/notes/observations/mobile-guard-forbidden-literals-are-a-hand-picked-rule-list-2026-08-04.md`.
