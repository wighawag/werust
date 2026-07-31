---
title: "A TOFU store that silently drops a pin is worse than no store: make blessing read-modify-write, and stop core tests reading the developer's real pins"
slug: pin-store-read-modify-write-and-test-isolation
blockedBy: [ipns-tofu-pin-and-warn-on-change]
covers: []
---

## What to build

Three residues of `ipns-tofu-pin-and-warn-on-change`, found at its Gate-2 and cut by the conductor at Gate-3 (2026-07-31). The first two are correctness; the third is a documented history that is not true.

**1. Blessing rewrites the whole file from a stale snapshot, so one window can silently DROP another's pin.** `BrowserShell::new` loads the pin store ONCE into `self.pins`, and `bless_current_name` saves `self.pins` wholesale. Two shells therefore each hold a snapshot taken at their own launch: bless `a.eth` in window A, then bless `b.eth` in window B, and B's write erases A's pin. Two windows is not exotic here — a second `werust` launch activates the same GTK application and opens a second window in-process, and two different VERSIONS are simply two processes.

A silently dropped pin is the precise failure a TOFU store cannot have: the user believes they blessed a name, the pin is gone, and the next resolution to a different CID produces NO warning — the security-relevant direction of the failure. The sibling settings store already does this correctly: `retrieval.rs`'s `apply_settings_request_in` is load -> mutate -> save per action. Make `bless_current_name` (and any other mutation) re-read the store from disk, merge the new pin into it, and write that, so a concurrent writer's pins survive. Keep the existing `with_pins_dir` isolation seam intact, and cover the drop with a test that blesses through two independently-constructed shells sharing one directory and asserts BOTH pins survive.

**2. Core tests read the developer's REAL `pins.json`.** `BrowserShell::new` calls `TrustedNamePins::load()`, which resolves the real settings directory, so every core test that does not use `with_pins_dir` reads whatever the developer has blessed in their own build. A developer who has blessed `ronan.eth` locally would flip the TOFU axis inside fixtures using that same name and could red unrelated chrome assertions, with a failure that reproduces on ONE machine and nowhere else. Nothing WRITES the real store today (the one bless without `with_pins_dir` returns early at the visibility gate), so this is a hermeticity hole rather than a live data-loss bug, but it is the kind that costs a bewildering afternoon.

Default test shells to an EMPTY store (or make the real-directory read explicit opt-in), and add the assertion the isolation test is missing: the real store is untouched. The work contract's shared-write rule is the standard being applied; this is its read-side twin.

**3. The recorded history of the GTK trust surface is wrong, in three places.** The diff turned the GTK trust badge from a plain `Label` with a hover tooltip into a `MenuButton` opening a `Popover` — a NEW desktop trust SURFACE, not one more line in an existing one. But `DECISIONS.md` section 8 and the authored follow-on task `macos-trust-surface-bless-affordance` both assert the opposite (that GTK already had a popover behind the badge and only needed a line plus a button), and `docs/platform-capability-matrix.toml`'s trust-explanation row still describes desktop as tooltip-only.

The CHANGE is right and should stay; what is wrong is the story told about it. Correct all three so they describe what exists: a real popover surface introduced by that task. The macOS follow-on's premise still holds (macOS needs its own affordance), so only the history it cites needs fixing. This matters because the next agent will read that history as the reason it is allowed to skip building a surface.

**Scope:** one read-modify-write fix with its test, one test-isolation default with its assertion, three documentation corrections. No change to the TOFU model, the trust posture, or the banner rules.

## Acceptance criteria

- [ ] Blessing a name re-reads the store and merges, so a pin blessed in one shell survives a bless in another shell sharing the same directory; covered by a test using two independently-constructed shells.
- [ ] Core tests do not read the developer's real `pins.json`; a test shell starts from an empty store unless it explicitly opts in.
- [ ] A test asserts the REAL pin store is untouched by the suite.
- [ ] `DECISIONS.md` section 8, `macos-trust-surface-bless-affordance` and the matrix's trust-explanation desktop cell all describe the GTK trust surface as it now is (a popover introduced by the TOFU task), not as it was claimed to have been.
- [ ] `cargo fmt --check && cargo clippy && cargo build && cargo test` green.

## Prompt

> Goal: three residues of `ipns-tofu-pin-and-warn-on-change`. (1) `BrowserShell::new` loads the pin store once and `bless_current_name` saves it wholesale, so two shells sharing a directory each hold a stale snapshot and the second bless silently ERASES the first — the security-relevant direction of failure for a TOFU store, because the user believes a name is blessed and no warning ever fires. Do what `retrieval.rs`'s `apply_settings_request_in` already does: load -> mutate -> save per action, keeping the `with_pins_dir` seam, and prove it with a test that blesses through two independently-constructed shells and asserts both pins survive. (2) `BrowserShell::new` calls `TrustedNamePins::load()` against the REAL settings dir, so every core test not using `with_pins_dir` reads the developer's own blessed names and can flip a TOFU axis inside a fixture — a failure that reproduces on one machine only. Default test shells to an empty store and add the missing assertion that the real store is untouched. (3) The diff turned the GTK badge from a tooltip Label into a MenuButton + Popover, a NEW trust surface, but `DECISIONS.md` section 8, the `macos-trust-surface-bless-affordance` task and the matrix's trust-explanation desktop cell all still say GTK already had one. Keep the change, fix all three claims — the next agent will otherwise read that false history as permission to skip building a surface.
