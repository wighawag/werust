---
title: "Gate-3 conductor review: macos-appkit-window-and-chrome (APPROVE, after the same Gate-2 block as its sibling)"
date: 2026-07-30
status: open
reviewOf: macos-appkit-window-and-chrome
verdict: approve
---

## Verdict: APPROVE (second attempt)

Merged as `8d14c38`. Blocked first for the SAME reason as the engine task: the `macos-14` leg was extended but never DISPATCHED, so ~1300 lines of AppKit had no runtime evidence. Recovered by dispatching the leg against the work branch, handing the result back, and re-dispatching. Local gate green; the leg is now GREEN on `main` too (run 30573986710).

## What the run actually proved

The leg went green against the branch ([run 30572253620](https://github.com/wighawag/werust/actions/runs/30572253620), macOS 14.8.7, AppleWebKit/605.1.15), all steps including "Build and drive the REAL AppKit window". `window_smoke` opened a real `NSWindow` off-screen and asserted, ending in PASS:

- The chrome paints the CORE's values: the trust badge, **its explanation as the tooltip**, the status line, and the absence of an error banner or invalid-entry badge on a fresh window.
- The ⋮ menu is the shared `BrowserMenu`, item for item, in order.
- A hash-verified `ipfs://` page (offline, pinned, through the PRODUCTION verifying route) settles; the URL bar shows the content-addressed URL; the trust indicator paints the core's verdict; **a successful load raises NO banner because progress lived in the URL bar**; and **the page view did NOT move or resize across a whole load**.
- The debug view opens through the core menu's Debug entry, the page's `console.log` reaches the Console tab, clearing the store empties both tabs, and closing the window frees the slot.
- The NEGATIVE CONTROL: bytes that do not hash to their CID FAIL, raise the prominent error banner with the core's protocol-named reason (`block hash mismatch: bytes do not match cid bafkrei…`), **a failure is the one state allowed to displace the page**, and a failed load is never reported verified.

That last group is the part I care about most: the URL-bar-progress rule that landed on desktop this morning, and the two-axis trust honesty, are now ENFORCED on a second platform by an executable assertion rather than by a reviewer's memory.

## Acceptance criteria, ticked

- [x] A native macOS window renders through the WKWebView backend with every surface present (proven by `window_smoke`, not asserted).
- [x] Every surface reads the SHARED derivation. `src/paint.rs` is host-independent and unit-tested on Ubuntu against the real core, so "the macOS window paints the shared derivation" is a checked fact; `src/window.rs` assigns fields to widgets and contains no rule.
- [x] **The debug-view row helpers were extracted into `werust-core`** (`console_row_text`, `network_status_text`, the `trust_*` column rules, `tail_plan`/`TailPlan`, plus an exported `DEBUG_CONSOLE_CSS_CLASSES` family), tests moving with them, and BOTH desktop debug views now paint from the one derivation. That was the second extraction I assigned to this task.
- [x] The ⋮ menu comes from the core's `BrowserMenu`.
- [x] ADR-0009 / ADR-0010 / the URL-bar-progress rule honoured rather than re-decided.
- [x] Manual steps recorded, and the honesty split corrected: the README no longer says "nothing about this window, yet".
- [x] Ubuntu gate green.

## Nit triage (5 non-blocking findings)

**For the human, two ratifications with real consequences.** First, the `macos-renderer` workflow now also triggers on PULL REQUESTS touching `crates/werust-core/**`, which was previously push-only. Most chrome and task work touches core, so every such PR now spends `macos-14` minutes and can be gated by a red macOS leg. That is either exactly what you want (a second platform guarding core changes) or a cost you would rather not pay on every PR; it should be a decision, not a side effect. Second, `DEBUG_CONSOLE_CSS_CLASSES` is deliberately NOT folded into `CHROME_CSS_CLASS_SETS` (sound: chrome families toggle on one widget, console classes colour a row), but the cost is that no aggregate now covers every exported family, so the no-unstyled-class guarantee does not automatically extend to the new family.

**Worth a small follow-up:** the URL-bar progress TOOLTIP composition is duplicated verbatim in two edges (`crates/werust-macos/src/paint.rs` and `crates/werust/src/main.rs`), same logic, same comment. It is exactly the class of thing this whole extraction effort exists to prevent, caught while small.

**Two accuracy nits, both minor and both in the spike README:** it attributes an environment and duration to the branch run in slightly more detail than that run itself records (some values come from the engine run), and it says the CI-d commit differs from the landed tree "only in `work/` bookkeeping" when it also differs in the README and DECISIONS entries that were corrected afterwards. The material half of that claim, that no source line differs, is verified true.

## The pattern worth naming

Both macOS tasks blocked at Gate 2 for the same structural reason, and neither was the agent's fault: **a worker cannot dispatch CI against its own branch, because the branch only reaches the arbiter when the run surfaces.** The conductor can, and now does. With `macos-renderer.yml` on `main`, the loop is: build blind, block on missing evidence, dispatch the leg at the work branch, hand the result back through `requeue -m`, rebuild. It cost one extra round trip per task and produced real evidence both times. Making it not cost that round trip is what driving these in `--propose` mode would buy.
