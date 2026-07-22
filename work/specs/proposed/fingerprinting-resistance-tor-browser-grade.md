---
title: "werust: Tor-Browser-grade fingerprinting resistance (unlinkability, not just no-leak)"
slug: fingerprinting-resistance-tor-browser-grade
needsAnswers: true
taskedAfter: [privacy-routing-socks5h-tor-vpn-and-profiles]
---

> PROPOSED spec \u2014 records intent, kept IN VIEW per the human's request but sequenced AFTER
> the no-leak privacy work (`privacy-routing-socks5h-tor-vpn-and-profiles`). Distinct concern:
> that spec promises NO NETWORK LEAK; THIS one promises UNLINKABILITY \u2014 that two werust users
> (or two sessions) look the same to a server, so you cannot be fingerprinted/tracked across
> sites even without a network leak. Much harder, longer horizon, spike-heavy. Not tasked.

## Problem Statement

Even with perfect network routing (Tor, no DNS/WebRTC leak), a browser can still be UNIQUELY
IDENTIFIED and TRACKED across sites by its FINGERPRINT: the exact set of values it exposes to
JavaScript/CSS/HTTP (screen size, fonts, canvas/WebGL rendering, User-Agent, timezone,
language, hardware concurrency, audio stack, etc.). Tor Browser's core defence is making
every user look IDENTICAL on these axes so the anonymity set is the whole population, not one
person. werust should aim for that grade of fingerprinting resistance \u2014 the complement to the
no-leak transport work \u2014 so a privacy user is genuinely unlinkable, not merely un-sniffed.

## Why it is its own spec (and comes later)

- The no-leak spec (`privacy-routing-...`) is about WHERE bytes go; this is about WHAT the
  page can OBSERVE. They are independent: you can leak-proof routing and still be trivially
  fingerprintable, or vice versa. Shipping the no-leak work first is right; over-claiming
  anonymity before this lands is the trap the no-leak spec explicitly guards against.
- It is BACKEND-SENSITIVE: on the WebKitGTK webview backend, werust does not fully control the
  JS engine's exposed surface, so some Tor-Browser techniques (deep JS-API normalisation,
  canvas-readback spoofing) may be HARD or need WebKit patches. On werust's OWN native
  renderer (T2+, no JS yet = T3) the surface is different. So this spec must be honest about
  what is achievable per backend \u2014 a likely reason it is spike-gated.

## Solution (shape, not final \u2014 the fingerprinting axes)

Aim, per Tor Browser's model, to NORMALISE or DENY the high-entropy surfaces:

1. **User-Agent + Client Hints + `navigator` props** \u2014 present one uniform value (locked UA,
   platform, hardware-concurrency, deviceMemory, languages) shared by all werust privacy
   users, not the real machine's.
2. **Screen / window / viewport** \u2014 letterboxing (round window inner size to a fixed grid) so
   window dimensions are not a unique signal; report the letterboxed size to JS.
3. **Canvas / WebGL / WebGPU readback** \u2014 the highest-entropy vector: either deny readback,
   return a uniform/permuted result, or prompt. (Hard on a webview whose GL stack werust does
   not own.)
4. **Fonts** \u2014 expose only a fixed bundled font set (ties to the native renderer's single
   bundled font choice already made for T1 golden determinism \u2014 a nice alignment); block
   system-font enumeration.
5. **Timezone / locale / clock** \u2014 report UTC (or a fixed tz) + reduced-resolution timers to
   blunt timing fingerprints; normalise `Intl`/locale.
6. **Audio (AudioContext) fingerprint** \u2014 normalise/deny the audio-stack signal.
7. **Media / codecs / plugins / MIME enumeration** \u2014 present a uniform set.
8. **HTTP headers** \u2014 uniform Accept-Language/Accept/UA ordering; no per-user variation.
9. **Behavioural** \u2014 (best-effort, note) pointer/keystroke timing, etc. \u2014 out of initial
   scope, flagged.

## User Stories

1. As a privacy user, two werust private-profile sessions present the SAME fingerprint to a
   server, so I cannot be tracked across sites by browser characteristics.
2. As a privacy user, high-entropy APIs (canvas readback, font enumeration, precise timers)
   do not uniquely identify me (denied or normalised), with breakage disclosed.
3. As a user, I understand this is best-effort per backend and where the limits are (honest
   about what the webview backend can and cannot normalise).

## Phased delivery (proposed, for review)

- **Phase 0 \u2014 spike + audit:** measure werust's ACTUAL fingerprint on the WebKitGTK backend
  (run it against a fingerprinting test suite); determine which axes are controllable via
  WebKit APIs / injected script vs. need patches vs. are unfixable on this backend. Output: a
  findings doc + a realistic per-axis plan. (Like wezig's exploration spikes.)
- **Phase 1 \u2014 the cheap, high-value axes:** locked UA + client hints + `navigator` props +
  uniform HTTP headers + timezone/locale normalisation + letterboxing. Mostly achievable via
  proxy/header control + injected script.
- **Phase 2 \u2014 the hard axes:** canvas/WebGL/audio readback normalisation, font-set locking,
  timer-resolution reduction \u2014 to the extent the backend allows (may need WebKit patches, a
  known heavy path werust already contemplated for the ipfs SW-scheme work).
- **Phase 3 \u2014 native-renderer alignment:** as werust's own renderer matures (T2/T3), its
  exposed surface is werust-controlled, so fingerprinting resistance can be BUILT IN rather
  than retrofitted onto a webview \u2014 the long-run home for this.

## Out of Scope (for now)

- Network-layer anonymity (that is `privacy-routing-...`; this ASSUMES it).
- Behavioural/biometric fingerprinting defences (timing of input, scroll) \u2014 later.
- A guarantee of PERFECT unlinkability on the webview backend \u2014 honesty over marketing; some
  axes may be unfixable until the native renderer, and the spec says so.

## OPEN QUESTIONS (needsAnswers: true)

1. **Backend reality.** Which axes are controllable on WebKitGTK via public APIs / injected
   script, which need WebKit patches, which are unfixable there? (Phase-0 spike answers this;
   do not commit scope before it.)
2. **Breakage tolerance.** Fingerprinting defences BREAK sites (canvas apps, font-dependent
   layout). How much breakage is acceptable in a private profile, and is it per-profile /
   toggle-able?
3. **Alignment with the native renderer.** Should serious fingerprinting resistance WAIT for
   werust's own renderer (where the surface is controlled), with only the cheap axes on the
   webview meanwhile? (Likely yes.)
4. **Anonymity-set honesty.** werust's user base is tiny; "look like all werust users" is a
   small anonymity set vs. Tor Browser's. Does werust align its fingerprint with TOR BROWSER's
   (join that larger set) where feasible, rather than inventing its own uniform value?
5. **Scope discipline.** Confirm this stays SEQUENCED AFTER no-leak routing and is never
   implied to be present before it ships \u2014 so werust never falsely claims Tor-Browser-grade
   anonymity.

## Why keep it in view

Fingerprinting resistance is what turns werust's privacy story from "your ISP can't see
hostnames" into "you are genuinely unlinkable" \u2014 the real Tor-Browser value. It is
deliberately later (harder, backend-sensitive, spike-heavy) and pairs with the native-
renderer maturity, but keeping it specced NOW ensures the earlier work (single bundled font,
the no-leak profiles, header control) is built in a fingerprinting-AWARE way rather than
needing rework. Alignment note: the T1 single-bundled-font decision already made for golden
determinism is coincidentally the RIGHT move for font-fingerprinting resistance.
