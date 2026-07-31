---
title: review-gate non-blocking nits for 'windows-app-manifest-is-malformed-xml-and-breaks-the-release-link' (Gate 2 approve)
date: 2026-07-31
status: open
reviewOf: windows-app-manifest-is-malformed-xml-and-breaks-the-release-link
---

## Non-blocking review findings

The PR/code review gate (Gate 2) APPROVED 'windows-app-manifest-is-malformed-xml-and-breaks-the-release-link' but raised the
following non-blocking findings (nits). They do not block integration; this
is their durable home for triage — promote-to-task / keep / delete.

- The scanner silently ACCEPTS an unquoted attribute value, which mt.exe would refuse, so it can bless a malformed manifest. In xml_well_formedness_error the no-quote branch skips every char until the closing angle bracket, so a tag like assembly name=werust (no quotes) returns None. Worth closing, since the spike README claims the scanner refuses anything outside its vocabulary loudly rather than passing it silently.
  (crates/werust-core/tests/release_plumbing_shape.rs, start-tag loop, quote == None branch; the teeth sample named 'an unquoted-run-on attribute value' only exercises an UNTERMINATED quote, not a missing one)
- The scanner panics on any non-ASCII byte in CHARACTER DATA instead of reporting, i.e. a legal edit reds the gate with a slicing panic. The char-data branch evaluates src[i..].starts_with on a byte index while i advances one BYTE at a time, so a multibyte char inside the description element slices mid-character. Suggest a char_indices walk or an is_char_boundary check.
  (release_plumbing_shape.rs xml_well_formedness_error, first branch (bytes[i] != angle bracket). Today's manifest char data is ASCII so the gate is green; the non-ASCII section sign sits inside a comment, which is consumed wholesale)
- Ratify the recorded decision: a ~200-line hand-written scanner rather than quick-xml/roxmltree as a dev-dependency, plus the NEW hard refusal it introduces (any DOCTYPE or other bang-construct reds the gate by design). Both are recorded, so this is a ratification, not a defect. Note the repo already ships a DOCTYPE-bearing XML file (the Info.plist heredoc in crates/werust-macos/bundle-app.sh), so the scanner is deliberately not reusable there.
  (docs/spikes/windows-app-manifest-is-malformed-xml-and-breaks-the-release-link/README.md, section 'The guard, and why it is hand-written'; the task itself asked for the dependency-free option to be preferred)
- Acceptance criterion 3 (a dispatched release.yml run showing windows-desktop-app GREEN with its artifact, URL recorded) is NOT met, and cannot be met by the build agent (no push, so no remote ref). The conductor must dispatch after pushing and record the URL under the README's 'Dispatched run' heading. Note that expat well-formedness is necessary but not sufficient: mt.exe is the only thing that proves the link succeeds.
  (README section 'Re-verification by dispatch' states NOT YET OBTAINED and cites the CONTEXT.md:40 corollary that obtaining an unmeasured criterion is the conductor's job)
- Ratify an in-scope choice the task did not ask for: a 7-line EDITING THESE COMMENTS block was added at the top of app.manifest, i.e. prose added to a file that is embedded into every shipped Windows binary. It is recorded in the README and is good documentation-at-the-choice-site; confirm the human is happy with the manifest carrying it.
  (crates/werust-windows/app.manifest lines 5-10)
