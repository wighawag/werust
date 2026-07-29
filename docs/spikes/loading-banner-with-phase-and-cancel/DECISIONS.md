# Decisions — loading banner (phase + cancel)

Task: `loading-banner-with-phase-and-cancel`.

## D1 — phase names: the four actual `LoadStep` variants, not the spec's
illustrative "Verifying…"

The task body's `What to build` lists the example phases as
"Resolving name…", "Fetching content…", **"Verifying…"**, "Rendering…", then
immediately states (twice, as a load-bearing coherence rule):

> Phase names come from the existing `LoadStep` variants verbatim. … Phase names
> match the existing `LoadStep` vocabulary verbatim so the debug Network tab and
> the banner cannot disagree.

The existing `LoadStep` enum (`crates/werust-core/src/lib.rs`) has exactly:
`Idle`, `ResolvingName`, `FetchingRecord`, `FetchingContent`, `Rendering`.
There is **no** `Verifying` variant — content verification happens *inside* the
`FetchingContent` step (the hash-verified content-addressed fetch path), not as
a distinct phase the shell exposes.

**Chosen:** use the four real `LoadStep` variants verbatim (`Resolving name…` /
`Fetching record…` / `Fetching content…` / `Rendering…`, plus a generic
`Loading…` when a load is in flight but the step is `Idle`). Do NOT invent a
`Verifying` phase — that would fork a new vocabulary item the debug Network tab
has no fact for, violating the "cannot disagree" rule the task itself states.

**Alternative considered:** add a new `LoadStep::Verifying` variant to
`werust-core`. Rejected: (a) the task explicitly scopes this as a "UI-ONLY
addition at the shell layer … not a core change"; (b) the core has no lifecycle
hook that fires a distinct verify-only step today (verification is folded into
the content fetch), so a `Verifying` variant would never be set and the banner
would never show it — a dead label.

**What it touches:** nothing outside this task. The mobile shells' `loadStepHint`
mappings (`WerustCore.kt` / `WerustCore.swift`) already enumerate exactly these
four wire names; the banner reuses the SAME set, so the banner, the footer
status line, and the debug Network tab all speak the one `LoadStep` vocabulary.

## D2 — banner shares the error-banner slot (mutual exclusion by load state)

The loading banner and the existing prominent error banner occupy the SAME slot
(directly under the toolbar, above the page view) on all three platforms. They
are mutually exclusive in practice: a load is either IN FLIGHT
(`is_loading()` true, no `last_error`) or has SETTLED as `Finished` / `Failed` /
`Idle`. A failed load shows the error banner (not the loading banner); an
in-flight load shows the loading banner (not the error banner); a settled-ok
chrome shows neither. So only one is ever visible at a time, and they never
compete.

**Alternative considered:** a separate dedicated slot for the loading banner.
Rejected: it would reserve permanent vertical space (a layout jump / gap on a
settled chrome) for a transient surface, where the error banner already proved
the under-toolbar slot works for a transient load-state surface.

## D3 — Cancel reuses `core.stop()`, surfaced twice

The banner's Cancel calls the SAME `BrowserShell::stop` the toolbar Stop button
already calls (desktop: `shell.borrow_mut().stop()`; Android: `core.stop()`;
iOS: `onStop` → `core.stop()`). No new mechanic, no new stop path — a second
affordance for the one existing stop action, surfaced where the user is looking
during a long load (the banner). On Android the stop call is inline on the UI
thread (a cheap non-blocking core call, no resolve/network), matching the
existing toolbar Stop button's wiring, so the ANR guard is not regressed.