---
title: "Gate-3 verdict: windows-backend-error-mapping-and-leg-header-accuracy (APPROVE) — the leg stops lying about itself, and the error stops misdiagnosing itself"
date: 2026-07-31
status: open
reviewOf: windows-backend-error-mapping-and-leg-header-accuracy
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. This is the task I cut at Gate-3 of the Windows engine to carry its nits; it closed all of them.

## Criteria, ticked

1. **A WebView2 failure AFTER the runtime-presence check no longer claims the runtime is missing.** MET. It surfaces as a plain `RendererError::Backend` carrying the platform's own detail, and `missing_runtime_error` is now used only where the runtime really is absent. This was the one with product value: a corrupt user-data folder, a policy block or a version refusal previously told the user to install a runtime they demonstrably had, with the real `HRESULT` buried in a parenthetical.
2. **A pure unit test pins both messages.** MET, on the Ubuntu gate, where a Windows runner can never exercise the missing-runtime path (its image always has the runtime).
3. **The leg header describes the filter it actually has.** MET. `crates/windows-renderer/**` and `crates/werust-windows/**` are explained rather than merely present, and the reason `werust-core`/`fetcher`/`renderer` stay push-only survives intact.
4. **The spike-docs push-filter entry decided deliberately.** MET, kept, with the reason recorded (re-recording a verdict is exactly the change that should re-run the leg).
5. **`DECISIONS.md` records the engine's `os_color_scheme()` + HKCU read.** MET.
6. **`desktop-paint` on both legs' PR filters decided deliberately.** MET: KEPT, with an honest justification in the YAML itself ("it is not Windows-shaped, but it is the one carrier both native desktop windows paint from, so a break in it is genuinely cross-platform").
7. **A Windows section in the repo README; the spike's claimed command matches the harness.** MET, and the harness was strengthened rather than the claim softened.

## Review-nit triage (4 raised, all non-blocking)

**Routed into `windows-release-packaging-leg`** rather than a new task, because that task is now GUARANTEED to open these exact files (see the ratify below), so the fix costs it nothing:

- **Three places say the new error LEADS with the platform detail; it TRAILS it after a colon.** Small, and pointedly ironic: this task existed to close the doc-overclaims-the-tool class and shipped a three-word instance of it. Behaviour is correct; only the rustdoc, the `DECISIONS.md` item and the test's assertion message disagree with it.
- **Two guard COMMENTS overclaim what their tests enforce.** The macOS pin is `contains(...)` plus a two-entry deny list, so a NEW macOS PR-filter path still lands silently despite a comment saying widening must edit a test; and the Windows header claims list-and-header "neither can move without the other going red" while no test holds the header prose. A comment promising a guard that does not exist is worse than no comment, because it is believed.

**Ratified:**

- **The Windows guard became an EXACT-set pin of the whole `pull_request` filter**, broader than the criterion asked (which was only to pin `desktop-paint`). Kept deliberately: it makes every future widening an explicit edit, which is precisely the accretion this leg has already suffered twice in three tasks. The cross-task cost is real and now signposted — `windows-release-packaging-leg` will go red until it updates `PULL_REQUEST_FILTER`, and I planted a forward-note telling it so and telling it not to loosen the pin to make its life easier.
- **`typecheck-windows-from-linux.sh` was strengthened from `cargo xwin check` to `cargo xwin clippy`.** The task allowed either strengthening the harness or softening the README claim; the agent did both, in the honest direction. No CI leg runs the script, so the gate is unaffected and a developer's local loop gets stricter. Keep.
