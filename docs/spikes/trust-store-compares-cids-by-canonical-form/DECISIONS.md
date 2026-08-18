# Decisions: comparing trusted CIDs by canonical form (`trust-store-compares-cids-by-canonical-form`)

The third task of spec `trust-store-hardening`, after the third state (`docs/adr/0014`) and the atomic write (`docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md`). It removes a FALSE POSITIVE from the trusted-name change warning: a republish of byte-identical content under a different CID spelling, or the same site seen through an edge that canonicalises CIDs, reported "this changed since you trusted it".

The mechanism itself (parse with the vetted `cid` crate, compare the CIDv1 of the same multihash) is the obvious one and did not meet the ADR gate. Two edges of it are real choices, and both are recorded here plus as a module doc at the site (`crates/werust-core/src/pins.rs`, the CID-spelling note).

Task: `work/tasks/*/trust-store-compares-cids-by-canonical-form.md`. The false positive matters more than its size suggests because `withhold-changed-content-until-trusted` makes this warning decide what LOADS: a warning users are trained to click through is worse than no warning at all.

## 1. Canonicalisation happens at COMPARISON time; the store keeps the spelling it was given

**Chosen.** `TrustedNamePins::bless` records the CID string verbatim, `to_json` writes it verbatim, and the comparison (`same_content_root`, reached from `MutableNameTrust::is_changed` / `is_unchanged`) canonicalises BOTH sides on the way in to the comparison. Nothing on disk is rewritten, and there is no migration step.

**Why.** Canonicalising on the way IN could only ever fix pins written after it landed, so it would need this same comparison-time canonicalisation anyway for every pin already recorded, leaving the write-side normalisation as pure addition with no property of its own. Comparison-time is also the only version that makes a pin recorded by an OLDER build match the instant this build runs, which is the acceptance the whole spec exists to protect: nobody may lose a warning to this task, and nobody may lose a match either. And it keeps the store's existing promise to record what it was handed, the same care as the unknown-member carrying: a `cid` crate upgrade that changed a preferred spelling would otherwise silently rewrite every user's records on the next bless.

**Rejected.** Canonicalising in `bless` (fixes only new pins, needs the comparison anyway, and rewrites the user's records on a read-modify-write). A one-off migration pass that rewrites every entry to canonical form on load (a write triggered by a READ, in a store whose whole design says a read must not write, and it would be the first thing to run against a file the advisory-lock task has not yet serialised). Storing BOTH forms per entry (a second field to keep consistent, and it makes the document's meaning depend on which build wrote it).

**Touches.** Nothing on disk changes shape, so the next task in the chain (re-keying on the ENSIP-15-normalized NAME) migrates names only and never touches the `cid` member. A later reader that wants "the canonical CID" must call the comparison, not read the field and assume a form. `chrome_json`'s `blessedCid` and the banner's two CIDs still show the recorded spellings, deliberately: the user is being asked to compare what was recorded with what is live, and silently re-spelling either one is exactly the kind of invisible edit this store avoids.

## 2. A string that does not parse as a CID is compared LITERALLY, and never canonicalised

**Chosen.** `same_content_root(recorded, current)` is: the same string is the same root; otherwise both sides must parse and their CIDv1 forms must be equal; a string that does not parse is never canonicalised, so it is equal only to itself. Never a panic. Covered by `a_cid_werust_cannot_parse_is_compared_literally_and_never_canonicalised`.

**Why.** The rule has to fail in a stated direction, and the two candidate directions fail differently:

* *Unparseable is never equal to anything* (the strictly louder reading of the task's "two unparseable strings must not silently compare equal") means a name whose root werust cannot parse warns FOREVER, and re-blessing cannot clear it: the user accepts the change, the store records the same unparseable string, and the next load warns again. That is a permanent false positive that no user action can silence, which is the precise failure mode this task exists to remove, and it would also un-match every pin recorded under the old rule for such a root (a warning gained, but by breaking rule 1's promise that an existing record still matches).
* *Literal comparison* keeps today's behaviour EXACTLY for anything werust cannot read, so this change can only ever remove a false warning and never add or remove a true one.

The property the task actually needs is that CANONICALISATION cannot invent an equality, and literal comparison gives that in full: two DIFFERENT unreadable strings are never equal (no lowercasing, no trimming, no prefix match), and a real CID is never equal to an unreadable one in either position. What remains equal is a string to ITSELF, which is not a claim about CIDs at all.

It is worth saying how reachable this is: in production the CID reaches the store from `contenthash`'s decode (`Cid::to_string`) or from an `ipfs://` URL the core built from one, so an unparseable root is a hand-edited store or a hand-typed URL, not a live path. The rule is about never panicking and never surprising, not about a case werust expects to meet.

**Rejected.** Treating unparseable as `Undeterminable` (that is a STORE-WIDE state with a write refusal attached, `docs/adr/0014`; a single odd CID is not a reason to refuse every write, and `from_json` already rejects an entry with no cid at all). Lower-casing or trimming before comparing (a hand-rolled normalisation of exactly the kind the `cid` crate exists to prevent, and it WOULD invent equalities).

**Touches.** `same_content_root` is public because it is now the ONE place this comparison is made; the withhold-on-change work must call it rather than mint a second `==`. `MutableNameTrust::is_blessable` inherits the rule (a republish is no longer blessable, because there is nothing left to record), and `BrowserShell::bless_current_name` therefore answers `NothingToRecord` for a re-encoding instead of writing a second spelling of the same root.

## 3. The trust surface's duplicate `==` was folded into the same comparison

**Chosen.** `werust_core::trust_pin_detail` matched on `pin.cid == name.cid` to choose between "You trusted exactly this content on `<date>`" and "On `<date>` you trusted `<cid>` instead". It now asks `name.is_unchanged()`. No wording changed and no affordance changed.

**Why.** It is the same question the axis already answers, asked a second way, and leaving it would have shipped the exact incoherence this task is about: for a republish the badge and the banner would say nothing had changed while the trust popover said the user had trusted something else. The banner wording, the bless affordance's visibility and the loudest-wins posture rule are settled (`docs/adr/0006`) and are untouched here; only which sentence is selected changes, and only in the case that was wrong.

**Touches.** Both carriers (`desktop_paint::ChromePaint` and `chrome_json`) read that one derivation, so all five edges pick up the fix with no edge change. `chrome_json`'s `blessedCid` / `nameChanged` facts are unchanged in shape.
