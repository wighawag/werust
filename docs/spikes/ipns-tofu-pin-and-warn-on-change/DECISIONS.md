# Decisions: trust-on-first-use for mutable names (`ipns-tofu-pin-and-warn-on-change`)

The task's three open questions were answered by a human before the build (they are quoted in the task body). What follows is what those answers left open once the code was written, plus the choices that turned out to touch other commands, flags or tasks. Each entry says what was chosen, why, what was rejected, and what it touches, so a reviewer can ratify or reverse it.

Spec: `work/specs/tasked/ens-to-ipfs-resolution-phase1-rpc-skeleton.md`. Model: `docs/adr/0006` (the two-axis trust posture), whose Consequences already named this task as the tracked follow-on.

## 1. Blessability follows the MUTABILITY AXIS, not the `MutableName` POSTURE

**Chosen.** Every name-resolved load is blessable: an `ipfs-ns` ENS name, an `ipns-ns` ENS name followed through its record, and (when a direct entry point exists) an IPNS name. `BrowserShell::refresh_chrome` fills the new `ChromeState::mutable_name` axis for any entry it recognises in `ens_pages`, regardless of the entry's `mutable` flag.

**Why.** Answer 3 settled the SCOPE as "both IPNS and ENS, per the settled two-axis model". Reading `ChromeState::is_mutable_name()` instead would have silently narrowed that, because that predicate answers a different question: it says which BADGE is showing, and the badge is the loudest-wins display outcome. Today `NameViaTrustedRpc` wins on every ENS load, so `MutableName` is never the visible posture for an ENS page at all (`docs/adr/0006` says exactly this), and an `ipfs-ns` ENS site would have been unblessable even though the ADR calls it controller-repointable. Blessability is the mutability AXIS ("can the controller repoint this?"), which the ADR answers yes for both.

**Rejected.** Gating on `is_mutable_name()` (would have made the feature nearly dead code); adding a third axis flag through the `Renderer` seam (the seam has no notion of a name or a pin, and does not need one).

**Touches.** Nothing outside this task today. It DOES mean that when Phase 2 clears the RPC-trust warning and ENS loads fall back to `MutableName`, blessability is unchanged: the fall-back is a display change, and this decision deliberately does not read the display. The reasoning lives at the choice site, in the `pins` module's "The mutability AXIS, not the `MutableName` POSTURE" section.

## 2. The pin is a SEPARATE `ChromeState` axis, not a fifth `TrustPosture`

**Chosen.** `ChromeState::mutable_name: Option<MutableNameTrust>`, orthogonal to `trust_posture`, exactly as `invalid_entry` is orthogonal to `last_error`. The display rules combine them (a changed pin is the loudest settled state and wins over every posture), but the facts stay apart.

**Why.** `TrustPosture` is the SEAM's truth about how this load's bytes and name were learned, computed by `TrustPosture::after_verify(ens_origin, mutable_name)` inside each backend. A pin is a durable USER decision about a name, read from a file, and unrelated to any backend. Making it a fifth posture would have forced every backend (WebKitGTK, WKWebView, WebView2, and both mobile ones) to learn what a pin is and to acquire a third axis flag, for a fact none of them can observe.

**Rejected.** A fifth `TrustPosture` variant; re-meaning `MutableName` to mean "changed" (would have destroyed the honest floor the ADR built).

**Touches.** `TRUST_INDICATOR_CSS_CLASSES` gains `trust-name-changed`, which is a real cross-edge cost: the GTK `APP_CSS` and the `desktop-paint` palette each need a colour, and both edges' no-unstyled-class gates enforce it. That is the toothed behaviour those gates were built for, not an accident.

## 3. The changed-name warning REUSES the failure-class banner rather than minting a second one

**Chosen.** `error_banner_visible` becomes "a failure-class state", of which there are now two: a failed load, or a mutable name that changed since the user trusted it. A LOAD failure still wins the banner text when both are true. The changed-name banner uses the existing HARD severity class (`error-banner`), so no third banner class is added.

**Why.** Answer 1 settled that a changed pin gets "the SAME prominence the repo already reserves for a fail-closed failure ... PLUS the high-contrast in-view banner treatment", and that a changed pin is failure-class (so it may displace the page, unlike transient in-flight state, per the sibling constraint from `loading-progress-in-the-url-bar-not-a-banner`). A SECOND high-contrast bar would compete for the one slot above the page, and every edge would have to decide which wins. Reusing the hard severity also means the two mobile edges, which colour their banner from the `retryable` FACT rather than from a class, paint it correctly with no change.

**Rejected.** A separate `warning_banner_*` rule set and widget on four edges; a third `ERROR_BANNER_CSS_CLASSES` member (would have needed a colour on every painter for a treatment that is deliberately identical to the hard one).

**Touches.** The `prominent-load-failure` capability row now has a second occupant. The row's own description still names the load-failure case; the new `mutable-name-change-warning` row states the second one and cross-references the shared surface. A reviewer might reasonably say the RULE NAME `error_banner_*` is now slightly wide for what it covers. It was left alone deliberately: renaming it would touch four edges plus three shape guards for a doc-comment-sized gain, and the doc comment at the rule says plainly what the two cases are. **This is the entry most worth a second opinion.**

## 4. The bless button's LABEL has two wordings

**Chosen.** `trust_pin_action_label` returns "Trust this content" on an unblessed name and "Trust the NEW content of this name" on a changed one.

**Why.** The same button means two materially different things: first-use trust, versus the SSH-host-key "I have looked at the change and I accept it". Reading identically in both states would make the second decision look routine.

**Rejected.** One neutral wording; a separate "accept the change" action (two actions for one pin write, and the edges would have to decide which to show).

**Touches.** Every edge, because all four take the label from this rule. That is why it is a core rule and not an edge string, and why the mobile shape guard drives BOTH wordings into its forbidden-literal list.

## 5. `bless_current_name()` returns "was it persisted", and refuses silently otherwise

**Chosen.** `BrowserShell::bless_current_name() -> bool` is gated on the very rule the edges paint the button's visibility from (`trust_pin_action_visible`), applies the pin in memory, saves, and returns whether the save reached disk. It is never an error.

**Why.** A pin store that cannot be written must not break browsing (the fail-safe stance), and there is no user-actionable difference between "no settings directory" and "the write failed": in both cases the bless holds for this session and the chrome updates. Sharing the gate with the button's visibility means "the button is shown" and "the action does something" cannot drift.

**Rejected.** A typed error enum (nothing at any edge would branch on it today); returning "did anything change" (would have conflated a successful in-memory bless with a failed write).

**Touches.** All four FFI/edge call sites, which today ignore the value and simply repaint. Documented at the method.

## 6. Names are keyed case-insensitively; re-blessing REPLACES

**Chosen.** `pin_key(name)` is the trimmed, lower-cased name; at most one pin per name.

**Why.** ENS names are case-insensitive (ENSIP-1 lower-cases before the namehash), so `Ronan.eth` and `ronan.eth` are one name. Two pins under two casings would make the warning MISS, which is the one failure mode a TOFU store cannot have. Replacing on re-bless is the SSH-host-key model: the next change is measured against what the user last accepted, not against the original.

**Rejected.** Verbatim keys (a silent miss); keeping a history of accepted CIDs (nothing in the settled UX reads it, and it is a strictly additive follow-on if a "what changed when" view is ever wanted).

## 7. The date is formatted in-tree rather than by binding a date crate

**Chosen.** `pins::format_utc_date` converts a Unix timestamp to `YYYY-MM-DD` with the closed-form proleptic-Gregorian `civil_from_days` identity, checked by an exhaustive day-by-day walk over four centuries.

**Why.** The warning quotes ONE calendar day back to the user. `chrono` is already in the dependency graph (via `rust-ipns`), so this is not about weight; it is about not adding a direct dependency, and a timezone-free civil-date conversion for a display string is arithmetic, not the "never hand-roll" territory (`docs/adr/0001` is about crypto and TLS). A locale-aware or timezone-aware date WOULD be, and is deliberately not what this is.

**Rejected.** Promoting `chrono` from a dev-dependency to a dependency (defensible, and the obvious reversal if a second date concern ever appears); storing a pre-formatted date string in `pins.json` (would have frozen the format into the persisted file).

## 8. The `macos` bless affordance is tracked, not built

**Chosen.** The parity matrix carries TWO rows: `mutable-name-change-warning` (implemented everywhere) and `mutable-name-tofu-bless` (`stubbed` on macOS, linked to `macos-trust-surface-bless-affordance`).

**Why.** The warning half is pure derivation and reaches the AppKit window through the shared `desktop_paint::ChromePaint` with no macOS change at all. The bless half needs a click target, and the AppKit trust indicator is a plain `NSTextField` label, where GTK, Android and iOS each already had a popover/alert to extend. That is a missing WIRE, not a missing platform, so `n-a` would be a lie. Splitting the rows is the precedent `trust-explanation` set when it was split out of `trust-indicator` "precisely so that gap can never hide inside a row that is green for the badge".

**Rejected.** One row `stubbed` on macOS (would have hidden a working warning behind a missing button); one row `implemented` everywhere (would have claimed an affordance that does not exist); writing the AppKit code blind (this gate compiles no macOS half, so it could not be checked here).

**Touches.** A new backlog task, and the `windows` column when `windows-parity-column-and-stub-tasks` creates it: the Win32 chrome is in exactly the same position and the follow-on task says so.

## What was deliberately NOT built

- **A pin-management surface** (list/forget blessed names). Nothing in the settled UX calls for it, and `pins.json` is a small readable file next to `retrieval.json`. A `werust://` page for it is a natural follow-on if the set ever grows.
- **A CLI surface.** `werust resolve` prints what a name resolves to; it makes no trust claim and holds no chrome, so it neither warns nor blesses. Wiring the pin there would have introduced a second place that can write the store.
- **Blessing anything but the ROOT name.** A pin is keyed on the whole-site identity (`ronan.eth`), so `ronan.eth/blog/` and `ronan.eth` share one pin, matching how `ens_pages` already re-derives a sub-path back to its site.
