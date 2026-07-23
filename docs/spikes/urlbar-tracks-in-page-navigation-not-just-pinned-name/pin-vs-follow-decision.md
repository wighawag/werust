# Pin-vs-follow decision: the URL bar on in-page navigation within an ENS page

Date: 2026-07-23
Task: `urlbar-tracks-in-page-navigation-not-just-pinned-name`
Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton` (user story 2)
Builds on: `ens-history-name-rederive-async-and-normalized` (the normalized `ens_pages` re-derive)

## The problem (v0.2.3 field finding C)

The ENS front door pins the typed `.eth` name in the URL bar via `url_override = Some(name)`, and that override PERSISTS across pumps (so the name stays put while the resolved CID loads). `pump()` writes `chrome.url_text` only when `!pinned`. So while an ENS name is pinned, ANY subsequent in-page navigation (a link click, which the real webview delivers as fresh `LoadEvent`s carrying a NEW backend URL, WITHOUT the shell calling `navigate()`) is suppressed from the bar: the name stays frozen and the new path never appears. The "shows on back only after visiting a new site first" symptom is the same stickiness clearing only once a non-ENS navigation drops the pin.

## The decision

**Drop the name pin once the user navigates OFF the resolved-root entry, so the bar FOLLOWS the backend URL for in-page navigation.** (Option 2 in the task, the recommended one.)

The pin is for the front-door ROOT load ONLY: the identity the user typed (`ronan.eth`) stays in the bar while its CID root loads. But an in-page navigation is a FRESH backend load to a DIFFERENT resource, and the bar must show where the user actually is. The root entry is NOT lost: it is stored in `ens_pages` keyed on its normalized CID, so a back/forward return to it re-derives the `.eth` name + posture via the normalized re-derive from the blockedBy task. Dropping the pin on in-page nav is safe precisely because the root is recoverable that way.

## Alternative considered and rejected

**Show `name/<path>` (e.g. `ronan.eth/some/page`)** so the identity AND the location are both honest. Rejected as the primary because:

- It requires deriving the in-page path cleanly from the backend URL RELATIVE to the resolved root, which is only well-defined when the in-page resource is a sub-path of the SAME root CID. A link to a DIFFERENT CID (or an off-site `https://` link) has no clean `name/<path>` form, so the code would need the follow-the-URL fallback ANYWAY. Adding the suffix path on top would be a second, conditionally-applicable display rule layered over the follow behaviour, i.e. more surface for the same honesty the follow behaviour already delivers.
- Follow-the-backend-URL is the browser-idiomatic behaviour (Brave/Opera show the real location on in-page nav), matches user expectation, and keeps the trust posture and the bar text tracking the SAME actual load path with one rule.

The `name/<path>` nicety remains open as a later cosmetic improvement for the same-root sub-path case; it is NOT needed to satisfy the finding.

## How it is implemented (mechanism)

- A new field `BrowserShell::pinned_root_key: Option<String>` records the NORMALIZED CID key (`ipfs::normalize_ens_page_key`) of the resolved root the current `url_override` name was pinned FOR. It is set only in `load_resolved_content` (the one place that pins a name AND has a backend root URL), and cleared everywhere `url_override` is cleared or re-pinned without a backend root (`navigate`, `go_back`, `go_forward`, `reload`, `fail_ens_load`, `fail_invalid_entry`).
- In `pump()`, for each drained `LoadEvent`, `drop_pin_on_in_page_nav(event.url())` compares the event URL's normalized key against `pinned_root_key`. The pinned root's own lifecycle events carry that same root CID, so they keep the pin; an in-page navigation's event URL normalizes to a DIFFERENT key, so the pin (`url_override` + `pinned_root_key`) is dropped and the bar follows the backend URL from there.
- Posture already tracks the load path: `refresh_chrome` re-marks the ENS axes ONLY when `current_url` is a known `ens_pages` entry. An in-page move to a non-ENS resource is not in `ens_pages`, and the backend resets posture to `UnverifiedOrigin` on each fresh `Started`, so an in-page move to a non-verified resource does not keep a stale ENS/verified posture. No posture change was needed beyond dropping the pin.

## Coherence check (new concept: `pinned_root_key`)

- Name: `pinned_root_key` reuses the existing "pin" language (`url_override` pins the display name) and the existing "normalized ens_pages key" concept; it does not re-mean either. It is the missing companion to `url_override`: WHICH root the name is pinned for, so the pin's scope (root-only) is representable.
- Layer: it lives on `BrowserShell` beside `url_override`/`ens_pages`, the shell's bar-identity state, and is consulted only in `pump` where in-page lifecycle events arrive. Correct layer (the shell owns the bar identity; the seam owns raw load events).
- No duplication: it does not overlap `ens_pages` (CID -> identity, for history re-derive) nor `url_override` (the display string); it is the pin's ROOT scope, which neither previously captured.

## Test coverage (network-isolated, FakeBackend)

- `in_page_navigation_on_an_ens_page_updates_the_bar_and_back_re_derives_the_name`: loads `ronan.eth`, drives an in-page link click via the new `BackendHandle::navigate_in_page` (which delivers fresh `LoadEvent`s WITHOUT the shell calling `navigate`, exactly as the real webview does), asserts the bar follows the in-page URL (no longer frozen on `ronan.eth`), the posture tracks the (non-ENS) load path, and a back to the root re-derives the name + `NameViaTrustedRpc` posture via the normalized `ens_pages` re-derive.
- `in_page_navigation_on_a_plain_page_tracks_its_url_unregressed`: a plain (non-pinned) page follows its URL on an in-page link click, unchanged.

The FakeBackend gained `navigate_in_page`, which models the previously-unmodeled path: a link-click load the backend delivers without the shell driving `navigate`. This is where the pinned name used to freeze the bar, and it is the seam behaviour the old fake could not express.
