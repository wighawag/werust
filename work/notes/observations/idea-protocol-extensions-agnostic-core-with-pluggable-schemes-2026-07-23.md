---
title: "Vision: make werust protocol-AGNOSTIC with pluggable protocol EXTENSIONS (ipfs / ipns / freenet / swarm as add-ons), dogfooded with ipfs"
date: 2026-07-23
status: open
kind: idea
adrCandidate: true
spec: ens-to-ipfs-resolution-phase1-rpc-skeleton
relatedAdr: [0001, 0003, 0004]
---

## The idea (human, v0.2.3 session)

Make werust CORE agnostic to any specific decentralised protocol, and let users EXTEND the browser with protocols - `ipfs`, `ipns`, `freenet`, `swarm`, etc. - by adding a PROTOCOL EXTENSION. The extension supplies a scheme (or a set of schemes) and the verified-retrieval/resolution logic behind it; the core knows nothing protocol-specific. werust ships/dogfoods this model with its OWN ipfs (and ipns) support built AS such an extension - proving the extension surface is real by eating it. Whether ipfs is loaded BY DEFAULT (bundled + on) or is an opt-in extension for a first publicised release is a decision to make when the surface is ready (lean: bundled-by-default so the ronan.eth/ipfs win is out-of-the-box, but structured as an extension so it is removable/replaceable).

ENS keeps a slightly special place: it is less a "content protocol" and more an ANCHOR / naming layer that can point AT various protocols (an ENS contenthash decodes to `ipfs-ns` / `ipns-ns` / swarm / arweave / ...). So ENS is the name-resolution seam that DISPATCHES to whichever protocol extension owns the decoded contenthash's scheme, rather than being one content protocol among the extensions. This is nice: ENS as a cross-protocol anchor, extensions as the content backends it can resolve into.

The payoff: werust core stays FOCUSED (a trust-honest shell + a naming anchor + a set of seams) yet VERSATILE (any protocol a user cares about is an add-on, without bloating the core or forking the browser).

NOT to be done now. Recorded so it is not lost and can be sharpened into an ADR + spec when the seams are ready to generalise.

## Why this fits what already exists (not a rewrite - a generalisation)

werust is ALREADY seam-structured, so this is mostly "widen and make pluggable what is already an internal seam", not a from-scratch design:

- **`ContentRetriever` seam (ADR-0004).** Retrieval is already "given a CID + a path, return verified bytes or a typed failure", with the trust/transport a SWAPPABLE backend, explicitly so that "delegated-routing, an embedded-p2p client, and a user-supplied gateway/node URL [are] pure backend swaps, not rewrites." A protocol extension generalises this from "the ipfs retriever" to "the retriever a registered scheme owns."
- **The scheme-handler edge.** `ipfs://` is already resolved through a registered custom-scheme handler on each platform WebView (`install_ipfs`: `register_uri_scheme` -> hash-verified fetch; ADR-0008 put that retrieval off the UI thread). That per-scheme registration IS the extension seam in embryo - today there is one hard-wired `ipfs` scheme; the generalisation is a REGISTRY of `scheme -> handler` that extensions populate.
- **ENS as the anchor.** `navigate_ens_name` already decodes an ENS contenthash by its OWN type and DISPATCHES (`ipfs-ns` -> ipfs path, `ipns-ns` -> IPNS-resolve-then-ipfs, everything else -> a named refusal, NEVER defaulted to ipfs). That dispatch-by-decoded-type is exactly the "ENS anchors into whichever protocol extension owns the scheme" shape - generalise the match arm into "look up the extension registered for the decoded scheme."
- **ADR-0003 (protocol sequencing) already frames a FAMILY of protocols** (ENS light client, embedded Freenet node, Tor) as `Subsystem`s under a "gated-protocol-subsystems-consent-and-lazy-activation" framework with a provider mode (embedded / external / gateway) and consent + lazy activation. A "protocol extension" is the natural user-facing packaging of that subsystem idea: a subsystem that registers scheme(s) + a retriever/resolver backend, gated by consent, activated lazily. This idea and ADR-0003's framework are the SAME direction seen from two angles (dev seam vs user-facing extension); they should be co-designed.

So the raw materials (a retrieval seam, per-scheme handler registration, ENS dispatch-by-type, a subsystem framework concept) already exist. The work is to (a) make the scheme handler a REGISTRY rather than a hard-wired `ipfs`, (b) define the EXTENSION contract (what a protocol extension provides: scheme(s), a `ContentRetriever`/resolver, trust posture rules, network-egress declaration for the privacy-routing constraint, consent/activation), (c) re-express werust's own ipfs/ipns as an extension over that contract (dogfood), and (d) decide the bundle-by-default question.

## Open questions to resolve when this becomes an ADR/spec (do NOT answer now)

- **Extension mechanism.** In-process Rust trait objects registered at build/config time (safe, simple, no sandboxing - "extension" = a compiled-in backend the user enables) vs a true dynamic/loadable extension (WASM? a process boundary? a manifest?) with its own sandboxing + trust story. The trust-honest posture (ADR-0001/0006) means an extension that can claim "content-verified" must be TRUSTED to actually verify - a third-party dynamic extension asserting verification is a security surface. Likely: start with in-process, curated, compiled-in extensions (ipfs bundled), defer untrusted third-party dynamic loading until the trust/sandbox story is designed.
- **Trust posture per protocol.** Each protocol has its own verification semantics (ipfs = per-block hash vs CID; freenet = its own CHK/SSK integrity; swarm = its own). The two-axis trust posture (ADR-0006) must be extension-supplied and honest per protocol, never a blanket "verified".
- **Privacy-routing constraint (ADR-0003 decision 2).** Every extension must declare + route its OWN network egress through the active transport (an embedded node dialling directly under a Tor profile deanonymises the user). The extension contract must include an egress declaration so a private profile can DISABLE any extension it cannot route.
- **ENS anchor vs content extension.** Confirm ENS stays a distinct naming/anchor seam that dispatches INTO extensions (by decoded contenthash scheme), rather than being modelled as one extension among equals. Registering an unknown decoded scheme should be an honest "no extension for `<scheme>`" refusal (mirrors today's named-refusal-not-defaulted-to-ipfs).
- **Bundle-by-default vs opt-in for the first publicised release.** Lean: ipfs bundled + on (out-of-the-box ronan.eth), structured as a removable/replaceable extension; but decide at readiness.
- **Relation to the existing `retrieval-backend-user-setting`** (done) and `gated-protocol-subsystems-...` (ADR-0003): the user-facing extension manager likely subsumes / builds on both. Reconcile them when specced.

## Suggested next step (NOT now)

When the current field-fix batch settles, promote this to an ADR ("werust is a protocol-agnostic shell; protocols are pluggable extensions; ENS is the cross-protocol anchor") + a spec that (1) defines the protocol-extension contract, (2) turns the hard-wired `ipfs` scheme handler into a registry, (3) re-expresses werust's ipfs/ipns as the first extension over it, and (4) records the bundle-by-default decision. Co-design with ADR-0003's subsystem framework and the privacy-routing spec so the egress/consent constraints are baked in, not retrofitted.
