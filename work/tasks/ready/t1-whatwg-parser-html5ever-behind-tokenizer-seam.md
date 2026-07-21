---
title: T1 — real WHATWG parser (html5ever) behind the Tokenizer|TreeBuilder seam
slug: t1-whatwg-parser-html5ever-behind-tokenizer-seam
spec: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
blockedBy: [native-renderer-t0-subset-path-behind-seam]
covers: [13]
---

## What to build

Replace the T0 subset tokenizer with a real WHATWG-algorithm HTML parser
(html5ever) behind the `Tokenizer | TreeBuilder` seam, so REAL documents parse
correctly into a DOM tree (not just the v0 allowlist). This is the parse half of T1;
the core CSS + shaping half is a sibling task. The swap happens behind the seam
established at T0, so the rest of the native path is undisturbed.

## Acceptance criteria

- [ ] html5ever is bound behind the `Tokenizer | TreeBuilder` seam and produces a DOM tree for real documents.
- [ ] The native render path consumes the html5ever-produced tree in place of the subset tree builder, behind the seam.
- [ ] The T1 HTML-parsing WPT tree-construction subset (`html/syntax/parsing/`) can be run against it (the threshold is wired in `t1-wpt-subset-regression-meter`).
- [ ] Tests cover the parser-seam swap and a sample real-document parse.

## Blocked by

- Blocked by `native-renderer-t0-subset-path-behind-seam`.

## Prompt

> Goal: the real parser — bind html5ever behind the `Tokenizer | TreeBuilder` seam so
> real documents parse (T1; see `docs/conformance-tiers.md`, `CONTEXT.md`). This is
> where the pure-Rust stack starts to pay off vs the Zig arm — a load-bearing part of
> the experiment.
>
> Swap the T0 subset tokenizer for html5ever behind the seam so the rest of the
> native path is undisturbed. Pair with `t1-core-css-stylo-and-latin-shaping-parley`
> (cascade + shaping) to produce real static layout. The WPT parse bar is wired
> separately (`t1-wpt-subset-regression-meter`).
>
> Done = real documents parse into a DOM via html5ever behind the seam, feeding the
> native render path.
