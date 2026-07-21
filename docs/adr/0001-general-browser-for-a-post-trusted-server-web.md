---
status: accepted
---

# A general-purpose web browser that does not trust the server by default

<!-- Thesis: a full, compatible general browser (renders the normal server-focused
web) that is additionally privacy-protecting, local-first, and a place to explore
web apps that do NOT assume a trusted server (IPFS, verifiable content). The
decentralised web is a consequence/expression of that stance, not the purpose.
Ported verbatim in substance from the wezig project (ADR-0011); this repo is the
Rust successor and inherits this thesis unchanged. -->

This project is **a new general-purpose web browser** — not a niche "web3
browser" and not a wallet-with-a-viewport. Its thesis is a **trust model**:
today's browsers were designed for an era where the *server* (the origin) was
king and was implicitly treated as trustworthy — you fetch from a server, you
trust what it sends, and your data lives on it. This browser is built for the
opposite world: one where **the origin is not trusted by default**, content can
be **content-addressed and verifiable** rather than server-authoritative, the
user's **privacy** is protected by default, and the user's experience and data
are **local-first**. Support for the decentralised web (content-addressed
resolution like `ipfs://`, a native Ethereum/EIP-1193 provider, verification of
what you're served) is a **consequence** of that stance, not the purpose.

**It is additive, not oppositional.** It MUST render and run the normal,
server-focused web like any general browser — full compatibility with today's
web is a hard requirement, not a compromise. On top of that baseline, it aims to
be a **place to explore new mechanisms for web apps that do NOT assume a trusted
server** (content-addressed apps like IPFS-hosted frontends, verifiable
resources, local-first data). The difference from incumbents is not *what web it
can open* — it opens all of it — but *whom it trusts by default, how it protects
the user, and where the source of truth is allowed to live*.

## Context

It is easy to describe the differentiators as "native Ethereum/IPFS," which
reads as "a browser for on-chain apps." That framing is too narrow and
mis-scopes downstream decisions (conformance targets, renderer scope, security
model): it pulls toward a niche dapp viewer when the actual goal is a **real
general web browser** that embodies a different trust posture. The load-bearing
"why" — *incumbent browsers assume a trustable server; this browser assumes it
should not* — is recorded here so every downstream decision (what to render, how
far, what to verify, where trust boundaries sit) is made against the right north
star.

## The thesis, stated plainly

- **Incumbent assumption (what we reject as the default):** the server/origin is
  authoritative and trustworthy; the browser's job is to faithfully present
  whatever the origin sends and to keep the user's session/data tied to that
  origin.
- **Our assumption:** the origin is *not* trusted by default. Prefer content
  that is **verifiable** (content-addressed / hash-checked) over content that is
  merely *served*; keep the user's data and experience **local-first**; make the
  trust boundaries **explicit and user-controlled** rather than implicit in
  "whoever answered the request."
- **General, not niche:** this is meant to be a browser you use for the whole
  web. The decentralised-web features (content-addressed resolution, a native
  wallet/provider, served-content verification) are how the thesis shows up in
  practice — first-class, not extension-grafted — but they do not narrow it to
  "web3 sites." A browser that could only render dapp frontends would have
  failed its own goal.
- **Compatible with the server-focused web (hard requirement):** it must open
  and correctly render the normal web that assumes a trusted server. It does not
  *replace* that web; it is a general browser that ADDS a home for the
  don't-trust-the-server alternatives. "Only works on the decentralised web" is
  a failure mode, not a feature.
- **A platform to explore new trust mechanisms:** beyond compatibility, it is
  intended as a place to try out and support NEW mechanisms for web apps that do
  not expect a trusted server (e.g. IPFS-hosted content-addressed apps,
  verifiable resources, local-first sync). Being a real general browser is the
  baseline that makes that exploration credible and reachable by real users.
- **Privacy is a first-class pillar (not just a side-effect):** protecting the
  user — from tracking, from origin overreach, from data exfiltration — is a
  core goal alongside the trust-model and local-first stances, not merely a
  by-product of them. It is a lens every feature is designed under, the same way
  the trust posture is.

## Consequences (why this is load-bearing, not a tagline)

- **Conformance / renderer scope is a GENERAL-browser target, not a dapp
  target.** "How far does the native renderer grow, measured how" must be
  answered against "a real general web browser," not "good enough for the
  on-chain apps we care about." The web3 features do not set the rendering bar.
- **The trust posture is itself a product surface.** Verification of served
  content, content-addressed loading, and explicit origin-trust are features the
  chrome and the renderer must expose — not silent internals (e.g. an indicator
  for "this was content-verified" vs "this was served by an unverified origin").
- **Local-first is a design constraint, not a feature bolt-on.** Data ownership,
  offline capability, and not-tying-the-user-to-an-origin inform storage,
  history, and networking decisions from the start.
- **The `Renderer` seam framing still holds and gains meaning.** Shipping
  usability on a system-webview backend while the native renderer matures is the
  hedge that keeps this project out of the "no usable product for years" trap;
  the *reason* the native renderer matters is now explicit — a browser that
  embodies a different trust model benefits from owning its own engine (it need
  not inherit an incumbent engine's server-trusting defaults).
- **The web3/IPFS work is re-anchored.** Native IPFS + the EIP-1193 provider are
  the decentralised-web *expression* of this thesis (verifiable, content-
  addressed, user-custodied), not the reason to exist. Their security model is a
  consequence of "don't trust the origin," which this ADR makes the root.

## Considered framings (and why rejected)

- **"A web3 / on-chain browser."** Rejected: narrows the project to a niche,
  mis-scopes the renderer to dapp frontends, and buries the actual thesis (trust)
  under one of its consequences (Ethereum). It is the framing this ADR exists to
  correct.
- **"ONLY a privacy browser."** Privacy IS a first-class pillar (see
  Consequences), but it is not the WHOLE framing: privacy is about *who watches
  you*, while the trust thesis is also about *whom you trust for the content and
  where truth lives* (origin-authoritative vs verifiable / local-first). So this
  is a privacy browser AND a don't-trust-the-server, local-first, general
  browser — reducing it to privacy alone would drop the content-trust and
  local-first halves of the thesis.
- **Leaving the thesis implicit.** Rejected: an unrecorded, load-bearing "why"
  silently mis-scopes every decision it should govern.

## Note

This ADR records intent/direction (a "why"), and is deliberately reversible in
its *details* as the product sharpens — but the core stance is the fixed point
the conformance/security targets are reconciled against: a **general** browser,
**fully compatible** with the server-focused web, that is
**origin-not-trusted-by-default**, **privacy-protecting**, and **local-first**,
and that serves as a **place to explore new mechanisms** for web apps that do not
assume a trusted server — with the decentralised web (IPFS, native provider,
verification) as a *consequence and expression* of that stance, not the purpose.
