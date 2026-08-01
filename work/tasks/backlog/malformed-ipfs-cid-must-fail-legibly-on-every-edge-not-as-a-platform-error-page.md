---
title: "A malformed `ipfs://` CID must fail in werust's OWN legible words on every edge: on Android it currently falls through to Chrome's `ERR_UNKNOWN_URL_SCHEME` page"
slug: malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page
blockedBy: []
covers: []
---

## What to build

Field-found 2026-08-01: the human typed an `ipfs://` URL on **Android** with a trailing `.` accidentally appended to the CID (`ipfs://bafybei…66r4.`) and got **Android's own error page**, not werust's. The reported symptom was "ipfs:// is not supported", which is a reasonable conclusion to draw and is wrong: `ipfs://` works fine, and a one-character typo produced a failure mode that looks identical to the scheme being unimplemented.

**The mechanism** (verified by reading the code, not inferred): the Android edge cannot hand `ipfs://` to the platform WebView at all, so `WerustCore.takePendingLoad` maps it to an internal origin (`https://<cid>.ipfs.werust.invalid`) before `loadUrl`. That map is written to be **total by falling back to the input**: when the CID fails to parse it returns the URL unchanged. The raw `ipfs://…` then goes straight to `WebView.loadUrl`, which is exactly what the surrounding code says must never happen, because the platform WebView cannot load that scheme (`net::ERR_UNKNOWN_URL_SCHEME`). The fallback that exists to be safe is what leaks an unloadable URL to the platform.

**Why this is a class and not a typo.** On desktop the same input reaches the registered scheme handler and comes back as werust's fail-closed error with a legible reason; the Windows and iOS edges register real custom-scheme handlers too, so they behave like desktop. Android is the only edge that maps `ipfs://` onto another origin, so it is the only edge with this fall-through. The result is a **platform parity + fail-closed honesty** gap: the same malformed input is legible on three edges and an alien Chrome page on the fourth. That is precisely the class the parity guard exists for (`docs/adr/0005-platform-capability-parity-guard.md`), and the fix is only durable if a matrix row measures it.

Build the behaviour, not the patch: a malformed/unparseable CID in an `ipfs://` entry must surface werust's own failure on **every** edge, and no edge may hand the platform a scheme it cannot load. Whether that is best done by rejecting the malformed CID at the shared entry door (so every edge refuses identically, and the URL bar shows the existing invalid-entry state) or by making the Android map's failure path an explicit werust error instead of a pass-through is the builder's call, but note the entry-door option is the one that gives parity for free, and the two are not exclusive.

## Acceptance criteria

- [ ] An `ipfs://` entry whose CID does not parse produces werust's OWN legible failure on Android (not the platform's error page), with the same honesty the desktop scheme-handler path already gives.
- [ ] The four edges agree on that behaviour, or the difference is a MEASURED and recorded row rather than an accident.
- [ ] No edge passes an `ipfs://` URL to a platform loader that cannot load it: the Android origin map's failure path never returns an unloadable URL to `loadUrl`.
- [ ] A capability-matrix row (`docs/platform-capability-matrix.toml`, ADR-0005) covers "malformed content-address input fails legibly", so this cannot silently regress or recur on a future edge.
- [ ] A test pins the mapping function's behaviour on an unparseable CID directly (it is pure and toolkit-free, so it needs no device).
- [ ] Tests cover the new behaviour and are network-isolated; mirror the repo's existing test style.

## Blocked by

- None. The `ipfs://` scheme handling, the Android origin map and the invalid-entry chrome state all already exist.

## Prompt

> Goal: a malformed `ipfs://` CID must fail in werust's own words on every platform edge. Today, on Android only, it falls through to Chrome's `ERR_UNKNOWN_URL_SCHEME` page, because the Android edge maps `ipfs://<cid>` to an internal `https://<cid>.ipfs.werust.invalid` origin (the platform WebView cannot load `ipfs://` itself) and that mapping falls back to returning its INPUT unchanged when the CID does not parse. The unloadable raw URL then reaches `WebView.loadUrl`. Desktop, Windows and iOS register real custom-scheme handlers, so they surface werust's fail-closed error instead.
>
> Start by reading the Android origin-map module and its `to_webview_url` / `from_webview_url` pair, the Android backend's pending-load accessor that applies the map, and the Kotlin activity's pending-load sync. Then compare with the desktop scheme-handler path (`werust_core::ipfs::resolve_ipfs_request`) so the failure WORDING matches what desktop already says rather than inventing a second vocabulary. Respect the repo's ONE-derivation rule (`CONTEXT.md`, "chrome presentation / painter"): any user-facing reason belongs in the toolkit-free core, never as a Kotlin literal.
>
> This is a PARITY class, not a one-line patch: `docs/adr/0005-platform-capability-parity-guard.md` exists because per-edge gaps like this recur silently, so the durable half of this task is the capability-matrix row that measures the behaviour. Prefer a fix that gives parity by construction (rejecting the malformed CID once, at the shared entry door) over four per-edge patches.
>
> FIRST, check this task against current reality (it is a launch snapshot and may have DRIFTED): confirm the Android origin map still has the fall-back-to-input shape described, and that no edge has changed how it loads `ipfs://` since 2026-08-01. If the mechanism differs, route to needs-attention rather than building against this description.
>
> RECORD non-obvious in-scope decisions durably and link them from the done record. In particular: if you choose entry-door rejection, you are deciding that a malformed `ipfs://` entry is an INVALID ENTRY (the URL-bar red-underline state) rather than a failed LOAD (the error banner), and those are deliberately separate axes in `ChromeState`. That choice is exactly the kind that belongs in a `## Decisions` block or an ADR.

---

### Claiming this task

```sh
dorfl claim malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page --arbiter origin
git fetch origin && git switch -c work/malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page origin/main
git mv work/tasks/ready/malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page.md work/tasks/done/malformed-ipfs-cid-must-fail-legibly-on-every-edge-not-as-a-platform-error-page.md
```
