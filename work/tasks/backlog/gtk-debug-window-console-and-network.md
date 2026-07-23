---
title: "GTK-side debug window (shift+F12): a console + network view of werust's own activity (ipfs/CAR/IPNS requests, logs, status/timing)"
slug: gtk-debug-window-console-and-network
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
blockedBy: []
covers: [2]
---

## What to build

A first-class diagnostic surface: a GTK-side debug window, toggled by shift+F12, showing werust's OWN activity — NOT the web-side WebKit inspector (that inspects the page's JS/DOM), but werust's engine/network side. HUMAN REQUEST (v0.2.2): "I want to at least see the console and the network." This is what would have made the timeout / whole-DAG / partial-load issues immediately visible.

At minimum two panes:
- **Console** — werust's own log stream: the events/messages/errors the engine emits (load lifecycle transitions, resolution steps — ENS resolve, IPNS record fetch+verify, contenthash decode — trust-posture changes, errors/warnings with their protocol-named reasons). A running log the user can read while browsing.
- **Network** — werust's OUTGOING requests on its side: each `ipfs://` resource / CAR fetch (the gateway URL, `dag-scope`, cid+path), each IPNS record fetch, each ENS `eth_call`, with status (ok/failed/timeout), timing (duration), and size (bytes). So a slow/partial/timeout load is legible: which request was slow, which failed, which refetched the whole DAG.

Wire it as a real GTK window/pane in the desktop shell (`crates/werust`), fed by an instrumentation hook the engine/fetcher/resolver emit events into (a lightweight event/log channel the debug window subscribes to — do not entangle it with the render path or the trust logic). shift+F12 toggles it. Keep it desktop-first (GTK); note whether/how a mobile equivalent would work (likely a follow-on), and register the capability in the parity matrix accordingly (desktop implemented, mobile tracked/n-a as decided).

## Acceptance criteria

- [ ] shift+F12 toggles a GTK-side debug window in the desktop shell.
- [ ] A Console pane shows werust's own log/event stream (load lifecycle, ENS/IPNS/contenthash resolution steps, trust-posture changes, errors with protocol-named reasons), updating live while browsing.
- [ ] A Network pane lists werust's outgoing requests (ipfs/CAR fetch with gateway + dag-scope + cid/path, IPNS record fetch, ENS eth_call) with status, timing, and bytes — enough to diagnose a slow/partial/timeout load.
- [ ] The instrumentation is a decoupled event/log hook (does not alter the render path, verification, or trust logic; a no-op cost when the window is closed).
- [ ] The capability is registered in the platform-capability matrix (desktop implemented; mobile tracked or n-a as decided) so parity stays honest.
- [ ] Tests cover the instrumentation/event feed (events are emitted for a resolve+fetch+load and carried to the debug model), network-isolated; the visual window is documented.

## Blocked by

- None — can start immediately.

## Prompt

> Goal: add a GTK-side debug window (shift+F12) showing werust's OWN console + network activity (not the web-side inspector). The human wants to see, while browsing, werust's log stream (load lifecycle, ENS/IPNS/contenthash resolution, trust changes, errors) and its outgoing requests (ipfs/CAR fetch with gateway+dag-scope+cid/path, IPNS record fetch, ENS eth_call) with status/timing/bytes. This is the tool that makes issues like the whole-DAG-per-request / timeout / partial-load visible.
>
> Where to look: the desktop shell `crates/werust/src/main.rs` (GTK window/keybindings — add shift+F12 + the debug window/pane), the engine/resolver/fetcher in `werust-core` (`resolve_ipfs_request`, `ens`, `ipns`) and `fetcher` (`retriever`, `HttpFetcher`) for where to emit events. Add a lightweight, decoupled event/log channel the debug window subscribes to — keep it OUT of the render/verify/trust path (a no-op when closed). Register the capability in `docs/platform-capability-matrix.toml`.
>
> Done = shift+F12 opens a GTK debug window with a live console + a network view of werust's requests (status/timing/bytes), the instrumentation is decoupled and does not touch verification/trust, the capability is in the parity matrix, and the event feed is tested. FIRST re-check the desktop shell + the resolve/fetch call sites. RECORD the instrumentation design durably.
