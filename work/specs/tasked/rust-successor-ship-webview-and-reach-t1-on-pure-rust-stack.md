---
title: "werust: ship day-one on webview and reach conformance T1 on the pure-Rust renderer stack"
slug: rust-successor-ship-webview-and-reach-t1-on-pure-rust-stack
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks. (The technical-detail sections below are trimmed by `to-task` once the work is tasked — they move into tasks/ADRs and this spec settles to its durable framing: Problem / Solution / User Stories / Out of Scope.)

## Problem Statement

I want a from-scratch, general-purpose web browser written in **Rust** — the
single-language successor to my existing Zig project **wezig** — that carries
forward all of wezig's language-independent learning but is implemented in Rust
for one-language uniformity (one language, one toolchain, one mental model for a
solo + LLM-driven codebase).

The browser is for a **"post-trusted-server" web**: the origin is not trusted by
default, verifiable / content-addressed content is preferred over
server-authoritative content, privacy is protected, and the experience is
local-first. It MUST render the normal server web with full compatibility (a hard
requirement), and decentralised-web capabilities (native `ipfs://` resolution, a
native Ethereum EIP-1193 provider) are first-class, not extensions — a
*consequence* of the trust stance, not the reason to exist. This thesis is fixed
and already recorded in `docs/adr/0001-general-browser-for-a-post-trusted-server-web.md`
(ported from wezig's ADR-0011).

The deeper reason this project exists is a **reversible experiment**: does
standing on the mature **pure-Rust renderer stack** (html5ever, stylo, taffy,
parley/cosmic-text, vello+wgpu) give a *simpler / faster* path to conformance
tier **T1** than wezig's Zig arm? wezig stays the live **control group**. If Rust
drowns in DOM object-graph friction, that is a valid finding — not a failure to
paper over.

This spec is the **committed slice**: ship usable day-one on a system webview,
grow a from-scratch native Rust renderer behind a seam up to **T1**, and *run*
the capability + trust-hook benchmark that will decide the native-renderer
architecture. The native-renderer architecture DECISION itself is a separate,
gated exploration spec (`rust-successor-native-renderer-architecture-benchmark`,
ordered `taskedAfter` this one).

## Solution

- **Ship from day one via a system-webview backend (WebKitGTK first)** behind a
  wide, hot-swappable `Renderer` seam — the "webview now, native later" hedge
  that keeps werust out of the "no usable product for years" trap. The webview
  backend qualifies as a real backend ONLY because it can satisfy the **trust
  hooks**: EIP-1193 provider injection (via a script-message bridge) and an
  `ipfs://` custom-scheme / request-interception hook — not merely because it
  renders well.
- **Grow a from-scratch native Rust renderer IN PARALLEL behind the same
  `Renderer` seam**, assembled from the mature pure-Rust stack rather than
  binding C for renderer internals. Climb the pinned conformance ladder
  (`docs/conformance-tiers.md`, ported from wezig's ADR-0012): **T0** (fixed v0
  subset) → **T1** (real static documents: WHATWG parse + core CSS + Latin/LTR
  shaping). Each tier lands BOTH a normal server-served page AND a
  content-addressed (`ipfs://`) page; a tier is not "reached" until both land.
- **Bind mature engines at the other seams; never hand-roll the dangerous parts.**
  `ScriptEngine`: bind a mature JS engine (SpiderMonkey leant) — a pure-Rust
  engine is an aspirational later swap-in, NOT day-one. `Fetcher`: bind a vetted
  HTTP+TLS stack (rustls or bound libcurl) — NEVER write TLS — plus a
  hash-verified content-addressed fetch path. Page-GPU on wgpu (WebGPU); WebGL
  via the ANGLE-style GLES-to-native route, not a hand-written GL stack.
- **Target desktop (Linux first) and mobile (iOS/Android)** as wezig does, with
  Swift/Kotlin only at the forced OS edge. Release via GoReleaser's Rust builder,
  parity with wezig including mobile artifacts (`docs/adr/0002`).
- **Run the native-renderer architecture benchmark** (capability + trust-hook) as
  the final story here, producing the evidence the follow-on exploration spec
  consumes to DECIDE the architecture.

## User Stories

A vertical, tracer-bullet ordering — each story is a real slice a user or a
reviewer can observe, climbing the ladder while keeping the product usable
throughout.

### Product shell + the Renderer seam (webview backend, day-one usable)

1. As a user, I want to open werust on Linux and browse the normal server web
   (enter a URL, see a page, navigate), so that the browser is usable from day
   one — served by a system-webview (WebKitGTK) backend.
2. As a user, I want back / forward / reload / stop and a live, interactive view
   (scroll, click, focus, keyboard input forwarded to the page), so that it
   behaves like a real browser and not a static viewer.
3. As a developer, I want the webview backend to sit behind a wide,
   hot-swappable `Renderer` seam (navigate/reload/stop, live interactive view,
   input/scroll/focus forwarding, load-lifecycle events, a script-message
   bridge, and a request-interception / custom-scheme hook), so that a native
   renderer can later be swapped in without touching the rest of the browser.
4. As a developer, I want a backend to QUALIFY only if it satisfies the trust
   hooks (provider injection + `ipfs://` scheme), not merely if it renders well,
   so that the seam encodes the thesis rather than just abstracting rendering.

### The trust hooks (first-class, on the webview backend first)

5. As a user, I want a native Ethereum **EIP-1193 provider** injected into pages
   (via the `Renderer` seam's script-message bridge), so that dapp frontends see
   a native provider — a first-class capability, not an extension.
6. As a user, I want `ipfs://` URLs resolved natively through the seam's
   custom-scheme / request-interception hook and **hash-verified** on the
   content-addressed fetch path, so that verifiable content-addressed content is
   a first-class scheme rendered at parity with the server path.
7. As a user, I want a visible indicator distinguishing content-verified
   (content-addressed) loads from unverified served-origin loads, so that the
   trust posture is a product surface, not a silent internal (per `0001`).

### The Fetcher seam (bound HTTP+TLS + verified content-addressed path)

8. As a developer, I want networking behind a `Fetcher` seam that binds a vetted
   HTTP+TLS stack (rustls or bound libcurl) — with TLS NEVER hand-written — so
   that the dangerous part is delegated to a vetted implementation.
9. As a developer, I want a hash-verified content-addressed fetch path in the
   `Fetcher` seam (verification moves to the hash), so that the `ipfs://` path
   has a real verifying fetch, not a trusting one.

### Native renderer — T0 (fixed v0 subset, behind the seam)

10. As a developer, I want a T0 native render path behind the `Renderer` seam: a
    naive subset tokenizer + allowlist tree builder, a real cascade over a small
    property set, block/inline flow, software text — assembled from the pure-Rust
    stack where possible, so that the v0 subset wezig already reached is matched
    in Rust.
11. As a user, I want the T0 **server-web floor** page (an authored static
    fragment on the v0 allowlist) to render correctly via the native path against
    committed golden fixtures, so that T0's server floor is objectively met.
12. As a user, I want the T0 **content-addressed floor** page (the same class of
    subset fragment over `ipfs://` / the content-addressed seam) to render
    identically to the server path, so that T0 is not "reached" until BOTH floors
    land.

### Native renderer — T1 (real static documents: the first real experiment)

13. As a developer, I want a real WHATWG-algorithm HTML parser (html5ever) behind
    the `Tokenizer | TreeBuilder` seam replacing the subset tokenizer, so that
    real documents parse correctly.
14. As a developer, I want a core CSS engine (stylo cascade) covering the common
    box-model, colour, typography, and normal-flow properties, plus real
    Latin/LTR shaping (parley/cosmic-text), so that real static block/inline
    layout is produced (no floats/flex/grid/tables — that is T2; no JS — that is
    T3).
15. As a user, I want the T1 **server-web floor** pages to render correctly via
    the native path: a real hand-authored article/doc page (a single MDN article
    / a `motherfuckingwebsite.com`-class page) AND an independently-authored
    static blog/news post, with properly shaped text, so the tier is not tuned to
    one exemplar.
16. As a user, I want the T1 **content-addressed floor** page — a real `ipfs://`
    static site fetched by CID (a Jekyll/Hugo-class site pinned to a CID) —
    rendered at parity with the server path, so that the thesis lands FIRST here:
    a verifiable content-addressed document opened as a first-class page.
17. As a developer, I want the T1 WPT-subset bar wired as the objective
    regression meter (≥ 90 % on `html/syntax/parsing/` tree-construction; ≥ 70 %
    across `css/CSS2/normal-flow/`, `css/css-box/`, `css/css-color/`,
    `css/css-fonts/`, `css/css-text/`; complex-script/bidi excluded), so that T1
    is guarded objectively while the page checklist remains the roadmap driver.

### Mobile parity + release

18. As a user, I want werust to build and run on iOS and Android (Swift/Kotlin
    only at the forced OS edge), so that mobile parity with wezig is reached.
19. As a maintainer, I want releases cut via GoReleaser's Rust builder (a
    deliberately Zig-less build path) producing desktop binaries + the mobile
    artifacts (Android APK, iOS simulator `.app`) with a conventional-commit
    changelog, so that release parity with wezig — mobile included — is met
    (`docs/adr/0002`).

### The experiment's measurement (feeds the follow-on exploration spec)

20. As a maintainer, I want the T1 climb measured AGAINST wezig's Zig arm on the
    shared conformance ladder (effort, code volume, friction — especially DOM
    object-graph friction), so that the reversible experiment produces a real
    finding either way.
21. As a maintainer, I want a **capability + trust-hook benchmark harness** that
    evaluates candidate native-renderer architectures (own from-scratch Rust
    engine vs reused Servo behind the seam vs a Blitz/Stylo-component assembly)
    against BOTH rendering capability AND the trust hooks (provider injection +
    `ipfs://` scheme), so that the follow-on architecture decision is made on
    EVIDENCE, not now (see the exploration spec, `taskedAfter` this one).

### Autonomy notes (the two gate axes)

- **`humanOnly`:** omitted. An agent MAY drive the tasking of this spec — the
  committed stories are well-specified vertical slices standing on documented
  mature libraries and the pinned conformance ladder. (Individual tasks the
  tasker emits may carry their own `humanOnly` where a security boundary — e.g.
  the wallet/provider path or TLS binding — warrants human judgement; that is the
  tasker's per-task call, NOT set here.)
- **`needsAnswers`:** omitted — this committed slice is fully build-taskable. The
  genuinely-deferred decisions (native-renderer architecture; TLS trust-store /
  pinning; wallet broker security model; display name) do NOT gate this slice:
  they are carried by the follow-on exploration spec
  (`rust-successor-native-renderer-architecture-benchmark`, `needsAnswers: true`),
  or are name-independent (the code slug `werust` is used throughout so a later
  rename never churns cross-references).

> Tasked — the Implementation/Testing detail that seeded tasking has moved into
> the `work/tasks/` tasks (what to build) and the durable rationale into ADRs
> (`docs/adr/0001` thesis, `docs/adr/0002` release; the pinned ladder in
> `docs/conformance-tiers.md`). This spec has settled to its durable framing
> below. Current truth: `docs/adr/` + code; remaining work: `work/tasks/`.

## Out of Scope

- **The native-renderer architecture DECISION** (own from-scratch Rust engine vs
  reused Servo vs Blitz/Stylo-component assembly) — deferred to the follow-on
  exploration spec `rust-successor-native-renderer-architecture-benchmark`,
  decided empirically by story 21's benchmark. This spec BUILDS the benchmark and
  climbs to T1 on an assembled stack; it does not PICK the final architecture.
- **T2 (floats/flex/grid/tables + complex-script/bidi) and T3 (JS + networking +
  dynamic DOM)** — beyond this slice; they extend the ladder later.
- **TLS trust-store / pinning policy**, and whether content-addressed fetches
  relax origin trust because verification moves to the hash — deferred (carried
  as an open question on the exploration spec).
- **The wallet broker security model** (own-process signing broker; the page
  never holds keys) — deferred (carried on the exploration spec). This slice
  injects the EIP-1193 provider; it does not finalise the key-custody model.
- **The user-facing product/display name** — deliberately undecided; the code
  slug `werust` is used for the work/ identity.
- **A pure-Rust JS engine and a hand-written GL/TLS stack** — explicitly never
  day-one; bound engines are used (an in-house engine is an aspirational later
  swap-in only).

## Further Notes

- This is a **reversible experiment** (`docs/adr/0001`): wezig is the live control
  group; a Rust arm that drowns in DOM object-graph friction is a VALID finding.
- The ported reference material (`docs/adr/0001` thesis, `docs/adr/0002` release,
  `docs/conformance-tiers.md` ladder) is the fixed frame; this spec is the
  committed build slice against it.
