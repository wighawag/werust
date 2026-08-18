//! The ENSIP-15 normalization CORPUS: a fixed table of names and the store keys
//! they must produce, and the dependency-bump TRIPWIRE that guards it (task
//! `trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`, spec
//! `trust-store-hardening`).
//!
//! # Why this file exists
//!
//! The trusted-name pin store keys every record on the ENSIP-15-NORMALIZED name
//! ([`pins::pin_key`](werust_core::pins::pin_key), reaching the ONE bound
//! `ens-normalize` call site). So the store's key space is a LIBRARY's output,
//! and a version of that library that changes ANY mapping silently re-keys every
//! affected record: the user's blessed names stop being found, the next visit
//! records fresh pins for names they already trusted, and nothing anywhere says
//! so. A dependency bump would be a trust reset.
//!
//! This table makes that bump RED instead. It is the same discipline this repo
//! already applies to a property of TEXT in a declarative file (`tests/
//! toolchain_pin_shape.rs`, `tests/verify_gate_shape.rs`: parse it, assert it,
//! never trust it), applied to a property of a LIBRARY's output.
//!
//! # What a maintainer does when this file REDS
//!
//! A red here is not a bug in this test. It means the mapping moved, and it is
//! the reviewable moment the bump exists for:
//!
//! 1. **Confirm the change is INTENDED.** Read the crate's changelog / the
//!    ENSIP-15 spec revision it tracks and satisfy yourself that the new output
//!    is the correct one. An unexplained mapping change is a reason to NOT bump.
//! 2. **Update the corpus AND the stamp in ONE deliberate change.** The expected
//!    values here and
//!    [`pins::NORMALIZATION_VERSION`](werust_core::pins::NORMALIZATION_VERSION)
//!    (the version stamped into every store this build writes) move together, in
//!    the same commit as the `ens-normalize` bump, so the file on disk keeps
//!    saying which normalization wrote it.
//! 3. **Decide whether STORED records need re-keying.** Every key in a user's
//!    `pins.json` that this change moves is a pin that will no longer be found.
//!    Re-keying is NOT automatic and is not this test's job: the read-time re-key
//!    (`trust-store-keys-on-the-resolved-normalized-name-and-migrates`) folds an
//!    old key onto today's derivation as the store is read, which handles a name
//!    whose NEW normalization this build can compute — and a store whose stamp
//!    names a version that is not this one is exactly where a maintainer looks to
//!    find out how many users are affected.
//!
//! # What this corpus does NOT claim
//!
//! Not that the library is CORRECT: these are the values `ens-normalize` returns
//! today, checked against what ENSIP-15 says they should be, and pinned so a
//! change to them is visible. Not that the list is exhaustive: it covers the
//! classes that bite a TOFU store (an invisible character, a confusable, a
//! fullwidth or circled spelling of plain Latin), because those are the ones
//! where two spellings of ONE identity could become two records. And it is not a
//! migration: the corpus reds, a human decides.
//!
//! The refusal cases assert only THAT the name is refused, never the normalizer's
//! wording: the detail string is the library's own sentence
//! ([`UnkeyableName::detail`](werust_core::pins::UnkeyableName)), and pinning
//! prose would red on a reworded message, which is noise, not a mapping change.

use werust_core::pins::{pin_key, NORMALIZATION_VERSION};

/// What the corpus expects of one input.
#[derive(Debug)]
enum Expected {
    /// ENSIP-15 accepts it, and THIS is the store key it produces.
    Key(&'static str),
    /// ENSIP-15 refuses it, so it has no store key: no pin, and no warning
    /// (`pin_key`'s own doc has the grounding). A name in this class fails
    /// resolution long before there is a page to bless.
    Refused,
}

/// One corpus row: the input, what it must produce, and WHY the row is here.
struct Case {
    input: &'static str,
    expected: Expected,
    why: &'static str,
}

/// The corpus. Inputs are written as ESCAPES with the code point named in the
/// `why`, because the whole point of half these rows is that the character is
/// invisible or indistinguishable in an editor.
const CORPUS: &[Case] = &[
    // ---- the boring case, so the table also documents what "normal" is -----
    Case {
        input: "ronan.eth",
        expected: Expected::Key("ronan.eth"),
        why: "plain ASCII: the key is the name, unchanged",
    },
    Case {
        input: "RONAN.ETH",
        expected: Expected::Key("ronan.eth"),
        why: "plain ASCII case: the one folding the OLD `to_lowercase` key also got right",
    },
    Case {
        input: " ronan.eth ",
        expected: Expected::Key("ronan.eth"),
        why: "the STORE's own trim around a typed name (pin_key's, not the normalizer's)",
    },
    // ---- emoji, with and without the U+FE0F variation selector ------------
    Case {
        input: "\u{2764}\u{FE0F}.eth",
        expected: Expected::Key("\u{2764}.eth"),
        why: "U+2764 HEAVY BLACK HEART + U+FE0F: normalization STRIPS the selector",
    },
    Case {
        input: "\u{2764}.eth",
        expected: Expected::Key("\u{2764}.eth"),
        why: "the same heart WITHOUT U+FE0F: the same key, so one identity is one record",
    },
    Case {
        input: "\u{1F600}\u{FE0F}.eth",
        expected: Expected::Key("\u{1F600}.eth"),
        why: "U+1F600 GRINNING FACE + U+FE0F: an emoji that is already emoji-presentation",
    },
    Case {
        input: "\u{1F600}.eth",
        expected: Expected::Key("\u{1F600}.eth"),
        why: "and without it: the pair a raw `to_lowercase` key split into two records",
    },
    Case {
        input: "ronan\u{FE0F}.eth",
        expected: Expected::Key("ronan.eth"),
        why: "U+FE0F after ASCII: the selector is dropped there too, not only after an emoji",
    },
    // ---- fullwidth and circled Latin: a spelling case folding does not fix -
    Case {
        input: "\u{FF32}\u{FF2F}\u{FF2E}\u{FF21}\u{FF2E}.eth",
        expected: Expected::Key("ronan.eth"),
        why: "FULLWIDTH LATIN CAPITAL R O N A N: `to_lowercase` maps it to fullwidth SMALL letters",
    },
    Case {
        input: "\u{24E1}\u{24DE}\u{24DD}\u{24D0}\u{24DD}.eth",
        expected: Expected::Key("ronan.eth"),
        why: "CIRCLED LATIN SMALL LETTER r o n a n: folded onto the plain letters",
    },
    Case {
        input: "\u{24C7}\u{24C4}\u{24C3}\u{24B6}\u{24C3}.eth",
        expected: Expected::Key("ronan.eth"),
        why: "CIRCLED LATIN CAPITAL R O N A N: circled AND cased, one key",
    },
    // ---- a mixed-script confusable ----------------------------------------
    Case {
        input: "r\u{043E}nan.eth",
        expected: Expected::Refused,
        why:
            "`ronan.eth` with a CYRILLIC SMALL LETTER O (U+043E): an illegal Latin+Cyrillic mixture",
    },
    Case {
        input: "\u{0440}\u{043E}\u{043D}\u{0430}\u{043D}.eth",
        expected: Expected::Key("\u{0440}\u{043E}\u{043D}\u{0430}\u{043D}.eth"),
        why:
            "the SAME letters all-Cyrillic: a single-script name is legal, and is its own identity",
    },
    // ---- invisible characters ---------------------------------------------
    Case {
        input: "ro\u{200B}nan.eth",
        expected: Expected::Key("ronan.eth"),
        why: "U+200B ZERO WIDTH SPACE: IGNORED, so an invisible variant keys onto the same record",
    },
    Case {
        input: "ro\u{00AD}nan.eth",
        expected: Expected::Key("ronan.eth"),
        why: "U+00AD SOFT HYPHEN: ignored the same way",
    },
    Case {
        input: "ro\u{200C}nan.eth",
        expected: Expected::Refused,
        why: "U+200C ZERO WIDTH NON-JOINER outside the sequences that allow it: disallowed",
    },
    Case {
        input: "ro\u{200D}nan.eth",
        expected: Expected::Refused,
        why: "U+200D ZERO WIDTH JOINER outside an emoji ZWJ sequence: disallowed",
    },
];

#[test]
fn the_normalization_corpus_still_maps_every_fixed_input_to_its_recorded_key() {
    // The tripwire itself. Every row is checked and the WHOLE drift is reported
    // at once, because a maintainer meeting this after a bump wants the full
    // list of mappings that moved, not the first one.
    let mut drift: Vec<String> = Vec::new();
    for Case {
        input,
        expected,
        why,
    } in CORPUS
    {
        let actual = pin_key(input);
        let agrees = match (expected, &actual) {
            (Expected::Key(key), Ok(actual)) => actual == key,
            (Expected::Refused, Err(_)) => true,
            _ => false,
        };
        if !agrees {
            drift.push(format!(
                "  {input:?} ({why})\n    corpus:   {expected:?}\n    library:  {actual:?}"
            ));
        }
    }
    assert!(
        drift.is_empty(),
        "the ENSIP-15 normalization the trusted-name pin store keys on has CHANGED, so every \
         stored pin under a moved key would silently stop being found.\n{drift}\n\nThis is the \
         dependency-bump tripwire (see this file's own docs): confirm the change is intended, \
         update this corpus AND `pins::NORMALIZATION_VERSION` (currently {NORMALIZATION_VERSION:?}) \
         in ONE deliberate change, and decide whether stored records need re-keying.",
        drift = drift.join("\n")
    );
}

#[test]
fn two_spellings_of_one_identity_are_one_key_and_two_identities_are_two() {
    // The corpus rows above assert each mapping; this asserts the PROPERTY the
    // store depends on, which is a relation between rows: spellings that mean
    // one name key onto one record, and names that merely LOOK alike do not.
    let key = |name: &str| pin_key(name).expect("a name werust can key");

    for (a, b, why) in [
        (
            "\u{2764}\u{FE0F}.eth",
            "\u{2764}.eth",
            "an emoji with and without U+FE0F",
        ),
        (
            "\u{FF32}\u{FF2F}\u{FF2E}\u{FF21}\u{FF2E}.eth",
            "ronan.eth",
            "fullwidth and plain Latin",
        ),
        (
            "\u{24E1}\u{24DE}\u{24DD}\u{24D0}\u{24DD}.eth",
            "RONAN.eth",
            "circled and plain, cased differently",
        ),
        (
            "ro\u{200B}nan.eth",
            "ronan.eth",
            "a name differing only by an invisible U+200B",
        ),
    ] {
        assert_eq!(
            key(a),
            key(b),
            "{why}: ONE identity must be ONE store key, or a blessed name reappears unblessed"
        );
    }

    assert_ne!(
        key("\u{0440}\u{043E}\u{043D}\u{0430}\u{043D}.eth"),
        key("ronan.eth"),
        "an all-Cyrillic homograph is a DIFFERENT name and must keep a different key: \
         collapsing it would let one site inherit another's bless"
    );
}

// ---------------------------------------------------------------------------
// The other half of the tripwire: the stamp must name the library that is
// actually linked.
// ---------------------------------------------------------------------------

/// The workspace root: this crate lives at `crates/werust-core` (same helper
/// shape as `tests/toolchain_pin_shape.rs`).
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("resolve workspace root from crates/werust-core")
}

fn read_repo_file(rel: &str) -> String {
    let path = repo_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The version of `ens-normalize` cargo actually RESOLVED, read from the
/// checked-in `Cargo.lock`.
///
/// The lock rather than the manifest, deliberately: the manifest carries a
/// REQUIREMENT (`"0.1.1"` is caret-`^0.1.1`), so a `cargo update` could link a
/// different mapping without the manifest changing a byte. The lock names the
/// crate that normalized.
fn locked_ens_normalize_version() -> String {
    let lock: toml::Table = read_repo_file("Cargo.lock")
        .parse()
        .expect("Cargo.lock must be valid TOML");
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .expect("Cargo.lock must hold a `[[package]]` array");
    let ens = packages
        .iter()
        .find(|p| p.get("name").and_then(toml::Value::as_str) == Some("ens-normalize"))
        .expect("`ens-normalize` must be in Cargo.lock: it is what normalizes every pin key");
    ens.get("version")
        .and_then(toml::Value::as_str)
        .expect("the locked `ens-normalize` must name a version")
        .to_string()
}

#[test]
fn the_recorded_normalization_version_names_the_library_that_is_actually_linked() {
    // A stamp that says one thing while another library normalizes is worse than
    // no stamp: it would tell a future maintainer that the store was written by a
    // normalization it was not. So the constant every store is stamped with is
    // checked against the LOCKED dependency, and a bump reds HERE too — the same
    // one deliberate change the corpus above asks for.
    let locked = locked_ens_normalize_version();
    assert_eq!(
        NORMALIZATION_VERSION,
        format!("ens-normalize {locked}"),
        "`pins::NORMALIZATION_VERSION` is stamped into every `pins.json` this build writes, so it \
         must name the `ens-normalize` version Cargo.lock resolves ({locked}). Bump both, plus the \
         corpus in this file, in ONE change."
    );
}

#[test]
fn the_manifest_pins_the_same_normalization_version_the_lock_resolves() {
    // The lock is what links, but the manifest is what a human EDITS, and a
    // requirement loose enough to float (`"0.1"`, `">=0.1"`) would let a routine
    // `cargo update` move every store key with no diff to read. Keeping the
    // requirement equal to the locked version is what makes the bump a
    // one-file, reviewable edit.
    let manifest: toml::Table = read_repo_file("crates/werust-core/Cargo.toml")
        .parse()
        .expect("werust-core's Cargo.toml must be valid TOML");
    let requirement = manifest
        .get("dependencies")
        .and_then(toml::Value::as_table)
        .expect("a `[dependencies]` table")
        .get("ens-normalize")
        .and_then(toml::Value::as_str)
        .expect("`ens-normalize` must be a plain version requirement")
        .to_string();
    assert_eq!(
        requirement.trim_start_matches('='),
        locked_ens_normalize_version(),
        "`crates/werust-core/Cargo.toml` must require the EXACT `ens-normalize` version the lock \
         resolves, so the normalization every trusted-name key derives from cannot move without a \
         deliberate edit"
    );
}
