---
title: "Idea (promoted to spec): in-app debug menu (console + network) — see work/specs/proposed/in-app-debug-menu-console-and-network.md"
date: 2026-07-26
status: promoted
kind: idea
promotedTo: in-app-debug-menu-console-and-network
spec: in-app-debug-menu-console-and-network
---

## Promoted to a spec

The v0.2.6 human request for a first-party in-app debug menu (a general browser ⋮ menu showing the version + a Debug entry that opens a tabbed Console + Network debug view, on every platform, overriding the earlier mobile-remote-inspection-only stance) was a coherent SUBSYSTEM with a shared architecture and cross-task dependencies, not independent field-fixes. It has therefore been written up as a full spec:

- **`work/specs/proposed/in-app-debug-menu-console-and-network.md`** — the Problem Statement (no console capture exists yet; network requests are only partly visible to werust today, iOS most constrained), the Solution (a bounded capture store in werust-core over the chrome/FFI surface + per-platform capture points + a general menu + per-platform tabbed debug views, native inspector stays as the deep devtools), the User Stories, Out of Scope (no REPL, the capture toggle is Phase 2, iOS coverage best-effort), the open decisions, and the derived task breakdown.

The five derived tasks are staged in `work/tasks/backlog/` with `spec: in-app-debug-menu-console-and-network`:
`debug-capture-store-console-and-network-in-core`, `general-browser-menu-with-version-and-debug-entry`, `debug-console-network-capture-per-platform`, `debug-view-console-network-tabs-desktop`, `debug-view-console-network-tabs-mobile` (+ a Phase-2 `debug-network-capture-toggle-config` named in the spec).

Original design rationale + the code reality-check now live in the spec; this note is the promotion pointer.
