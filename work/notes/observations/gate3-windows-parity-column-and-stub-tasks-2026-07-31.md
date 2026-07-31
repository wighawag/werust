---
title: "Gate-3 verdict: windows-parity-column-and-stub-tasks (APPROVE) — five platforms, and the matrix still refuses to flatter"
date: 2026-07-31
status: open
reviewOf: windows-parity-column-and-stub-tasks
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. `platforms = ["desktop", "macos", "windows", "ios", "android"]`, and all 24 capability rows carry an explicit Windows cell.

## Criteria, ticked

1. **`windows` in `platforms`, every row with an explicit cell.** MET, 24 for 24.
2. **Honest cells.** MET, and the distribution is again credible rather than flattering: **20 implemented, 3 stubbed, 1 n-a**. Each stub points at a task that really exists — two of them (`windows-trust-surface-bless-affordance`, `windows-debug-network-capture-main-document-and-scheme-handled`) authored in this same change, one at the existing `windows-release-packaging-leg`.
3. **No cell claims what the spikes list as unmeasured or unwired.** MET, with each unwitnessed cell carrying its limit inline — see the standing question below.
4. **The guard's expected-platform list gains `windows`.** MET, so the column cannot be silently dropped, matching what the macOS column established.

**The drift update I planted was honoured**: the task read the macOS column as its worked example, matched its cell style, added itself to the guard, and did NOT rename the `desktop` key as a side-effect.

## Acted on

- **The macOS sibling task had gone stale.** `macos-trust-surface-bless-affordance` still said the `windows` column "does not exist yet" and told its builder to leave the Win32 window alone FOR THAT REASON — which this diff falsified. I updated it: the reason is now simply that `windows-trust-surface-bless-affordance` owns that window, with the instruction that anything both windows want goes to `crates/desktop-paint` so the sibling consumes it rather than re-deriving it. I am about to drive that task, so a stale premise would have cost a build.
- **A pre-existing contradiction the new cell surfaced**: `crates/werust-windows/src/debugview.rs` says real Chrome DevTools are "one menu entry away", while the matrix says (correctly, deliberately) that the Windows menu carries no devtools entry and F12 is the reach. Planted the correction in `windows-debug-network-capture-main-document-and-scheme-handled`, which already opens that file.

## Raised to the human — two are now OVERDUE rather than open

- **The `desktop` platform key now names one of THREE desktops** and is still unpinned in the CONTEXT.md glossary. The macOS review explicitly asked for the rename-to-`linux` versus pin-`desktop` decision to be made BEFORE a third desktop column landed. It has now landed. The task was right to refuse the rename as a side-effect (it touches the matrix, the guard, ADR-0005's prose and any in-flight parity task), so this needs scheduling as its own small change.
- **Nobody owns the human-on-a-Windows-box sweep**, exactly as nobody owns the macOS one. Seven Windows cells are wired-and-shape-guarded but unwitnessed (SPA URL tracking, back/forward, retrieval backend, scheme-less entry, `_blank`/`window.open`, web pathing fallback, 3xx redirects), and the unmeasured halves (HiDPI, input and focus routing, window management, debug-view colours) point only at manual steps in a spike README. This is now the SECOND platform in the same position, which makes "wired, not witnessed" a standing standard rather than a one-off: worth one human yes, and probably a manual-sweep task per platform.

## Ratified

- **Pointing the `follow-os-color-scheme` stub at the EXISTING `windows-release-packaging-leg`** (which must add the comctl32 manifest anyway) rather than cutting a dedicated manifest task, and adding an acceptance criterion to that task to flip the cell when it lands. Sensible reuse; it does make the packaging task load-bearing for a matrix cell, which is worth knowing before it is driven.
- **One `n-a`**, on the same capability macOS marks `n-a`, for the same structural reason.

## Small residue, not cut as a task

The decisions note claims each of FOUR task bodies names its sibling; only the two NEW Windows tasks do. I fixed the macOS trust-surface one by hand (above) because I am driving it next; the remaining mismatch is one line in one macOS task and is not worth a follow-up of its own.
