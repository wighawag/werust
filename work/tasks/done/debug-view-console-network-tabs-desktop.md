---
title: "Desktop debug view: a tabbed screen (Console + Network) opened from the menu's Debug entry, rendering the core capture store"
slug: debug-view-console-network-tabs-desktop
spec: in-app-debug-menu-console-and-network
blockedBy: [debug-capture-store-console-and-network-in-core, general-browser-menu-with-version-and-debug-entry]
covers: [1, 3]
---

## What to build

The DESKTOP debug view for werust's in-app debug menu (design: `work/notes/observations/idea-in-app-debug-menu-console-and-network-2026-07-26.md`). The general menu's Debug entry (`general-browser-menu-with-version-and-debug-entry`) opens a tabbed debug screen showing the CONSOLE log and NETWORK requests captured into the core store (`debug-capture-store-console-and-network-in-core`). This is the standalone, no-tether debug surface (the native WebKit inspector on F12 stays as the deep devtools).

READ-FIRST / drift check: confirm the store exposes console + network entries over the chrome/FFI surface (blockedBy store task) and the menu has a Debug entry with an open-debug-view hook (blockedBy menu task).

Build (desktop, GTK, `crates/werust/src/main.rs` + as needed the seam):
- A debug view surface: a panel (a `Notebook`/tab view in a pane, or a separate window/dialog - decide + record; a togglable bottom/side panel is browser-idiomatic) with at least two TABS:
  - **Console**: a scrollable list of the captured `ConsoleEntry`s (level-coloured: error red, warn amber, etc.), showing level + message + source:line. Newest at the bottom (or top - record), auto-scroll optional.
  - **Network**: a scrollable table/list of the captured `NetworkEntry`s: method, url, status, mime/type, size, and werust's honest per-request trust posture (an ipfs:// row shows content-verified, an https:// row unverified-origin - render the SAME posture vocabulary the trust indicator uses, per ADR-0006; do NOT invent a new trust label).
- A CLEAR affordance (calls the store's `clear()`), and the view UPDATES as new entries are captured (poll the store on the existing pump/refresh cadence, or a signal - reuse the existing refresh loop; do not add a busy loop).
- Opened from the menu's Debug entry (fill the open-debug-view hook the menu task left), toggled closed again.
- Keep it READ-ONLY (render the store); capturing is the capture task's job. A typeable console REPL is NOT in scope here (the native inspector has that); this is the console LOG + network LIST.

Coherence: render the shared store the edges already read; use the established trust-posture vocabulary; do not re-mean trust; reuse the refresh cadence (no busy loop). The Network tab honestly labels each request's trust.

## Acceptance criteria

- [ ] The desktop menu's Debug entry opens a debug view with a Console tab and a Network tab, rendering the core capture store.
- [ ] Console tab shows captured console entries (level + message + source:line, level-distinguished); Network tab shows captured requests (method, url, status, mime, size) each with werust's honest per-request trust posture using the SAME vocabulary as the trust indicator (ADR-0006, not a new label).
- [ ] The view updates as new entries are captured (reusing the existing refresh cadence, no busy loop) and has a Clear action (calls the store's clear()).
- [ ] The view is read-only (renders the store); it opens from the menu Debug entry and closes again; the native F12 WebKit inspector is unaffected (both coexist).
- [ ] Desktop-scoped (the mobile debug view is a separate task); tracked per the parity guard. Tests cover the render-from-store mapping where testable + recorded manual steps for the GTK view.

## Blocked by

- `debug-capture-store-console-and-network-in-core` (the store it renders).
- `general-browser-menu-with-version-and-debug-entry` (the menu Debug entry that opens it).

## Prompt

> Goal: the DESKTOP debug view - the menu's Debug entry opens a tabbed screen (Console + Network) rendering the core capture store. Standalone, no-tether; the F12 native WebKit inspector stays as the deep devtools.
>
> Where to look: `crates/werust/src/main.rs` (the shell/toolbar/menu; add a debug panel or window with a GTK Notebook/tabs). Console tab: level-coloured list of ConsoleEntry (level+message+source:line). Network tab: list/table of NetworkEntry (method,url,status,mime,size) each showing werust's HONEST per-request trust posture using the SAME vocabulary as the trust indicator (ADR-0006, no new label). Clear button -> store.clear(). Update on the existing refresh cadence (no busy loop). Open from the menu Debug hook the menu task left; toggle closed. READ-ONLY render (a typeable REPL is the native inspector's job, out of scope).
>
> Done = Console+Network tabs rendering the store, honest per-request trust in Network, Clear + live-update via the refresh loop, opens from the menu, F12 inspector unaffected, desktop-scoped + parity-tracked, tested where testable + manual steps. FIRST re-check the store surface + the menu Debug hook exist.

## Requeue 2026-07-28

CONDUCTOR FIX-UP (Gate-2 is RIGHT — one defect, decided fix). Branch green + preserved; CONTINUE from its tip. Everything else about the desktop view was accepted; do NOT redesign the Notebook/tabs/columns/Clear. Fix ONLY the refresh-freezes-at-cap defect and finish.

THE DEFECT: DebugView::refresh (crates/werust/src/main.rs) appends only when the store snapshot is LONGER than what it rendered, and resets only when SHORTER. But DebugCapture's ring buffers evict via pop_front at the 300-entry cap, so once a buffer is full its length NEVER changes (it stays 300): neither branch fires, the view silently freezes on entries the store has already discarded, and the Console/Network tabs go stale in exactly the long-session case the ring buffer exists for. 300 requests is one busy session.

THE FIX: make the store eviction-observable with a MONOTONIC SEQUENCE on each entry (assign an incrementing u64 on push; it survives pop_front). In the view, remember the LAST sequence rendered. On refresh, find where that last-seen sequence falls in the current snapshot and append only the entries AFTER it; when it is ABSENT (everything the view holds was evicted) do a full rebuild; when the snapshot is shorter than rendered (a clear) rebuild. That drops exactly the evicted rows and is still incremental, not a rebuild-per-tick. Add the sequence to the ConsoleEntry/NetworkEntry internals (or carry it alongside); it does not need to reach the FFI/edges. Correct DECISIONS.md Decision 2, which describes the incremental design but misses the at-capacity eviction case.

TEST it — this is the whole point: pushing past the cap must still render the newest entry and must not leave the view showing rows the store evicted. Network-isolated.

## Requeue 2026-07-28

CONDUCTOR FIX-UP ROUND 2 (Gate-2 caught the second half of the same defect — your sequence detection works, but the append path still never REMOVES the evicted rows). Branch green + preserved; CONTINUE from its tip. The Notebook/tabs/columns/Clear and the sequence + tail_plan logic were accepted; do NOT redesign them. Close this one gap and finish.

THE RESIDUAL: after a rebuild the view mirrors the store (300 rows, anchor = last sequence). Each at-cap push evicts one from the store's front and appends one at the back. tail_plan finds the anchor in the snapshot and returns AppendFrom, which only APPENDS the new tail — and the ONLY row removal in the whole view is clear_list_box, reachable only on a Rebuild. So the view's row count climbs 300 toward ~600, its top rows are entries the store has already discarded, and it stays stale for ~300 more pushes until the anchor itself is evicted and a Rebuild finally fires. That is still the stale-view defect, just deferred.

THE FIX: on the AppendFrom path, DROP from the top of the ListBox the rows the store has evicted. Track the FIRST-rendered sequence (or the rendered row count) as well as the last; on AppendFrom the rows whose sequence is BELOW the current snapshot head are no longer in the store, so remove exactly those from the top of the list (the drop count is snapshot_len minus the rows the view still legitimately holds), then append the new tail. That keeps the view's row count at the cap and its contents mirroring the store continuously, still incremental.

CORRECT the two claims the review falsified: DECISIONS.md Decision 2's correction paragraph currently says evicted rows are dropped from the view's top implicitly — they are NOT on the append path; and README manual step 10 claims row counts stay at 300 — they do not. Make both say what is true after this fix.

EXTEND the past-cap display test: it must assert the row count STAYS AT MAX after incremental at-cap appends (not merely after a rebuild), and that the view's top rows are not entries the store evicted. Network-isolated.
