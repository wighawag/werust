---
title: "Field test v0.2.2: IPNS works but slow/timeouts + per-request whole-DAG refetch causes flaky partial loads; want clearer loading/error UX + a GTK-side debug window (console/network)"
date: 2026-07-23
status: open
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
kind: field-observation
source: human manual test of the v0.2.2 build on desktop + Android
---

## Confirmed WORKING in v0.2.2 (the prior field fixes landed)
- `mandalas.eth` renders well on desktop (color-scheme fix works).
- The neutral loading state shows on navigation (#5 works).
- `ronan.eth` (IPNS) now RESOLVES and RENDERS (the V2-only rust-ipns fix works) — it was NOT "unsupported"; it genuinely resolves now. (The earlier "unsupported" was pre-IPNS-task.)

## New issues found

### A. `transport error: timeout: global` on first load of ronan.eth; worked on retry
IPNS adds a round-trip (fetch+verify the record, THEN fetch the content), so the first load is slower and hits the fetcher's 30s `DEFAULT_GLOBAL_TIMEOUT`. A reload (warm) succeeded. The timeout is too aggressive for the IPNS + whole-DAG path (see B).

### B. Flaky PARTIAL loads: many resources (CSS) not loaded on first try, reload fixes it — ROOT CAUSE FOUND
The trustless-gateway CAR backend fetches with `dag-scope=all` (`GET <gateway>/ipfs/<cid>?format=car&dag-scope=all`) — i.e. EACH request (the directory root AND every sub-resource css/js/image) re-fetches the ENTIRE site DAG as a CAR, re-verifies all blocks, and reassembles, just to serve one resource. So a real site does N whole-DAG fetches (one per resource), each large and slow; individual ones time out -> CSS/assets not applied -> reload (warm/cached) works. This is a performance + correctness bug: use `dag-scope=entity` + `entity-bytes` (Trustless Gateway spec) so each request fetches ONLY the blocks for the requested resource, not the whole DAG every time. This is almost certainly the real cause of BOTH the timeouts (A) and the partial loads.

### C. Loading / error indicator too weak
The human wants clearer feedback: what is loading vs errored, and progress. The current loading state is a neutral badge; a partial/slow load needs a better indicator (progress / per-resource state), and the error surfacing (though now a banner from the prior task) could be clearer for a transient/timeout case (which is retryable, not a hard fail).

### D. Wanted: a GTK-side DEBUG WINDOW on shift+F12 (console + network)
The human wants a debugging surface: shift+F12 opens a GTK-side debug view (NOT the web-side WebKit inspector) showing at least the console (printh/log/errors from werust) and the NETWORK activity (what werust requested: each ipfs:// resource / CAR fetch / IPNS record fetch, status, timing, bytes). This would have made A/B trivially visible. A first-class dev/diagnostic surface.

## Triage
B is the highest-value fix (whole-DAG-per-request) and likely resolves A and the partial loads together; A (timeout) should still be raised/split (record fetch vs content fetch) as a safety margin. C (loading/error UX) and D (debug window) are UX/tooling. None hand-fixed here; each a scoped task.

## UPDATE (2026-07-23, human): use the REAL platform web inspector, not a custom GTK window
Correction to issue D: the shift+F12 that works on desktop today is the GTK INTERACTIVE DEBUGGER (widget tree/CSS), not web content. The human wants the real web devtools (console with a typeable JS REPL + network), like a desktop browser, and ideally the same on mobile. Every platform's WebView already ships a full inspector, so NO custom debug window is needed:
- Desktop WebKitGTK: enable `enable-developer-extras` + show `WebInspector` in-window (real WebKit devtools: console REPL + network + DOM).
- iOS WKWebView: `isInspectable = true` -> Safari Web Inspector over USB.
- Android System WebView: `setWebContentsDebuggingEnabled(true)` -> chrome://inspect (Chrome DevTools) over USB.
Re-scoped `gtk-debug-window-console-and-network` -> `enable-web-inspector-devtools-all-platforms` (enable each platform's native inspector, gated behind a debug build/setting). Much simpler + better than a hand-built window. (A werust-side network/console log for its OWN requests — CAR fetch/dag-scope/IPNS/eth_call — is a SEPARATE, optional nice-to-have, since the web inspector shows the page's network, not werust's internal ipfs:// resolution; captured but not a priority.)
