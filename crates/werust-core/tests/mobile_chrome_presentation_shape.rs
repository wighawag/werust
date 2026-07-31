//! Mobile chrome-presentation wiring shape guard (task
//! `mobile-chrome-presentation-from-one-derivation`, `docs/adr/0011`).
//!
//! WHAT LANDED: the Kotlin and Swift chrome presentation is no longer
//! re-derived. `werust-core` decides (`status_line`, `trust_indicator` /
//! `_detail`, `error_banner_*`, `invalid_entry_badge_*`, `load_progress_*`),
//! [`werust_core::chrome_json`] CARRIES the result to both mobile edges on the
//! document they already decode every refresh, and each edge reads a FIELD where
//! it used to run a `when`/`switch`. The `ffi_json` chrome encoders that used to
//! sit in `werust-android/rust` and `werust-ios/rust`, byte-for-byte twins of
//! each other, are gone with it.
//!
//! WHY A SOURCE-SHAPE GUARD: the collapsed code is Kotlin and Swift, and this
//! repo's `verify` gate is pure Rust (`cargo fmt && clippy && build && test`, no
//! Android SDK, no Xcode). The DERIVATION half is pinned by real unit tests
//! (`the_chrome_json_carries_the_derivation_verbatim_for_every_chrome_shape` in
//! `crates/werust-core/src/lib.rs` drives every shape of `ChromeState` a rule can
//! branch on), but nothing in the gate can see whether the EDGES actually read
//! that carrier. A re-derived twin is exactly the failure mode that shipped
//! for months here: the trust EXPLANATION reached desktop only, and the
//! load-progress unit was a fraction in Rust and Swift but a percent in Kotlin.
//! So this test PARSES both edges and asserts the shape, in the same spirit as
//! the sibling guards `chrome_css_class_set_edge_wiring_shape.rs`,
//! `debug_view_mobile_wiring_shape.rs` and
//! `crates/werust-android/rust/tests/system_back_wiring_shape.rs`.
//!
//! Acceptance criteria mapped to assertions below:
//! - Neither edge re-derives: no string the core's presentation rules PRODUCE is
//!   written as a literal in either edge, and the old twin methods are gone
//!   (`no_mobile_edge_restates_a_string_the_core_derivation_produces`,
//!   `the_kotlin_and_swift_chrome_twins_are_gone`).
//! - Each edge DECODES the derived half of the carrier and PAINTS from it
//!   (`both_mobile_bindings_decode_every_derived_field`,
//!   `both_mobile_painters_paint_from_the_derived_fields`).
//! - The trust EXPLANATION exists on BOTH mobile platforms, surfaced in a
//!   platform-appropriate way (an accessibility description + a tap affordance,
//!   never a hover tooltip)
//!   (`both_mobile_edges_surface_the_trust_explanation`).
//! - One encoder, in the core: neither mobile crate carries a chrome-JSON twin
//!   (`the_chrome_json_is_encoded_once_in_the_core_not_per_mobile_crate`).

use std::path::{Path, PathBuf};

use renderer::{LoadState, TrustPosture};
use werust_core::pins::{MutableNameTrust, TrustedNamePin};
use werust_core::{
    invalid_entry_badge_text, load_progress_hint, trust_indicator, trust_indicator_detail,
    trust_pin_action_label, ChromeState, LoadStep,
};

/// Read a source file relative to the repo root. `CARGO_MANIFEST_DIR` is
/// `crates/werust-core`, so the root is two levels up.
fn source(relative: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

const KOTLIN_BINDING: &str =
    "crates/werust-android/app/src/main/java/com/github/wighawag/werust/WerustCore.kt";
const KOTLIN_PAINTER: &str =
    "crates/werust-android/app/src/main/java/com/github/wighawag/werust/BrowserActivity.kt";
const SWIFT_BINDING: &str = "crates/werust-ios/App/Sources/WerustCore.swift";
const SWIFT_PAINTER: &str = "crates/werust-ios/App/Sources/WKWebViewShellController.swift";

/// Every mobile source this guard covers: the two thin bindings that decode the
/// chrome carrier, and the two painters that assign it to widgets.
fn every_mobile_source() -> Vec<(&'static str, String)> {
    [KOTLIN_BINDING, KOTLIN_PAINTER, SWIFT_BINDING, SWIFT_PAINTER]
        .into_iter()
        .map(|path| (path, source(path)))
        .collect()
}

/// The FACT half of the carrier: the `ChromeState` fields, in the wire
/// vocabulary. Listed only so the forbidden-literal scan below can tell a JSON
/// KEY from a derived VALUE (`"loading"` is both a key here and the generic
/// phase name `load_progress_hint` falls back to).
///
/// Together with [`DERIVED_FIELDS`] this is the single list this guard drives, so
/// "the edge decodes it" and "the painter paints it" cannot silently cover
/// different subsets.
const FACT_FIELDS: &[&str] = &[
    "url",
    "loadState",
    "loading",
    "loadStep",
    "canGoBack",
    "canGoForward",
    "trustPosture",
    "error",
    "failureKind",
    "retryable",
    "invalidEntry",
    // The TOFU mutable-name axis (task `ipns-tofu-pin-and-warn-on-change`).
    "mutableName",
    "mutableNameCid",
    "blessedCid",
    "nameChanged",
];

/// The DERIVED half of the carrier.
const DERIVED_FIELDS: &[&str] = &[
    "statusLine",
    "trustIndicator",
    "trustIndicatorDetail",
    "errorBannerVisible",
    "errorBannerText",
    "invalidEntryBadgeVisible",
    "invalidEntryBadgeText",
    "loadProgressVisible",
    "loadProgressFraction",
    "loadProgressHint",
    // The trust surface's TOFU section: whether the bless is offered, what the
    // action says, and the body naming the name + CIDs.
    "trustPinActionVisible",
    "trustPinActionLabel",
    "trustPinDetail",
];

/// The source with COMMENTS removed and every STRING LITERAL collected.
///
/// Both halves matter and both need the same scanner:
///
/// * the literal scan is what proves no edge restates a derived string, and it
///   must look at literals ONLY: `loadingProgress` is an identifier that
///   contains the hint word "loading", and a doc comment that NAMES the removed
///   twin (as several now do) is documentation, not a re-derivation;
/// * the code scan is what proves the fields are really read, and it must ignore
///   comments for the mirror-image reason: a field named only in a comment is not
///   wired to anything.
///
/// Kotlin and Swift share enough lexical shape for one scanner: `//` line
/// comments, `/* */` block comments, `"` strings with backslash escapes, and
/// `"""` raw strings. A `//` inside a string (every `https://` URL here) is not a
/// comment, which is exactly why this is a scanner and not a regex.
fn scan(source: &str) -> (String, Vec<String>) {
    let bytes = source.as_bytes();
    let mut code = String::with_capacity(source.len());
    let mut literals = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let tail = &bytes[i..];
        if tail.starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if tail.starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if tail.starts_with(b"\"\"\"") {
            let start = i + 3;
            i = start;
            while i < bytes.len() && !bytes[i..].starts_with(b"\"\"\"") {
                i += 1;
            }
            literals.push(source[start..i.min(source.len())].to_string());
            i = (i + 3).min(bytes.len());
            continue;
        }
        if bytes[i] == b'"' {
            i += 1;
            let start = i;
            while i < bytes.len() && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            literals.push(source[start..i.min(source.len())].to_string());
            i = (i + 1).min(bytes.len());
            continue;
        }
        code.push_str(&source[i..i + 1]);
        i += 1;
    }
    (code, literals)
}

/// Every DISTINCT string the core's chrome presentation rules can PRODUCE, plus
/// the fixed fragments of the two rules that interpolate a reason.
///
/// Driven from the core itself over `TrustPosture::ALL` / `LoadStep::ALL` (each
/// kept complete by a compile-time check), so a FIFTH posture's badge and
/// explanation join this forbidden list automatically: an edge that hand-wrote
/// the new badge would red this guard without anyone remembering to extend a
/// literal list here.
fn every_derived_string() -> Vec<String> {
    let mut produced: Vec<String> = Vec::new();
    for posture in TrustPosture::ALL {
        for load_state in [LoadState::Idle, LoadState::Started] {
            let state = ChromeState {
                load_state,
                trust_posture: posture,
                ..ChromeState::default()
            };
            produced.push(trust_indicator(&state).to_string());
            produced.push(trust_indicator_detail(&state).to_string());
        }
    }
    for step in LoadStep::ALL {
        let state = ChromeState {
            load_state: LoadState::Started,
            load_step: step,
            ..ChromeState::default()
        };
        produced.push(load_progress_hint(&state).to_string());
    }
    // The TOFU mutable-name axis (task `ipns-tofu-pin-and-warn-on-change`): the
    // badge + explanation of a blessed-then-CHANGED name, and BOTH wordings of the
    // bless action. Driven from the core, like the postures above, so a reworded
    // action joins this forbidden list without anyone remembering to.
    let pin = TrustedNamePin {
        name: "ronan.eth".into(),
        cid: "bafyblessed".into(),
        blessed_at: 1_800_000_000,
        posture: TrustPosture::NameViaTrustedRpc,
    };
    for (cid, blessed) in [
        ("bafyblessed", None),
        ("bafychanged", Some(pin.clone())),
        ("bafyblessed", Some(pin)),
    ] {
        let state = ChromeState {
            load_state: LoadState::Finished,
            trust_posture: TrustPosture::NameViaTrustedRpc,
            mutable_name: Some(MutableNameTrust {
                name: "ronan.eth".into(),
                cid: cid.into(),
                blessed,
            }),
            ..ChromeState::default()
        };
        produced.push(trust_indicator(&state).to_string());
        produced.push(trust_indicator_detail(&state).to_string());
        produced.push(trust_pin_action_label(&state).to_string());
    }
    produced.push(
        invalid_entry_badge_text(&ChromeState {
            invalid_entry: Some("not a url".into()),
            ..ChromeState::default()
        })
        .to_string(),
    );
    // The two rules that interpolate a reason, by their FIXED fragments: the
    // framing wording is the rule, the reason is the fact it embeds.
    produced.push("failed: ".to_string());
    produced.push("loading\u{2026} \u{2014} ".to_string());
    produced.push("This page failed to load".to_string());
    produced.push("This page timed out".to_string());
    produced.retain(|text| !text.is_empty());
    produced.sort();
    produced.dedup();
    produced
}

#[test]
fn no_mobile_edge_restates_a_string_the_core_derivation_produces() {
    // THE property of the collapse: every string a mobile chrome SHOWS is the
    // core's, carried over the chrome JSON, so no mobile source needs to contain
    // one. A literal here is a twin in the making even when it agrees today:
    // that is precisely how iOS ended up with a build-time `"⛔ invalid URL"` that
    // was never refreshed from the rule, and how the trust EXPLANATION never
    // reached either mobile platform at all.
    let forbidden = every_derived_string();
    assert!(
        forbidden.len() >= 10,
        "the drive must really produce the derived strings; it produced {forbidden:?}"
    );
    for (path, text) in every_mobile_source() {
        let (_, literals) = scan(&text);
        for literal in &literals {
            // A carrier KEY is not a derived value, even when the two spell the
            // same word: `"loading"` is the fact's JSON key AND the generic phase
            // name `load_progress_hint` falls back to, and the edges must keep
            // naming the key to decode the document at all.
            if FACT_FIELDS.contains(&literal.as_str()) || DERIVED_FIELDS.contains(&literal.as_str())
            {
                continue;
            }
            for derived in &forbidden {
                assert!(
                    !literal.contains(derived.as_str()),
                    "{path} carries the string literal {literal:?}, which restates the core's own \
                     derivation ({derived:?}). The rule belongs in `werust-core`; the edge reads \
                     the carried field."
                );
            }
        }
    }
}

#[test]
fn the_kotlin_and_swift_chrome_twins_are_gone() {
    // The twins by NAME: each edge used to carry its own `statusLine()` /
    // `trustIndicator()` / `errorBanner()` / `invalidEntryBadge()` /
    // `loadProgress*()` implementation of the same rule set. They are FIELDS now,
    // so a declaration of any of them would be the rule coming back.
    let (kotlin, _) = scan(&source(KOTLIN_BINDING));
    for twin in [
        "fun statusLine()",
        "fun trustIndicator()",
        "fun loadStepHint()",
        "fun errorBanner()",
        "fun errorBannerVisible()",
        "fun errorIsRetryable()",
        "fun invalidEntryBadge()",
        "fun invalidEntryVisible()",
        "fun loadProgressVisible()",
        "fun loadProgressPercent()",
        "fun loadProgressHint()",
    ] {
        assert!(
            !kotlin.contains(twin),
            "`{twin}` is a re-derivation of a `werust-core` rule; the Kotlin edge must read the \
             carried field instead"
        );
    }
    // The Kotlin percent was the one UNIT fork among the three copies (0.25 in
    // Rust and Swift, 25 here), so the fraction must be what crosses.
    assert!(
        !kotlin.contains("loadProgressPercent"),
        "the load-progress unit is the core's FRACTION on every edge; the percent scale belongs \
         to the `ProgressBar` at the paint site"
    );

    let (swift, _) = scan(&source(SWIFT_BINDING));
    for twin in [
        "func statusLine()",
        "func trustIndicator()",
        "func loadStepHint()",
        "func errorBanner()",
        "func errorBannerVisible()",
        "func errorIsRetryable()",
        "func invalidEntryBadge()",
        "func invalidEntryVisible()",
        "func loadProgressVisible()",
        "func loadProgressFraction()",
        "func loadProgressHint()",
    ] {
        assert!(
            !swift.contains(twin),
            "`{twin}` is a re-derivation of a `werust-core` rule; the Swift edge must read the \
             carried field instead"
        );
    }
}

#[test]
fn both_mobile_bindings_decode_every_derived_field() {
    // The carrier is only collapsed if the edges actually DECODE it: a field the
    // core adds but an edge never reads is a field that edge still has to derive.
    for path in [KOTLIN_BINDING, SWIFT_BINDING] {
        let (code, literals) = scan(&source(path));
        for field in DERIVED_FIELDS {
            assert!(
                literals.iter().any(|literal| literal == field),
                "{path} must decode the carrier's `{field}` (the JSON key) from the chrome document"
            );
            assert!(
                code.contains(field),
                "{path} must bind the carrier's `{field}` to a property the painter can read"
            );
        }
    }
}

#[test]
fn both_mobile_painters_paint_from_the_derived_fields() {
    // And the painters must assign THOSE fields, not re-compute an equivalent.
    // Property reads (`chrome.statusLine`), never calls (`chrome.statusLine()`),
    // so a method sneaking back in is visible here too.
    for path in [KOTLIN_PAINTER, SWIFT_PAINTER] {
        let (code, _) = scan(&source(path));
        for field in DERIVED_FIELDS {
            let read = format!("chrome.{field}");
            assert!(
                code.contains(&read),
                "{path} must paint from the carried `{read}`"
            );
            assert!(
                !code.contains(&format!("{read}(")),
                "`{read}()` is a re-derivation at the edge; the carrier hands over a value"
            );
        }
    }
}

#[test]
fn both_mobile_edges_surface_the_trust_explanation() {
    // The gap this task closed: `trust_indicator_detail`, the sentence saying
    // what a posture MEANS, existed only on desktop, where it is the badge's
    // hover tooltip. Mobile has no hover, so each edge must surface it in a
    // platform-appropriate way: an ACCESSIBILITY description (the screen reader
    // reads the meaning, not the glyph) AND an explicit TAP affordance.
    let (kotlin, _) = scan(&source(KOTLIN_PAINTER));
    assert!(
        kotlin.contains("trust.contentDescription = chrome.trustIndicatorDetail"),
        "the Android trust badge must carry the core's explanation as its accessibility \
         description"
    );
    assert!(
        kotlin.contains("setOnClickListener { showTrustExplanation() }"),
        "the Android trust badge must be TAPPABLE, since there is no hover to show a tooltip on"
    );
    assert!(
        kotlin.contains("AlertDialog.Builder(this)"),
        "the Android tap must actually show the explanation"
    );

    let (swift, _) = scan(&source(SWIFT_PAINTER));
    assert!(
        swift.contains("trustLabel.accessibilityLabel = chrome.trustIndicatorDetail"),
        "the iOS trust badge must carry the core's explanation as its accessibility label"
    );
    assert!(
        swift.contains(
            "UITapGestureRecognizer(target: self, action: #selector(showTrustExplanation))"
        ),
        "the iOS trust badge must be TAPPABLE, since there is no hover to show a tooltip on"
    );
    assert!(
        swift.contains("UIAlertController("),
        "the iOS tap must actually show the explanation"
    );
}

#[test]
fn both_mobile_edges_offer_the_tofu_bless_from_the_trust_surface() {
    // Acceptance (task `ipns-tofu-pin-and-warn-on-change`, the settled UX): the
    // BLESS is an EXPLICIT user action reached FROM the trust indicator, never a
    // first-visit prompt. Mobile already opens a trust surface on a badge TAP
    // (the explanation alert), so the TOFU section belongs in THAT surface: the
    // core's `trust_pin_detail` line, plus the action shown exactly when the core
    // says `trust_pin_action_visible`, labelled with the core's own
    // `trust_pin_action_label` and dispatched into the SHARED
    // `BrowserShell::bless_current_name` over the FFI.
    //
    // A source-shape guard for the same reason its siblings are: the wiring is
    // Kotlin and Swift, which this repo's pure-Rust gate never compiles, and an
    // edge that decided for itself whether to offer the action (or minted its own
    // button wording) would be exactly the twin the collapse removed.
    let (kotlin, _) = scan(&source(KOTLIN_PAINTER));
    assert!(
        kotlin.contains("trustPinActionOffered = chrome.trustPinActionVisible")
            && kotlin.contains("trustPinActionLabelText = chrome.trustPinActionLabel")
            && kotlin.contains("trustPinDetailText = chrome.trustPinDetail"),
        "the Android trust surface must take all three TOFU values from the carrier"
    );
    assert!(
        kotlin.contains("if (trustPinActionOffered)"),
        "the Android bless action must be offered exactly when the CORE says so"
    );
    assert!(
        kotlin.contains("setNeutralButton(trustPinActionLabelText)"),
        "the Android bless button must wear the core's label, not one minted here"
    );
    assert!(
        kotlin.contains("driveCore { core.blessName() }"),
        "blessing writes a file, so it must go through the off-UI-thread dispatch \
         every session-driving action uses (the ANR guard)"
    );

    let (swift, _) = scan(&source(SWIFT_PAINTER));
    assert!(
        swift.contains("trustPinActionOffered = chrome.trustPinActionVisible")
            && swift.contains("trustPinActionLabelText = chrome.trustPinActionLabel")
            && swift.contains("trustPinDetailText = chrome.trustPinDetail"),
        "the iOS trust surface must take all three TOFU values from the carrier"
    );
    assert!(
        swift.contains("if trustPinActionOffered"),
        "the iOS bless action must be offered exactly when the CORE says so"
    );
    assert!(
        swift.contains("UIAlertAction(title: trustPinActionLabelText"),
        "the iOS bless button must wear the core's label, not one minted here"
    );
    assert!(
        swift.contains("self?.core.blessName()"),
        "the iOS bless must dispatch into the shared core, not a mobile pin store"
    );

    // And neither mobile crate carries a pin store of its own: both FFI entry
    // points call the one shared shell action.
    for path in [
        "crates/werust-android/rust/src/lib.rs",
        "crates/werust-ios/rust/src/lib.rs",
    ] {
        let (code, _) = scan(&source(path));
        assert!(
            code.contains("self.shell.bless_current_name()"),
            "{path} must bless through the shared shell"
        );
        assert!(
            !code.contains("TrustedNamePins"),
            "{path} must not open the pin store itself; the shell owns it"
        );
    }
}

#[test]
fn the_chrome_json_is_encoded_once_in_the_core_not_per_mobile_crate() {
    // The carrier itself was duplicated one level down: `werust-android/rust` and
    // `werust-ios/rust` each held an `ffi_json` module documented as the
    // "byte-for-byte twin" of the other, so adding a field meant adding it twice.
    // Both now call the core's ONE encoder.
    for path in [
        "crates/werust-android/rust/src/lib.rs",
        "crates/werust-ios/rust/src/lib.rs",
    ] {
        let (code, _) = scan(&source(path));
        assert!(
            code.contains("werust_core::chrome_json(self.shell.chrome())"),
            "{path} must encode the chrome through the core's one encoder"
        );
        assert!(
            !code.contains("mod ffi_json"),
            "{path} must not carry a second chrome encoder"
        );
    }
    for twin in [
        "crates/werust-android/rust/src/ffi_json.rs",
        "crates/werust-ios/rust/src/ffi_json.rs",
    ] {
        let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(twin);
        assert!(
            !path.exists(),
            "{twin} is a second copy of the chrome wire form; the core owns it now"
        );
    }
}

#[test]
fn the_scanner_reads_literals_and_code_apart() {
    // The guard ON the guard: every assertion above rests on `scan` telling
    // literals, code and comments apart. A scanner that treated `https://` as a
    // comment, or a doc comment as code, would make the literal check vacuous (it
    // would find nothing) or the wiring checks false-green (a field "read" only in
    // prose).
    let fixture = "\
// a comment mentioning \u{26a0} unverified origin
/* and a block one with \"a quoted phrase\" */
val url = \"https://example.com/\" // trailing comment
val badge = \"\u{26d4} invalid URL\"
val raw = \"\"\"a raw \" one\"\"\"
val identifier = loadingProgress
";
    let (code, literals) = scan(fixture);
    assert!(
        literals.contains(&"https://example.com/".to_string()),
        "a `//` inside a string is not a comment: {literals:?}"
    );
    assert!(
        literals.contains(&"\u{26d4} invalid URL".to_string()),
        "a plain literal is collected: {literals:?}"
    );
    assert!(
        literals.contains(&"a raw \" one".to_string()),
        "a raw literal is collected: {literals:?}"
    );
    assert!(
        !literals.iter().any(|l| l.contains("unverified origin")),
        "text inside a COMMENT is not a literal (documentation may name a removed twin): \
         {literals:?}"
    );
    assert!(
        !literals.iter().any(|l| l.contains("a quoted phrase")),
        "a quote inside a BLOCK comment is not a literal: {literals:?}"
    );
    assert!(
        code.contains("val identifier = loadingProgress"),
        "code outside comments and literals is kept: {code:?}"
    );
    assert!(
        !code.contains("trailing comment") && !code.contains("block one"),
        "comments are stripped from the code view: {code:?}"
    );
    assert!(
        !code.contains("example.com"),
        "literal CONTENT is not part of the code view, so an identifier check cannot be \
         satisfied by a string: {code:?}"
    );
}
