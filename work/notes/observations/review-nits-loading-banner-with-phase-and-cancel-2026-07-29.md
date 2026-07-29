---
title: review-gate non-blocking nits for 'loading-banner-with-phase-and-cancel' (Gate 2 approve)
date: 2026-07-29
status: open
reviewOf: loading-banner-with-phase-and-cancel
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'loading-banner-with-phase-and-cancel' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The 'Resolving name…' and 'Fetching record…' banner labels are effectively unreachable at runtime — should the banner also cover the ENS-resolution phase that triggers the field-test freeze?
  (loading_banner_visible gates on ChromeState::is_loading() (true only for LoadState::Started|Committed). But the core pairs ResolvingName/FetchingRecord with renderer.load_state()=Idle (resolution runs BEFORE renderer.navigate sets Started — lib.rs:1099-1199 sets resolving_step while the backend load has not started; refresh_chrome at lib.rs:1775-1790 derives load_step from resolving_step ONLY when it is Some, while load_state stays Idle). So during the ENS/IPNS resolution phase is_loading() is false and the banner hides. That phase is exactly the ronan.eth freeze the task was cut to signal. The backend content-fetch/render phases (FetchingContent/Rendering, load_state Started/Committed) ARE covered and the banner shows then. The task's acceptance criteria explicitly gate on is_loading() so the code meets them; the gap is between the task's motivational framing (frozen page on long retrievals) and its gate. Not recorded in DECISIONS.md as a limitation.)
- The desktop unit test asserts the ResolvingName/FetchingRecord banner text using a synthetic chrome state (LoadState::Started + LoadStep::ResolvingName) the core never produces — giving false confidence the resolution phase is covered. Use the real Idle+ResolvingName state?
  (crates/werust/src/main.rs test loading_banner_names_the_phase_while_a_load_is_in_flight_and_hides_when_settled constructs ChromeState{load_state: Started, load_step: ResolvingName} and asserts the banner shows 'Resolving name…'. The core never pairs Started with ResolvingName (see lib.rs:1785-1790: resolving_step wins while load_state is Idle; Started maps to FetchingContent). The real resolution state (Idle+ResolvingName) would fail loading_banner_visible (is_loading false). The test passes but does not reflect a reachable chrome state.)
