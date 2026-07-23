---
title: review-gate non-blocking nits for 'track-webview-url-on-spa-clientside-navigation' (Gate 2 approve)
date: 2026-07-23
status: open
reviewOf: track-webview-url-on-spa-clientside-navigation
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'track-webview-url-on-spa-clientside-navigation' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- RATIFY Decision 1: a same-document URL change is a new LoadEvent::UrlChanged variant (not a faked lifecycle event, not a separate poll). Recorded, well-motivated (flows through the one pump drain, reuses url()), and consistent with the task's stated preference.
  (docs/spikes/.../DECISIONS.md Decision 1; renderer/src/lib.rs UrlChanged variant)
- RATIFY Decision 2: ENS association matched on ROOT-CID-PREFIX (EnsIdentity gains root_cid+root_name; ens_identity_for_url does exact-key-then-prefix lookup). Closes the ipfs:// sub-path leak; plain non-ipfs pages never match. Sound.
  (werust-core/src/lib.rs ens_identity_for_url; ipfs.rs ipfs_root_cid_and_path)
- RATIFY Decision 3: a NEW spa-url-tracking parity-matrix row rather than folding into address-bar, so a re-stubbed OS edge on one platform is caught. Coherent with ADR-0005 intent.
  (docs/platform-capability-matrix.toml new capability block)
- Desktop-only ordering nuance (not covered by a test): on a genuine full-page load, WebKitGTK notify::uri can fire with the new target before load-changed(Started) calls life.begin, so url_changed may emit a spurious UrlChanged for a real load. Effect is benign (drop_pin_on_in_page_nav is idempotent and Started runs it too; the arm never touches load state/error, refresh_chrome reconciles). Flagging for ratification; a live desktop check would confirm no double-repaint or transient bar flicker.
  (webview-renderer/src/backend.rs connect_uri_notify + connect_load_changed ordering; headless test only exercises url_changed after a settled load)
