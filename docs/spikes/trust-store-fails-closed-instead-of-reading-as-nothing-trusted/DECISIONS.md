# Decisions: the trust store's third state (`trust-store-fails-closed-instead-of-reading-as-nothing-trusted`)

The head task of spec `trust-store-hardening`: an unreadable `pins.json` stops reading as "nothing trusted", and nothing is written while that holds. The load-bearing decision (readers get a STATE, writers REFUSE) is an ADR, `docs/adr/0014`, because five sibling tasks build on it and a reviewer would otherwise have to reverse-engineer the asymmetry. What follows is the rest: each says what was chosen, why, what was rejected, and what it touches, so a reviewer can ratify or reverse it.

Task: `work/tasks/*/trust-store-fails-closed-instead-of-reading-as-nothing-trusted.md`. Prior decisions this one builds on: `docs/spikes/pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns/DECISIONS.md` (which kept `TrustedNamePins::load()` as the ONE site that knows where the user's store is, so this task had one read site to teach rather than two).

## 1. "Cannot determine trust" is a `ChromeState` AXIS, not a variant of `MutableNameTrust`

**Chosen.** `ChromeState::trust_undeterminable: Option<UndeterminableTrust>`, beside the existing `mutable_name` axis, with the predicate `ChromeState::trust_is_undeterminable()`. `MutableNameTrust` (name, CID, `blessed`) is unchanged.

**Why.** It is a fact about the STORE, not about this page's name: it holds whether or not the current page is a name-resolved load, and it is what stops an absent pin from being read as "the user has blessed nothing". That is the same reason `invalid_entry` is a separate axis from `last_error`. It also keeps the change small at the edges: every `ChromeState` literal in the repo already ends in `..ChromeState::default()`, so no painter or fixture had to be rewritten.

**Rejected.** Turning `MutableNameTrust::blessed: Option<TrustedNamePin>` into a three-variant enum (`Unblessed` / `Blessed` / `Undeterminable`). It reads well at the one call site that asks "what is on file for this name", but it puts a store-wide fact inside a per-name value, makes it inexpressible on a page with no mutable name, and rewrites every `MutableNameTrust` literal in four crates for no behavioural gain.

**Touches.** `ChromeState` gains a public field and a public predicate; `can_bless_name()` now reads it. Any future rule that asks "is this name blessed" must ask the axis too, or it will make a claim werust cannot support.

## 2. The chrome STATES the third state and offers no bless; the posture badge and the banner are untouched

**Chosen.** While trust cannot be determined: `trust_pin_detail` says so, naming the file and the reason (`pins.json records something werust cannot read honestly: entry 1 (\`ronan.eth\`) records a trust posture this build does not know: \`from-the-future\``); `trust_pin_action_visible` is false, so no edge paints a bless button; `trust_indicator*` and `error_banner*` are unchanged.

**Why.** The task's two constraints are "must not claim a name is unblessed" and "must not break browsing", and the badge is where breaking it would start: the posture says how THIS load's bytes and name were learned, which a settings file knows nothing about, so making a corrupt `pins.json` change the badge on every page would be a false claim in the other direction. The detail string is the ONE place the state is put into words, so all five edges say it identically through the two carriers they already read (`desktop_paint::ChromePaint` and `chrome_json`'s `trustPinDetail`).

**Rejected.** A dedicated badge or a sixth trust CSS class (a store-level fault is not a posture, and every painter would have to learn one); raising the error banner (that surface is failure-class, and a file werust cannot read is not a failed load: it would displace the page on every navigation until the user found the file).

**Touches.** All five edges, via the derived strings only. No edge code changed.

## 3. No new chrome-JSON key

**Chosen.** `chrome_json` carries the third state only through the derived strings it already carries (`trustPinDetail`, `trustPinActionVisible`, `trustPinActionLabel`). No `trustUndeterminable` FACT was minted.

**Why.** The mobile edges paint the TOFU section from those derived fields, so they inherit the new wording and the withdrawn affordance with no Kotlin or Swift change and no new twin to drift. A fact is worth adding when an edge must BRANCH on it (as they branch on `retryable` for colour), and neither edge does here.

**Touches.** If a later task gives an edge a reason to branch (a "your trust store needs attention" affordance, say), it adds the key then; the `ChromeState` axis is already there to encode.

## 4. The shell KEEPS its pin cache while the state holds

**Chosen.** `BrowserShell::apply_pin_store_read` replaces the cache on a successful read, keeps it when there is no durable store, and keeps it (setting the axis) when the read is undeterminable.

**Why.** The store may never make the chrome say LESS. Dropping the cache would let a corrupt file SILENCE a change warning werust had already derived from a good read, which is precisely the missed-warning direction this whole spec exists to close, and it would hand an attacker who can corrupt the file a way to suppress the loudest state the chrome has. What the stale cache must NOT do is support a claim of absence, and it cannot: `can_bless_name` and `trust_pin_detail` both read the axis, so "no pin in the cache" never reaches the user as "you have not trusted this".

**Rejected.** Clearing the cache to a known-empty store (simpler to reason about, but it converts a corrupt file into a silenced warning); failing the load (the store is advisory, and the task forbids it).

**Touches.** `apply_pin_store_read` is the ONLY writer of both the cache and the axis, deliberately, so a future read site cannot update one and forget the other.

## 5. The write refusal lives in `save_to`, and is reported as the existing `false`

**Chosen.** `TrustedNamePins::save_to` re-reads the document it is about to replace and returns `false` without writing when that read is undeterminable. `BrowserShell::bless_current_name` ALSO returns early on an undeterminable read, before it mutates anything.

**Why.** In `save_to` it is structural: every writer inherits it, including the "forget this pin" action this store does not have yet, and including the RACE where the file goes bad after the navigation that read it, so the affordance is on screen when the user takes it. The shell's early return is not redundant with it: it is what makes the refusal VISIBLE (the axis is set and the chrome re-derived) rather than only a `false` the caller may ignore. `false` is the right report because it already means exactly "the bless holds for this session but could not be recorded" (the no-settings-directory case), and a store werust cannot read must not raise an error class that could break browsing.

**Rejected.** Refusing only at the shell (a second writer would silently reintroduce the destruction); a new error type out of `save_to` (a new refusal class at every call site, for a case the existing `false` already words correctly).

**Touches.** `save_to` now performs a READ before every write. The sibling atomic-write task (`trust-store-writes-atomically-and-keeps-fields-it-does-not-know`) rewrites this method and must keep the guard; the advisory-lock task (`trust-store-serialises-read-modify-write-so-no-bless-is-lost`) will want that read INSIDE its critical section rather than beside it.

## 6. A document with no `pins` array is undeterminable, not empty

**Chosen.** `{}`, `[]`, `{"pins":"nope"}` and an empty document all yield `UndeterminableTrust::Unparseable`. Only an ABSENT file (and an absent directory) is an empty store.

**Why.** The wire form always writes `{"pins":[…]}`, so a document without it was not written by this store: treating it as "no records" is exactly the "reads as nothing trusted" failure, one level up from a parse error. The fresh-install path that must stay silent is the missing FILE, which it does.

**Touches.** A hand-written `{}` placed there by a user now reads as undeterminable and blocks writes until removed. Deleting the file is the documented cure and is what a fresh install looks like anyway.
