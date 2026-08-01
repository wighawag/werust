---
title: "Two indicators and one details panel: show trust and privacy separately, with protocol identity and certificate detail behind a click"
slug: trust-and-privacy-indicators
needsAnswers: true
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.

<!-- open-questions -->

## Open questions

1. **Is the privacy indicator always visible, or only when it is notable?** Every ordinary `https://` page is fully observable, so an always-on indicator is red on nearly every visit, which is how the https padlock died. Options: always visible; visible only when BETTER than baseline; visible only when the page DEGRADES it mid-load (a verified page reaching out to a server). This is the display decision `docs/adr/0012` deliberately left open.
2. **What are the discrete privacy levels, exactly?** The ADR gives the ladder in prose (no egress > local+Tor > local > public+Tor > public). Turning it into a fixed set of named levels with one colour each is a product decision, and it must be a total function over every channel werust can use today, including plain https and the injected provider's RPC.
3. **How is the privacy level of a channel ESTABLISHED?** Some of it is configuration werust knows (which retrieval backend, is routing on). Some of it is not knowable (an ISP, a VPN the OS is using, whether a "local" node is actually remote). The indicator must not claim more than werust can actually observe about its own egress.
4. **On mobile, do the two indicators combine into one affordance?** Toolbar width is the same scarce resource that justifies removing back/forward there.

<!-- /open-questions -->

## Problem Statement

werust's trust indicator answers one question well and a second question not at all.

It answers "could anyone have lied to me about this page?", which is `docs/adr/0006`'s integrity model. It says nothing about "who can see what I am reading?", and users of a privacy-focused browser ask that question at least as often. Worse, the two answers frequently point in OPPOSITE directions, so the existing badge is not merely incomplete, it is potentially misleading: a direct `ipfs://<cid>` fetched from a public gateway shows werust's strongest trust state while being one of its least private loads, because that gateway sees every page fetched through it.

The badge is also drawn with text glyphs (`✓ verified`, `◇ content verified, mutable name`), baked into the label strings, which renders inconsistently across platforms and makes the most trust-bearing element of the chrome look the least intentional. And there is nowhere to see the DETAIL behind either answer: which protocol resolved the name, which CID the page actually is, or, on the ordinary web, anything at all about the TLS certificate, which every mainstream browser exposes.

## Solution

Two indicators, and one panel behind them.

- A **trust** indicator, unchanged in meaning from `docs/adr/0006`, given a real icon instead of a text glyph.
- A **privacy** indicator beside it, showing how observable this page's loading was, derived as the worst channel the page touched and degrading as the page loads.
- A **details panel**, opened from either, carrying protocol identity (IPFS, ENS, IPNS, Swarm, plain https), the resolved CID or record, the existing mutable-name comparison and bless action, and, for `https://` pages, certificate detail in the manner of a mainstream browser.

Colour is shared vocabulary across both indicators: green for the fully self-sufficient case, orange where a third party is trusted or observes, red where neither holds. Green is deliberately unreachable today, and that is recorded rather than hidden.

## User Stories

1. As a user, I want to see at a glance whether this page could have been tampered with, so that I know whether to believe what it says.
2. As a user, I want to see at a glance whether fetching this page was observable, so that I know whether reading it was private.
3. As a user, I want those to be SEPARATE indicators, so that a page which is verified but observed does not look the same as one which is private but unverifiable.
4. As a user, I want to click either indicator and see the details, so that I can check the specifics rather than trusting a colour.
5. As a user, I want the panel to tell me which protocol resolved this page, so that I know whether I am reading something fetched by name or by hash.
6. As a user on an ordinary https site, I want to see the certificate, so that werust does not do less than the browser I came from.
7. As a user of a mutable name, I want the existing comparison and bless action to stay where I already find them, so that this change does not move a security action I rely on.
8. As a user, I want the trust icon to look native on my platform, so that the most security-relevant part of the chrome looks deliberate.
9. As a user, I want the same posture to look the same on every platform, so that a page cannot seem safer on my phone than on my desktop.
10. As a privacy-conscious user, I want the indicator to drop when a page reaches out to a server mid-load, so that I can tell a self-contained page from one that phoned home.
11. As a privacy-conscious user, I do NOT want a green badge implying I am unidentifiable when werust has not addressed fingerprinting, so that the indicator never overstates its protection.
12. As a user, I want to understand what a level MEANS in words, not just by colour, so that the indicator teaches rather than merely rates.
13. As a colour-blind user, I want the level distinguishable without relying on hue alone, so that the indicator works for me at all.
14. As a mobile user, I want this to fit in my toolbar without crowding out the URL, so that the chrome stays usable on a phone.

### Autonomy notes

- **`humanOnly`**: not set. The trust-model decision is already made and recorded in `docs/adr/0012`; what remains is implementation.
- **`needsAnswers`**: set. The four questions above are product decisions (visibility policy, the discrete level set, what werust may honestly claim to observe, mobile combination) that would otherwise be guessed at tasking time.

## Implementation Decisions

`docs/adr/0012` is the governing decision and should be read first; it records why privacy is a second indicator rather than another `TrustPosture`, why it is shaped differently, and what it is forbidden from claiming.

**Both indicators derive in the toolkit-free core, like every other chrome fact.** Trust already does (`trust_indicator`, `trust_indicator_detail`, `trust_indicator_css_class` and the mutually-exclusive `TRUST_INDICATOR_CSS_CLASSES` family). Privacy gets the same treatment: level, explanation and class from ONE derivation, so five painters cannot drift. The repo has already paid for that drift twice (the trust explanation shipped desktop-only; the mobile `when`/`switch` twins diverged), which is why this is not negotiable.

**The icon is a TOKEN exported by core; the asset is the edge's.** Add an icon-token family in the shape of the existing CSS-class family, and take the glyph OUT of the label text so an edge drawing a real icon is not also drawing a glyph. The human's answer on assets: real assets, drawn as SVG in-repo where needed. Protocol marks (IPFS, ENS) belong to the PANEL, not the toolbar badge, so the toolbar keeps exactly two icons regardless of how many protocols exist.

**Privacy is worst-of-channels, computed as the load proceeds.** It must reset per navigation and must never improve within one. Note this is a genuinely different mechanism from the settled trust posture, and the obvious implementation (a single value set at commit time) is wrong.

**The panel is the existing trust popover, expanded.** It already holds the posture explanation, the resolved-versus-blessed comparison and the bless action. Adding protocol identity, privacy explanation and certificate detail extends that surface. If page-originated egress consent is ever built (`work/notes/ideas/mixed-trust-consent-on-verified-pages.md`), the panel is its natural home too, which makes it the single trust/privacy/permissions surface rather than the first of three.

**Certificate detail is a per-edge capability.** WebKitGTK, WKWebView (`serverTrust`), Android (`SslCertificate`) and WebView2 expose different information; this needs a capability-matrix row (`docs/adr/0005`) and will likely lag on one edge. It should be built so a missing capability degrades honestly rather than showing an empty panel.

## Testing Decisions

Everything above the painter is pure and testable without a display, which is where the value is: the privacy derivation over a SEQUENCE of channels (proving it takes the worst, not the first, and resets per navigation) is a table test; the level and icon-token families get the mutual-exclusivity and exactly-one-per-state guards the CSS-class family already has, plus the existing style of assertion that a painter iterates the exported family rather than a local literal list. The claim boundary deserves a test of its own in spirit: a privacy level that would imply unlinkability must not be reachable from transport facts alone. Certificate detail is asserted per edge, with the matrix row recording what each can actually produce.

## Out of Scope

- **Fingerprinting resistance.** Separate proposed spec; this indicator explicitly does not claim it (`docs/adr/0012`).
- **Tor / VPN routing itself.** `privacy-routing-socks5h-tor-vpn-and-profiles` builds the capability; this only REPORTS it.
- **Built-in verified retrieval.** Phase-2 work that unlocks green; not built here.
- **Consent for page-originated egress.** Captured as an idea; it would live in this panel but is a different feature.
- **Changing `TrustPosture` or its precedence.** `docs/adr/0006` stands unmodified.

## Further Notes

The external system that settled the two-indicator design is worth keeping in view while building: **sprl.it** resolves ENS names under homomorphic encryption so the server cannot learn the query, yet it can still return a wrong answer undetectably. Any design that can express "green privacy, orange trust" for that system, and "orange privacy, green trust" for a gateway-fetched CID, is expressive enough. Any design that cannot, is not.
