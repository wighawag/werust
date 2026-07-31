---
title: "Gate-3 verdict: macos-parity-column-and-stub-tasks (APPROVE) — the column is honest, and it strengthened the guard rather than leaning on it"
date: 2026-07-31
status: open
reviewOf: macos-parity-column-and-stub-tasks
verdict: APPROVE
---

## Verdict: APPROVE ✅

Merged to `main`. 21 capability rows gained an explicit `macos` cell; two follow-on tasks were authored to back the two `stubbed` cells; the guard itself was tightened.

This is the task type most likely to be quietly dishonest, because filling a column with `implemented` is free and nobody checks. So the review was aimed there.

## Criteria, ticked against the merged diff

1. **`platforms` includes `macos`, every row carries an explicit macOS cell.** MET. 21 rows, 21 cells.
2. **Each cell honest: `implemented` only where it really works, `stubbed` with a real slug, `n-a` only where genuinely inapplicable.** MET, and the distribution is credible rather than flattering: **18 implemented, 2 stubbed, 1 n-a**. A column of 21 `implemented` would have been the smell; this is not that.
   - The single `n-a` (`system-back-navigates-history`) is the strongest cell in the diff. It does not say "not applicable" and stop. It says macOS has no OS-level Back routed to the app, names the on-screen control as where that capability lives instead, names the platform ANALOGUE (the WKWebView two-finger swipe gesture), and points at a fresh observation recording that the analogue is NOT enabled. That is the opposite of using `n-a` to hide a gap.
   - The two `stubbed` cells point at `macos-web-inspector-safari-devtools` and `macos-debug-network-capture-main-document-and-scheme-handled`, both authored in the same change and both present in `tasks/backlog/`. The guard resolves them, so they are real, not decorative.
3. **Every `stubbed` cell's task exists.** MET, enforced by the guard itself.
4. **The parity test passes in the normal `verify` gate with NO weakening of the guard.** MET, and EXCEEDED: the guard was STRENGTHENED. `macos` is now hardcoded into the expected-platform list, so a later change cannot quietly drop the column and take every gap it tracks with it. That is an unasked-for change to a shared guard, and it is the right one: silently dropping a platform is precisely the omission failure mode ADR-0005 exists for, and it was previously possible. Ratified.

**The forward-note I planted was honoured exactly.** The task added ONLY the `macos` column and left `windows` to its sibling, including the "make the cells easy to sit beside a Windows column" part.

## The one judgement call worth the human's attention

`implemented` here means **wired** (ADR-0005's own wording), not **witnessed on a Mac**. Six cells — follow-os-color-scheme, spa-url-tracking, scheme-less-entry-routing, blank/window.open-navigates-in-place, ipfs-redirects-3xx, retrieval-backend — rest on compilation + shape guards + shared-core tests, with the runtime half unwitnessed. Each says so inline and points at a specific manual step in the macOS spike README, which is the honest way to do it, and the remaining cells DO map to real assertions in the window smoke that ran on a real macos-14 runner.

I am approving it because it matches the guard's documented meaning and because every limit is stated rather than implied. But it is worth one human yes, for a specific reason: **this repo has bounced macOS work twice for shipping a prediction where a measurement was owed**, and "wired but unwitnessed" is the same family of claim wearing better clothes. Related, and raised with it: nothing on the work board OWNS the human-on-a-Mac sweep those six cells defer to. They point at README prose, and `stubbed` is reserved for wiring gaps, so an unwitnessed-but-wired cell has no board slot at all. Both go to the human batch.

## Review-nit triage (5 raised, all non-blocking)

- **Guard strengthened without being asked.** RATIFIED (above).
- **The meaning of `implemented`, and the unowned Mac sweep.** To the human batch (above). Not blocking: the limits are stated, not hidden.
- **The `desktop` platform key now means Linux/GTK only.** To the human batch. The agent found this itself, documented it in the file header, the guard comment AND an observation note, and correctly did NOT rename anything: a `desktop` -> `linux` rename touches the matrix, the guard, ADR-0005's prose and whichever parity task lands second. That restraint is right. With a THIRD desktop column (Windows) queued, the decision is now cheap to make and gets more expensive with each column.
- **CONTEXT.md has no glossary entry pinning the platform keys.** Folded into the same human question; a glossary entry is the natural home for whatever is decided.
- **The sibling Windows task still asserted the old `platforms` line.** ACTED ON: I planted a drift update in `windows-parity-column-and-stub-tasks` giving it the current platforms line, pointing it at the macOS column as the worked example of the honesty standard, telling it to add `windows` to the guard's expected list for the same reason macOS is there, and warning it off the `desktop` rename. One line of drift caught now instead of a wasted build later.

## Off-path findings the build filed correctly (not fixed in scope)

- `desktop-platform-key-now-means-linux-only-2026-07-31.md` — the naming decision, with its four owners named.
- `macos-swipe-back-gesture-not-enabled-2026-07-31.md` — `allowsBackForwardNavigationGestures` is never set on the macOS `WKWebView`, so it defaults to false and the two-finger swipe does nothing. Same shape as the recorded iOS finding, on the same WebKit property. This is a real, small, user-visible gap on two platforms with a shared cause; it deserves a task, and it is in the human batch as a candidate rather than authored unilaterally.
