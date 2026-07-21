---
title: T1 WPT-subset regression meter (parse ≥90%, core-CSS ≥70%)
slug: t1-wpt-subset-regression-meter
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [t1-core-css-stylo-and-latin-shaping-parley]
covers: [17]
---

## What to build

Wire the T1 WPT-subset bar as the objective regression meter for the native T1 path:
run the HTML-parsing tree-construction subset (`html/syntax/parsing/`) and the core
CSS static-layout areas (`css/CSS2/normal-flow/`, `css/css-box/`, `css/css-color/`,
`css/css-fonts/`, `css/css-text/`), and enforce the thresholds — ≥ 90 % on parse,
≥ 70 % on core CSS. Complex-script / bidi subsets are EXCLUDED at T1 (deferred with
T2 shaping). This is the objective regression guard, NOT the roadmap driver (the page
checklist drives; this measures/guards).

## Acceptance criteria

- [ ] The named WPT subsets run against the native T1 path and produce a pass-rate.
- [ ] The thresholds (≥ 90 % tree-construction, ≥ 70 % core-CSS areas) are enforced — a drop below fails the meter.
- [ ] Complex-script/bidi WPT subsets are excluded from the T1 bar.
- [ ] The meter is runnable in CI and reports a comparable-over-time number.

## Blocked by

- Blocked by `t1-core-css-stylo-and-latin-shaping-parley`.

## Prompt

> Goal: the objective T1 regression meter — the WPT subset bars from
> `docs/conformance-tiers.md` T1 (parse ≥90%, core-CSS ≥70%; complex-script/bidi
> excluded).
>
> This is the SECONDARY meter, not the roadmap: the page checklists
> (`t1-server-web-floor-article-and-blog`, `t1-content-addressed-floor-ipfs-static-site`)
> define "reached"; this catches regressions and gives a comparable number (also
> feeding the vs-wezig comparison). Wire the named subsets against the native path and
> enforce the thresholds in CI.
>
> Done = the T1 WPT subsets run and enforce their thresholds as a CI regression guard.
