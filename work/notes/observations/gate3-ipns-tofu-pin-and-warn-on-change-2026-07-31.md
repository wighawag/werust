---
title: "Gate-3 verdict: ipns-tofu-pin-and-warn-on-change (APPROVE) — the mutable-name warning becomes actionable, with two real residues"
date: 2026-07-31
status: open
reviewOf: ipns-tofu-pin-and-warn-on-change
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. 2,402 insertions: a new `crates/werust-core/src/pins.rs` (737 lines), the trust surface wired on GTK and both mobile edges, a matrix row, eight recorded decisions, and a follow-on task authored for macOS.

This is the task that turns `MutableName` from "this could change" into "this changed since you trusted it", which is the only version of that warning a user can act on.

## Criteria and the human's recorded answers

The task carried three human answers rather than open questions, and all three were built to:

1. **The bless is an explicit action from the TRUST INDICATOR, not a first-visit prompt**, and a changed pin is FAILURE-class: the louder posture plus the high-contrast banner, never silent, never a hard block. MET, including the sibling constraint (in-flight progress may not displace the page; a failure may).
2. **A `pins.json` beside `retrieval.json`, reusing the settings mechanism verbatim**, with the same `WERUST_SETTINGS_DIR` lever and directory-taking cores so tests isolate into a temp dir. MET as to placement and the isolation seam; see residue 1 and 2, which are about how the store is USED, not where it lives.
3. **Both IPNS and ENS are blessable; a blessed-then-changed name is strictly LOUDER than plain `MutableName`, and an unblessed name behaves exactly as today.** MET.

## Two residues that are correctness, not polish

Cut together as `pin-store-read-modify-write-and-test-isolation`:

- **Blessing rewrites the whole file from a snapshot taken at shell construction, so one window silently DROPS another's pin.** `BrowserShell::new` loads once into `self.pins`; `bless_current_name` saves it wholesale. Bless in window A, then in window B, and A's pin is erased. Two windows is not exotic (a second launch activates the same GTK app in-process; two versions are two processes). This is the security-relevant direction of failure: the user believes a name is blessed, the pin is gone, and the next resolution to a DIFFERENT CID produces no warning at all — the store failing silently open, which is the one thing a TOFU store must not do. The sibling `retrieval.rs` already does load-modify-write per action, so the correct pattern is in the repo and was not followed here.
- **Core tests read the developer's REAL `pins.json`.** `BrowserShell::new` resolves the real settings dir, so any core test not using `with_pins_dir` inherits whatever the developer has blessed locally. A developer who blessed `ronan.eth` in their own build flips the TOFU axis inside fixtures using that name, and reds unrelated chrome assertions on ONE machine only. Nothing writes the real store today, so it is a hermeticity hole rather than live data loss, but it is the flavour of bug that costs a bewildering afternoon.

Also folded in: **the recorded history of the GTK trust surface is false in three places.** The diff turned the badge from a tooltip `Label` into a `MenuButton` + `Popover` — a new desktop trust SURFACE — while `DECISIONS.md` §8, the authored `macos-trust-surface-bless-affordance` task, and the matrix's trust-explanation desktop cell all still say GTK already had a popover and only needed a line and a button. The change is right; the story is wrong, and the next agent will read that story as permission to skip building a surface.

## Raised to the human rather than ratified

- **`error_banner_visible` / `error_banner_text` now mean a failure-CLASS state** (a failed load OR a changed trusted name), so the `error_banner_*` name is now wider than what it covers, and the `prominent-load-failure` capability row has a second occupant. The alternative — a second banner surface on four edges — was rejected for a good reason, and the human's own answer settled that a changed pin gets failure-class prominence. It reads correct to me. But it is a vocabulary widening that every edge now inherits, and this repo takes its shared vocabulary seriously enough that I would rather it be ratified than assumed.
- **The changed-name banner is not gated on `is_loading`, while the badge and the pin action are.** So reloading a changed site shows the LOADING badge (which by its own doc asserts nothing about trust) above a failure-class banner asserting the change. The two failure-class cases also differ in flight, because `navigate` clears `last_error` while the mutable-name axis is re-derived. My reading is that this is deliberate and right — a warning that flickers off during every reload is a warning that can be missed by reloading — but it is a visible in-flight combination nobody has decided out loud, so it goes to the human with that suggested default.

## The rest of the nits

The eight recorded decisions are otherwise sound and I ratify them. The task also authored `macos-trust-surface-bless-affordance` for the platform it could not reach, which is the right instinct: the affordance exists on GTK and both mobile edges, and macOS is now the odd one out.
