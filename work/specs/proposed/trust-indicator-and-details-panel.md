---
title: "Give the trust indicator a real icon and a details panel: colour-coded posture in the toolbar, protocol identity and certificate detail behind a click"
slug: trust-indicator-and-details-panel
---

> Launch snapshot — records intent at creation, NOT maintained. Current truth: `docs/adr/` (decisions) + the code; remaining work: `work/tasks/ready/` tasks.

## Problem Statement

The trust indicator is the most important element in werust's chrome, and it currently looks like the least intentional one.

It is drawn with text glyphs (`✓ verified`, `◇ content verified, mutable name`, `◈ name via trusted RPC`) baked directly into the label strings. Those characters render inconsistently across platforms, have no weight or optical alignment matching the surrounding UI, and cannot be replaced by an edge without re-deriving the string they are embedded in. The single most security-relevant thing werust shows is improvised typography.

There is also nowhere to see the DETAIL behind the verdict. A user cannot find out which protocol resolved this page, which CID it actually is, or, on the ordinary web, anything at all about the TLS certificate, which every mainstream browser exposes on exactly this click. The posture is asserted without evidence, in a browser whose entire premise is not taking assertions on faith.

## Solution

The trust indicator becomes a real, colour-coded icon, and clicking it opens a details panel.

- The **icon** is derived centrally like every other chrome fact, so all five edges show the same posture the same way, with each edge drawing a native-quality asset rather than a glyph.
- **Colour** carries the posture's severity, so the verdict is readable before the words are.
- The **panel** carries the evidence: which protocol resolved the page (IPFS, ENS, IPNS, Swarm, plain https), the resolved CID or record, the existing mutable-name comparison and bless action, and, for `https://` pages, certificate detail in the manner of a mainstream browser.

The privacy indicator that this spec originally also covered is DEFERRED (`docs/adr/0012` status note): every channel werust can use today sits on the same rung, so it would show one constant value forever. Privacy is surfaced in words in the retrieval settings surface instead, until routing or a local node or built-in retrieval gives it something to distinguish.

## User Stories

1. As a user, I want the trust posture shown as a clear icon, so that I can read it at a glance rather than parsing a glyph and a sentence.
2. As a user, I want the icon colour-coded, so that I can tell a verified page from an unverified one without reading at all.
3. As a user, I want that icon to look native on my platform, so that the most security-relevant part of the chrome looks deliberate.
4. As a user of more than one platform, I want the same posture to look the same everywhere, so that a page cannot seem safer on my phone than on my desktop.
5. As a colour-blind user, I want the posture distinguishable without relying on hue alone, so that the indicator works for me at all.
6. As a user, I want to click the indicator and see the details behind the verdict, so that I can check the evidence rather than trusting a colour.
7. As a user, I want the panel to tell me which protocol resolved this page, so that I know whether I am reading something fetched by name or by hash.
8. As a user, I want to see the actual CID or record the page resolved to, so that I can compare it against something I know independently.
9. As a user on an ordinary https site, I want to see the certificate, so that werust does not do less than the browser I came from.
10. As a user of a mutable name, I want the existing resolved-versus-blessed comparison and the bless action to stay where I already find them, so that this change does not move a security action I rely on.
11. As a user whose blessed name now points somewhere new, I want that to be the loudest thing the chrome shows, so that I cannot miss it.
12. As a mobile user, I want the indicator and its panel to work on a phone without crowding the URL bar, so that the chrome stays usable.
13. As a user, I want the panel to explain each posture in words, so that the indicator teaches me the model rather than just rating pages.

### Autonomy notes

- **`humanOnly`**: not set. The trust model itself is already decided and recorded (`docs/adr/0006`, `docs/adr/0012`); what remains is presentation.
- **`needsAnswers`**: not set. Both launch questions were answered by the human on 2026-08-01 and are recorded under Implementation Decisions (the colour mapping including its ordinary-web consequence, and the certificate-detail floor plus its honest-degradation rule).

## Implementation Decisions

**The icon is a TOKEN exported by core; the asset belongs to the edge.** Core already exports `trust_indicator` (text), `trust_indicator_detail` (explanation) and `trust_indicator_css_class` (exactly one of the mutually-exclusive `TRUST_INDICATOR_CSS_CLASSES`). Add an icon-token family in the same shape, and take the glyph OUT of the label text so an edge drawing a real icon is not also drawing a character beside it. Assets are real (SVG in-repo where needed), not Unicode.

**One derivation, five painters, no exceptions.** The repo has paid for this twice already: the trust EXPLANATION once shipped desktop-only, and the mobile `when`/`switch` twins drifted from it. A painter must iterate the exported family rather than a local literal list, which is the existing guard against the stale-badge bug.

**Protocol identity lives in the PANEL, never in the toolbar badge.** IPFS, ENS, IPNS, Swarm and https are open-ended and will grow; the toolbar keeps exactly one icon whose meaning is the trust verdict, so adding a protocol never adds chrome. This is `docs/adr/0012`'s split, and it is what keeps the badge's vocabulary finite.

**The panel is the EXISTING trust popover, expanded.** It already holds the posture explanation, the resolved-versus-blessed comparison and the bless action; this adds protocol identity, the resolved reference and certificate detail. Never a second surface. If page-originated egress consent is ever built (`work/notes/ideas/mixed-trust-consent-on-verified-pages.md`), this panel is its home too.

**Certificate detail is a per-edge capability, not a uniform feature**, so it needs a capability-matrix row (`docs/adr/0005`) and must degrade honestly on an edge that cannot produce it.

**The colour mapping is fixed** (human, 2026-08-01), over the whole `TRUST_INDICATOR_CSS_CLASSES` family so it is total by construction:

| state | colour | why |
| --- | --- | --- |
| content-verified | **green** | a direct `ipfs://<cid>`: hash-verified and immutable, so nothing can lie about it |
| mutable-name | **orange** | the bytes verified, but the controller can repoint the name |
| name-via-trusted-rpc | **orange** | the bytes verified, but a trusted RPC could misdirect the name |
| unverified-origin | **red** | the ordinary server web: nothing was verified |
| name-changed | **red** | a name the user personally blessed now points elsewhere, the loudest state `docs/adr/0006` defines |
| loading | **neutral** | no posture is being asserted yet |

**The consequence is deliberate and was accepted explicitly: the ENTIRE ordinary https web shows red.** This is a stance, not an oversight. werust's thesis is that the origin is not trusted by default (`docs/adr/0001`), so a page whose bytes nothing verified is exactly what red means, however common that is. Do not soften it to "neutral because it is normal": that would restate the thesis as its opposite. The two red states are still distinguishable by icon and words, since name-changed is a personal-trust violation while unverified-origin is merely the status quo.

**Colour is not the only channel** (story 5): the posture must remain distinguishable by icon shape and by the words already derived, so hue is reinforcement rather than the sole carrier. This matters more than usual here, because two states share red and two share orange, so hue alone cannot separate the pairs even for a user who sees it perfectly.

**Certificate detail has a fixed FLOOR and an honest ceiling** (human, 2026-08-01): the minimum useful set is issuer, subject, validity dates and fingerprint. An edge may show more if it can, but an edge that cannot produce the set **says so** rather than hiding the section: a silently absent panel section looks like a bug and reads like a claim about the certificate. Which edges can produce what is the capability-matrix row's job to record.

## Testing Decisions

The icon-token and colour families get exactly the guards the CSS-class family already has: mutual exclusivity, exactly one per state, and the assertion that a painter iterates the exported family instead of a local list. The glyph's removal from the label text is itself assertable (no posture string may contain one). Panel content is derived-value tested per posture, including that the bless action still appears exactly where `ipns-tofu-pin-and-warn-on-change` put it. Certificate detail is asserted per edge, with the matrix row recording what each can actually produce, and one test that a capability-less edge degrades to an honest message rather than an empty section.

## Out of Scope

- **The privacy indicator.** Deferred; see `docs/adr/0012`'s status note. Do not add a second badge.
- **Changing `TrustPosture`, its axes or its precedence.** `docs/adr/0006` stands unmodified; this spec only presents it.
- **Consent for page-originated egress.** Captured as an idea; it would live in this panel but is a different feature.
- **Fingerprinting resistance and routing.** Separate proposed specs, and neither is claimed by this indicator.

## Further Notes

Worth keeping in view when the privacy indicator is eventually built: `docs/adr/0012` records why it must be a SEPARATE indicator rather than another posture, and the two systems that prove it (a gateway-fetched CID is integrity-perfect and privacy-worst; a PIR-based ENS resolver such as sprl.it is privacy-strong and integrity-weak). The panel designed here is where that second indicator's detail would land, so leaving room for it costs nothing now.
