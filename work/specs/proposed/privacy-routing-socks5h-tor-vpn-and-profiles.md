---
title: "werust: privacy routing (SOCKS5h / Tor / VPN, embedded Tor option) + browser profiles, leak-proof by construction"
slug: privacy-routing-socks5h-tor-vpn-and-profiles
status: proposed
needsAnswers: true
---

> PROPOSED spec \u2014 records intent for human review before tasking. Privacy/anonymity is a
> domain where a HALF-implementation is WORSE than none (a leak gives false confidence), so
> this is specced conservatively: the central requirement is LEAK-PROOF BY CONSTRUCTION, not
> "point at a proxy and hope". Cross-cuts the subsystem model
> (`gated-protocol-subsystems-consent-and-lazy-activation`) because embedded backends leak
> too. Not yet tasked; OPEN QUESTIONS below need answers first.

## Problem Statement

werust should be privacy-friendly: a user can route ALL of the browser's traffic through a
privacy transport \u2014 an external **SOCKS5h** endpoint, **Tor**, or a **VPN** \u2014 and optionally
have **Tor running embedded** (Tor-Browser-style, no separate Tor install). Whatever is
chosen, **no traffic may leak outside the chosen transport** (no direct DNS, no direct
connections, no WebRTC/STUN escape, no embedded-subsystem side-channel). Configuration is
possible both as a launch flag (`--socks5h http://localhost:9050`-style) AND as a PERSISTED
setting, and werust supports **profiles** (Chrome-style) so a user can keep, e.g., a normal
profile and a Tor profile with separate state, cookies, and routing.

## The non-negotiable: leak-proof by construction

The hard part is not "send webview traffic to a proxy" \u2014 WebKitGTK supports that. The hard
part is that MANY things make network connections, and a single one bypassing the transport
DEANONYMISES the user. The spec's central rule: when a privacy transport is active, EVERY
network egress goes through it or is BLOCKED; there is no direct-connection fallback.
Enumerated leak vectors werust MUST close:

1. **DNS.** Use `socks5h` semantics (remote DNS \u2014 the PROXY resolves hostnames), NEVER
   `socks5` (local DNS = the ISP sees every hostname). This single distinction is the most
   common leak; the `h` is mandatory for anonymity. No system-resolver call may happen for a
   proxied navigation.
2. **The webview backend (WebKitGTK).** Set `WEBKIT_NETWORK_PROXY_MODE_CUSTOM` with the SOCKS
   URI on the `WebKitNetworkSession`/context, and verify no scheme/host bypasses it.
3. **WebRTC / STUN / ICE.** WebRTC can open direct UDP connections revealing the real IP even
   behind a proxy \u2014 the classic browser deanonymisation. Must be disabled or forced through
   the transport (Tor cannot carry UDP; likely DISABLE WebRTC entirely in privacy mode).
4. **The `Fetcher` (`ureq`/rustls).** werust's own HTTP fetch path (server-web + the IPFS
   gateway `ContentSource`) must route through the SOCKS proxy too, not connect directly.
5. **EMBEDDED SUBSYSTEMS (the werust-specific danger).** This is the leak vector generic
   browsers do not have: an embedded Freenet node, a local IPFS node, or the Ethereum light
   client's execution-RPC calls each open their OWN connections. If the webview is Tor'd but
   the Freenet node dials peers directly, the user is deanonymised. RULE: when a privacy
   transport is active, every consent-gated / provider-mode subsystem MUST either route
   through the same transport or be BLOCKED (and the user told). Some P2P subsystems may be
   fundamentally incompatible with Tor (UDP, direct peer dials) \u2014 then they are DISABLED in
   that profile, not silently leaking. (Direct tie to
   `gated-protocol-subsystems-consent-and-lazy-activation`: privacy mode is a constraint on
   provider mode.)
6. **Captive-portal / OS proxy-bypass / IPv6.** No `NO_PROXY`-style bypass, no IPv6 escape if
   the transport is IPv4-only, no OS-level split.
7. **Timing/identity correlation (best-effort, note not solve).** werust is NOT claiming to
   be Tor Browser's full fingerprinting-resistance; it claims NO NETWORK LEAK. Fingerprinting
   defences (letterboxing, UA normalisation) are a SEPARATE, later concern \u2014 called out so we
   do not over-promise anonymity we have not built.

**Fail-closed:** if the transport is unreachable or a component cannot be routed through it,
werust BLOCKS that traffic and surfaces a clear error \u2014 it does NOT fall back to a direct
connection. Better a failed load than a silent deanonymisation.

## Solution (shape, not final)

### Transport configuration (flag + persisted + profile-scoped)

- **Launch flag:** e.g. `--socks5h host:port` (or `--proxy socks5h://host:port`) sets the
  transport for that run. (Confirm exact flag spelling in open Q; `socks5h` semantics are the
  point regardless.)
- **Persisted setting:** the same choice saved in config (per profile), so it survives
  restarts without the flag.
- **Transport kinds:** external SOCKS5h endpoint (generic; covers a user's own Tor/VPN/SSH
  tunnel at `localhost:9050`-class), and \u2014 as an option \u2014 EMBEDDED Tor (below). A "system
  VPN" is largely transparent (OS routes all traffic) but werust should still not leak DNS
  around it; the SOCKS5h path is the primary explicit mechanism.

### Embedded Tor (the Tor-Browser-style option)

- Optionally run Tor IN werust (bundled `arti`, the Rust Tor implementation, is the
  thesis-aligned candidate \u2014 confirm in open Q), so the user gets Tor with no separate
  install. This is itself a heavyweight `Subsystem` under the gated-subsystems model
  (consent + lazy start + a bootstrap/circuit delay to disclose), provider-mode = embedded
  vs external-Tor (`localhost:9050`).
- When embedded Tor is the profile's transport, the SAME leak-proofing rules apply; the
  transport just happens to be in-process.

### Profiles (Chrome-style)

- Named profiles, each with its OWN: state (cookies, storage, cache, history), subsystem
  config/consents, AND privacy transport. So a "Tor" profile routes everything through Tor
  with its own isolated state; a "normal" profile is direct. Switching profiles switches the
  whole isolation boundary.
- Profile selection at launch (`--profile <name>`) and at runtime; a default profile.
- Isolation is the point: no state bleed between profiles (a Tor profile must not share
  cookies/cache/DNS-cache with a direct profile). This also underpins future
  container/multi-account use.

### Reuse vs new

- REUSE: the WebKitGTK `Renderer` backend (accepts custom proxy settings); the `Fetcher`
  seam (route its client through the proxy); the subsystem model (embedded Tor is a
  subsystem; privacy mode constrains every other subsystem's provider mode); config/settings.
- NEW: the transport config (flag + persisted + per-profile); the leak-proofing enforcement
  across webview + fetcher + subsystems + WebRTC; embedded Tor (`arti`) as a subsystem; the
  PROFILE system (isolated state + per-profile transport); a leak-test / self-check.

## User Stories

1. As a user, I launch `werust --socks5h localhost:9050` and ALL browser traffic (pages, DNS,
   fetches) goes through that proxy \u2014 nothing leaks directly.
2. As a user, I save that transport in settings so I do not need the flag each time.
3. As a user, I can turn on embedded Tor and browse over Tor with no separate Tor install.
4. As a user, if the proxy/Tor is down, loads FAIL clearly rather than silently going direct.
5. As a user, I have separate profiles (e.g. "normal" and "tor") with isolated state and
   their own routing, and I can switch between them.
6. As a privacy-conscious user, WebRTC and any embedded subsystem that cannot be routed
   through my transport are disabled in my private profile, and I am told so \u2014 no surprise
   direct connection.
7. As a user, I can run a leak self-check that confirms DNS + connections + WebRTC are not
   escaping the transport.

## Phased delivery (proposed, for review)

- **Phase 1 \u2014 external SOCKS5h, leak-proof webview + fetcher:** `--socks5h`/persisted config
  routes the WebKitGTK webview (custom proxy, remote DNS) AND the `Fetcher` through the
  proxy; DISABLE WebRTC in proxied mode; fail-closed on transport-down; a DNS/connection
  leak self-check. (No profiles/embedded-Tor yet; this is the core anti-leak spine.)
- **Phase 2 \u2014 profiles:** Chrome-style isolated profiles with per-profile state + transport.
- **Phase 3 \u2014 embedded Tor + subsystem routing:** embedded Tor (`arti`) as a gated subsystem;
  enforce that every OTHER subsystem (Freenet/IPFS/light-client) either routes through the
  active transport or is blocked-with-notice; the "some P2P subsystems disabled under Tor"
  policy.
- **Phase 4 \u2014 hardening:** mobile transport/Tor story; broader leak-vector audit (IPv6,
  captive portal); optional fingerprinting defences (explicitly separate from no-leak).

## Out of Scope (for this spec)

- Full Tor-Browser-grade FINGERPRINTING resistance (letterboxing, exhaustive UA/JS surface
  normalisation) \u2014 werust promises NO NETWORK LEAK here, not unlinkability. Fingerprinting is
  the separate, sequenced-after spec `fingerprinting-resistance-tor-browser-grade` (kept in
  view per the human's request). THIS spec must still be built fingerprinting-AWARE (uniform
  headers, single bundled font, per-profile isolation) so that follow-on needs no rework; the
  unlinkability guarantee is that spec's, not this one's.
- The per-protocol backends themselves (their specs) \u2014 this governs how they are ROUTED, not
  what they are.
- System-level VPN setup (that is the OS's job) \u2014 werust ensures it does not leak AROUND a
  VPN (DNS), but does not configure the VPN.

## OPEN QUESTIONS (must be answered before tasking \u2014 needsAnswers: true)

1. **Flag surface.** Exact launch flag(s): `--socks5h host:port`? `--proxy socks5h://...`?
   Both + a `--tor` shortcut for embedded Tor? (The `socks5h` remote-DNS semantics are
   required regardless of spelling.)
2. **WebRTC policy in privacy mode.** Disable WebRTC entirely (safest; Tor cannot carry UDP)
   or attempt to route it? (Recommend: DISABLE in any proxied/Tor profile.)
3. **Embedded Tor engine.** `arti` (Rust Tor, thesis-aligned) vs bundling the C Tor daemon?
   `arti`'s maturity for browser use is the risk (spike-gate it, like Freenet).
4. **Subsystem-under-Tor policy.** For each subsystem (Freenet node, IPFS node, light-client
   RPC): route-through-transport where possible, else DISABLE-with-notice in a private
   profile. Which are routable (TCP-only, SOCKS-aware) vs must-be-disabled (UDP/direct-dial)?
   This needs a per-subsystem determination.
5. **Fail-closed strictness.** Confirm: transport-down / un-routable component => BLOCK +
   error, NEVER direct fallback. (Recommend: yes, absolutely \u2014 the whole point.)
6. **Profile model depth.** How Chrome-like? (Separate cookie/storage/cache/history +
   per-profile transport + per-profile subsystem consents.) Profile switching UX; a default
   profile; `--profile <name>`. On-disk isolation layout.
7. **Leak self-check.** Ship a built-in leak test (DNS/connection/WebRTC egress check against
   the transport) the user can run \u2014 how thorough, and does it run automatically when a
   private profile activates?
8. **Mobile.** SOCKS5h + embedded Tor + profiles on Android/iOS (platform VPN/proxy APIs,
   Orbot-style external Tor, background limits). Desktop-first acceptable?
9. **Scope honesty.** Confirm the promise is NO NETWORK LEAK, with fingerprinting-resistance
   explicitly OUT of scope for now, so marketing/UI never implies full Tor-Browser anonymity.

## Why this is the right long-run bet

Privacy is a first-class reason a user would choose a browser like werust, and it composes
naturally with the decentralised-protocol thesis (Tor + ipfs/ENS/Freenet is a strong
privacy-and-sovereignty story). But it is ALSO the feature most dangerous to ship
half-done: the werust-specific twist \u2014 embedded subsystems that make their own connections \u2014
means privacy routing MUST be designed together with the subsystem model, not bolted on.
Speccing it now, leak-proof-by-construction and fail-closed, with an honest scope (no-leak,
not yet unlinkability), is what makes it trustworthy rather than a false promise.
