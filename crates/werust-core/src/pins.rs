//! Trust-on-first-use (TOFU) for MUTABLE names: the user BLESSES the CID a name
//! resolves to today, and werust WARNS when a later resolution returns a
//! DIFFERENT one: the SSH-host-key model applied to names.
//!
//! # Why this module exists
//!
//! `docs/adr/0006`'s second axis says a name is MUTABLE: an IPNS key holder can
//! publish a new record and an ENS owner can call `setContenthash`, so
//! content-verified bytes reached through a name are only "the bytes this name
//! points at right now". The chrome says so
//! ([`TrustPosture::MutableName`](renderer::TrustPosture::MutableName) /
//! [`NameViaTrustedRpc`](renderer::TrustPosture::NameViaTrustedRpc)), but "this
//! COULD change" is not actionable: it reads the same on the day the site is
//! genuine and on the day it is replaced. This module turns it into the
//! actionable "this CHANGED since you trusted it".
//!
//! # The three settled decisions this module implements
//!
//! 1. **The bless is EXPLICIT, never a first-visit prompt.** The user reaches it
//!    from the trust indicator (the surface that already explains the posture);
//!    the core's job here is only to say WHAT that surface shows and WHETHER the
//!    action is offered ([`crate::trust_pin_action_visible`],
//!    [`crate::trust_pin_action_label`], [`crate::trust_pin_detail`]).
//! 2. **The store is a `pins.json` NEXT TO `retrieval.json`**, reusing the
//!    [`retrieval`](crate::retrieval) settings mechanism VERBATIM: the same
//!    [`settings_dir`](crate::retrieval::settings_dir) resolution, the same
//!    [`WERUST_SETTINGS_DIR`](crate::retrieval::SETTINGS_DIR_ENV) lever, and the
//!    same directory-taking [`load_from`](TrustedNamePins::load_from) /
//!    [`save_to`](TrustedNamePins::save_to) cores so a test isolates the store
//!    into a scratch directory with NO process-global env mutation (the
//!    shared-write rule). Each pin records name -> CID **plus** the timestamp and
//!    the resolution POSTURE at bless time, so a later change can say which trust
//!    level the user was actually blessing.
//! 3. **BOTH IPNS and ENS names are blessable**, per the two-axis model: both are
//!    controller-repointable. See the MUTABILITY-AXIS note below for why that is
//!    deliberately WIDER than the displayed `MutableName` posture.
//!
//! # The mutability AXIS, not the `MutableName` POSTURE
//!
//! [`ChromeState::is_mutable_name`](crate::ChromeState::is_mutable_name) answers
//! "which badge is showing", which is the LOUDEST-wins display outcome: an ENS
//! name resolved over the Phase-1 trusted RPC shows `NameViaTrustedRpc` even
//! though it is also mutable, so the `MutableName` badge is currently never the
//! visible one for an ENS load at all. Blessability is the OTHER question ("can
//! the controller repoint this name?"), which `docs/adr/0006` answers YES for
//! every ENS name (`ipfs-ns` included: we cannot cheaply prove a name is locked)
//! and YES for every IPNS name. So a pin is offered for EVERY name-resolved load,
//! not only for one whose badge happens to read `mutable-name`. Reading the
//! posture instead would have silently made `ipfs-ns` ENS sites unblessable while
//! the ADR calls them mutable.
//!
//! # Fail-safe, and the THIRD state
//!
//! The pin store is ADVISORY and one-directional: it can only make werust say
//! MORE, never less. An unblessed name behaves exactly as it did before; a
//! blessed-and-unchanged name behaves exactly as it did before; a blessed-then-
//! CHANGED name adds a louder warning. Nothing here authorises a load, relaxes a
//! verification, or feeds the retrieval path, and a load NEVER fails because of
//! this file. The blessed CID is never used to CHOOSE what to load either: the
//! name still resolves normally and the bytes are still hash-verified, so a pin
//! can never cause unverified content to render.
//!
//! What a fail-safe store may NOT do is read as "nothing trusted" when it cannot
//! be read at all. "No records" (a fresh install: no settings directory, no
//! `pins.json`) and "cannot determine trust" (an unreadable file, a document that
//! is not this wire form, an entry werust cannot read honestly, two entries for
//! one name) are DIFFERENT facts, and the store distinguishes them
//! ([`UndeterminableTrust`], [`PinStoreRead`], task
//! `trust-store-fails-closed-instead-of-reading-as-nothing-trusted`,
//! `docs/adr/0014`). The asymmetry that ADR records, and the rule this module
//! implements, is:
//!
//! - **Readers get the STATE.** A missing file is still an EMPTY store (a fresh
//!   install is not an error), but an unreadable one yields
//!   [`UndeterminableTrust`] carrying WHY, so no caller can flatten it into
//!   "nothing blessed" by accident. A malformed entry is REPORTED rather than
//!   filtered out, so a future fifth [`TrustPosture`] cannot silently un-trust
//!   every name recorded under it.
//! - **Writers REFUSE while it holds** ([`save_to`](TrustedNamePins::save_to)).
//!   This is the half a naive fix forgets and the exploitable one: inserting into
//!   what merely LOOKS like an empty store is how ONE transient read failure
//!   permanently replaces every record with a single fresh one — the same
//!   destruction, from the write side.
//!
//! Neither half fails a LOAD: the chrome states that trust cannot be determined
//! and offers no bless (which the write would refuse anyway), and browsing is
//! untouched. The secondary shape choices (why the chrome carries it as its own
//! axis, why the shell keeps its cache while it holds) are recorded at
//! `docs/spikes/trust-store-fails-closed-instead-of-reading-as-nothing-trusted/DECISIONS.md`.
//!
//! # The WRITE: atomic, non-destructive, and honest about failing
//!
//! Three properties of the same [`save_to`](TrustedNamePins::save_to), all cheap
//! and all invisible until the day they matter (task
//! `trust-store-writes-atomically-and-keeps-fields-it-does-not-know`, decisions at
//! `docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md`):
//!
//! 1. **A reader never sees half a document.** The save writes a TEMP file in the
//!    SAME directory, flushes it, and RENAMES it onto `pins.json`, so a crash, an
//!    OOM kill or a full disk mid-write leaves the PREVIOUS document, not a
//!    truncated one. Same directory is load-bearing (a rename is atomic only
//!    within one filesystem), and the temp file is removed on the failure path.
//!    Atomicity is not mutual exclusion: two windows can still LOSE an update,
//!    which the advisory lock of
//!    `trust-store-serialises-read-modify-write-so-no-bless-is-lost` owns. This
//!    makes that loss clean instead of corrupt.
//! 2. **A field this build does not know is CARRIED, never stripped.** Two werust
//!    versions are two processes (the same fact the whole read-modify-write shape
//!    follows from), so an OLDER build re-writing the document must not delete
//!    what a NEWER one recorded. Unknown members are preserved at BOTH levels the
//!    document has: the document itself, and each entry, re-attached to the same
//!    pin on the way out. werust keeps authority over the fields it OWNS: the
//!    known members are written last, so a carried member can never shadow one,
//!    and the posture keeps the ONE shared wire spelling.
//! 3. **The outcome is a sentence, not a bit** ([`PinSaveOutcome`]). "There was
//!    nothing to record", "I could not write it" and "I refuse to write over a
//!    store I cannot read" are three different answers, and the last two are the
//!    ones worth showing someone. Nothing SHOWS them yet: there is no
//!    trust-management surface and building one is out of scope, so the taxonomy
//!    exists (and is propagated through the shell's bless path) so that surface is
//!    not blocked on a plumbing change when it arrives.
//!
//! # One content root, more than one CID spelling
//!
//! A CID is a self-describing name, and the SAME content root has more than one
//! legal spelling: a CIDv0 `Qm…` and its CIDv1 `bafybe…` are one root, and a
//! CIDv1 can be written in any multibase. werust really does meet both forms:
//! the ENSIP-7 decoder emits whatever form the name's contenthash carried
//! ([`crate::contenthash`]), while the Android edge canonicalises every CID to
//! lowercase base32 CIDv1 for its internal origin
//! (`crates/werust-android/rust/src/origin_map.rs`). So a raw string equality
//! reported "this changed since you trusted it" for a REPUBLISH OF IDENTICAL
//! CONTENT, or merely for the same site seen through another edge. False
//! positives are what train a user to click through the one warning that
//! matters, so the comparison goes through ONE canonical form
//! ([`same_content_root`], the vetted `cid` crate, never a hand-rolled
//! multibase). Two decisions the shape rests on (task
//! `trust-store-compares-cids-by-canonical-form`, recorded at
//! `docs/spikes/trust-store-compares-cids-by-canonical-form/DECISIONS.md`):
//!
//! 1. **Canonicalisation happens at COMPARISON time; the store keeps the
//!    spelling it was given.** A pin recorded by an earlier build therefore
//!    matches the moment this build runs, with no rewrite and no migration step,
//!    which matters because canonicalising on the way IN could only ever fix pins
//!    written after it landed, so it would need this comparison anyway. It also
//!    keeps the store's promise to record what it was handed, the same care the
//!    unknown-member note describes.
//! 2. **A string werust cannot parse as a CID is compared LITERALLY**, byte for
//!    byte, exactly as before. Canonicalisation can therefore never invent an
//!    equality between two spellings werust could not read, and a root werust
//!    cannot parse is not condemned to a warning that no re-bless could ever
//!    clear. Never a panic, whatever is in the file.
//!
//! # One IDENTITY, one key: the store keys on the RESOLVED name
//!
//! The key a pin is recorded under is the ENSIP-15-NORMALIZED name the
//! RESOLUTION produced ([`pin_key`],
//! [`ResolvedName::normalized_name`](crate::name_resolution::ResolvedName::normalized_name)),
//! not a fold this module computes from whatever string a caller was holding.
//! The namehash normalizes the name already and used to throw the result away,
//! while the store keyed on `trim().to_lowercase()`: an ASCII case fold. For
//! every name whose normalization is NOT plain case folding (an emoji with and
//! without its U+FE0F variation selector, fullwidth or circled Latin, other
//! confusables) that made ONE resolved identity into TWO keys, which is exactly
//! the missed warning this store exists to raise (task
//! `trust-store-keys-on-the-resolved-normalized-name-and-migrates`). Two rules
//! hold the shape together:
//!
//! 1. **One derivation, one call site.** [`pin_key`] reaches the SAME
//!    [`ens::normalize_name`](crate::ens::normalize_name) the namehash does, and
//!    the shell keys the store on the value the resolution RETURNED rather than
//!    re-deriving one from the typed spelling. A second derivation of an
//!    identity is a second chance to drift, and a store whose key disagrees with
//!    the resolved identity warns about nothing.
//! 2. **The key is FALLIBLE, and never falls back.** A name normalization
//!    refuses has no key, no pin and no warning, said out loud as an
//!    [`UnkeyableName`] ([`pin_key`]'s own doc has the grounding). A fallback to
//!    the old fold would be the second key space this change exists to delete.
//!
//! A NON-ENS name needs no special case: a bare IPNS key (`k51qzi…`, base36) is
//! a lower-case ASCII dot-less label, so the same normalization returns it
//! unchanged, and a test asserts that rather than prose.
//!
//! # Re-keying what is already recorded
//!
//! A record written under the OLD fold would MISS after that change, and the
//! next bless would write a SECOND record for a name the user already trusts. So
//! every entry is re-keyed AS IT IS READ (`read_entry`), which makes the
//! migration part of the one thing every caller already does. It is in place and
//! idempotent (re-keying an already-normalized key returns it, so a second
//! launch re-keys nothing), it inherits the write rules whole because it reaches
//! disk only through [`save_to`](TrustedNamePins::save_to) (refused while the
//! store is unreadable, atomic when it lands), and it needs no separate startup
//! rewrite: a store nobody ever writes again is still read correctly forever.
//! Two old records that collapse onto ONE new key are the very bug being fixed,
//! so they are REPORTED through the existing
//! [`DuplicateName`](UndeterminableTrust::DuplicateName) rule — never merged by
//! picking whichever sorted first, which would decide for the user which content
//! they had trusted.
//!
//! # Which NORMALIZATION wrote this store: the stamp, and the corpus
//!
//! A key derived from a LIBRARY is only as stable as that library: a version of
//! `ens-normalize` that changes any mapping re-keys every affected record, the
//! user's blessed names stop being found, and the next visit records fresh pins
//! for names they already trusted. A dependency bump would be a trust reset, and
//! nothing would say so. Two mechanisms make it loud instead (task
//! `trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp`,
//! decisions at
//! `docs/spikes/trust-store-pins-normalization-stability-with-a-corpus-and-a-stamp/DECISIONS.md`):
//!
//! 1. **A fixed corpus REDS the gate** (`crates/werust-core/tests/normalization_corpus.rs`):
//!    a checked-in table of names and the keys [`pin_key`] must produce for them,
//!    covering the classes that bite a TOFU store (an emoji with and without
//!    U+FE0F, fullwidth and circled Latin, a mixed-script confusable, an
//!    invisible-character pair, plain ASCII). Bumping the crate without updating
//!    the table fails the build, which is the reviewable moment the bump needs.
//!    That file also carries the maintainer's procedure for a red.
//! 2. **The document RECORDS which normalization wrote it**
//!    ([`NORMALIZATION_VERSION`], the `normalizationVersion` member,
//!    [`TrustedNamePins::normalization_version`]), so a change is DETECTABLE from
//!    the file rather than inferred from a version somebody remembers running.
//!
//! What the two claim is deliberately narrow: the corpus proves the mapping is
//! what it was when the corpus was written, and the stamp proves which
//! normalization last WROTE the file. Neither claims the library is correct, and
//! neither is a migration.
//!
//! A store with NO stamp is not an error and not a mismatch: it was written
//! before stamping existed, and it loads exactly as it always did. The stamp is
//! an ordinary top-level document member, which is what makes an OLDER build
//! preserve it: to that build it is simply a member it does not know, carried by
//! the unknown-member rule above.
//!
//! **What a MISMATCH means, and what it does.** A stamp naming a version that is
//! not this build's says exactly one thing: *the records in this file were keyed
//! by a different normalization, so a key this build derives may not be the key
//! that is stored.* It is RECORDED and READABLE
//! ([`normalization_version_mismatch`](TrustedNamePins::normalization_version_mismatch))
//! and NOTHING acts on it — it does not make the store undeterminable, does not
//! block a write, does not re-key, and does not change what the chrome says. That
//! is the honest minimum: acting on it silently is the trust reset this whole
//! mechanism exists to prevent, and the read-time re-key already folds an old key
//! onto today's derivation for every name this build CAN normalize. A surface
//! that eventually reports it (or a migration that acts on it) inherits a fact it
//! can read, rather than a version it has to guess.
//!
//! # Vocabulary note: "pin"
//!
//! `pin` is already used loosely in this crate for "held in place" (the shell
//! PINS a `.eth` name in the URL bar, a `pinned_root_key`, a pinned record
//! source). The TOFU sense is a DIFFERENT, durable thing, so it is always spelled
//! out as a **trusted name pin** ([`TrustedNamePin`], [`TrustedNamePins`],
//! `pins.json`) and the verb for creating one is **bless**, never "pin". The
//! spelling is the settled decision 2's (`pins.json`); the discipline is so the
//! two senses cannot be confused at a call site.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use fetcher::Cid;
use renderer::TrustPosture;

use crate::debug::{trust_posture_from_wire_name, trust_posture_wire_name};

/// The pin-store file name, under the SAME settings directory
/// [`retrieval::settings_dir`](crate::retrieval::settings_dir) resolves (settled
/// decision 2: `pins.json` lives NEXT TO `retrieval.json`, one mechanism, one
/// `WERUST_SETTINGS_DIR` lever, not a second location).
pub const PINS_FILE: &str = "pins.json";

/// The NORMALIZATION this build keys pins with: the bound `ens-normalize` crate
/// and the version of it Cargo.lock resolves — the value stamped into every
/// document [`TrustedNamePins::save_to`] writes.
///
/// It names a LIBRARY and a version rather than an abstract "ENSIP-15 revision",
/// because the library is what actually decides a key: two crates claiming the
/// same spec revision can still disagree, and the question this stamp answers is
/// "which code produced the keys in this file?".
///
/// It is kept honest by `crates/werust-core/tests/normalization_corpus.rs`, which
/// reds if it stops naming the version the lockfile resolves — so bumping the
/// dependency, the corpus and this constant is ONE deliberate change rather than
/// three chances to forget one.
pub const NORMALIZATION_VERSION: &str = "ens-normalize 0.1.1";

/// The document member [`NORMALIZATION_VERSION`] is recorded under.
///
/// A plain top-level string member, deliberately: that is exactly the shape the
/// unknown-member rule carries, so a build that does not know this member (every
/// werust before it existed) preserves it through a read-modify-write instead of
/// stripping it.
const NORMALIZATION_VERSION_MEMBER: &str = "normalizationVersion";

// ---------------------------------------------------------------------------
// The pin value.
// ---------------------------------------------------------------------------

/// One trust-on-first-use pin: the CID a mutable NAME resolved to at the moment
/// the user blessed it, with when they did and what werust was claiming then.
///
/// The posture is recorded (not just the CID) because a later change must be able
/// to say which trust level the user was actually blessing: "you trusted this
/// while werust was telling you the name came over a trusted RPC" is a materially
/// different sentence from "you trusted this while werust could verify the record
/// itself".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedNamePin {
    /// The mutable name, in the store's canonical key form: the
    /// ENSIP-15-normalized name the resolution produced; see [`pin_key`].
    pub name: String,
    /// The CID the name resolved to when it was blessed, VERBATIM in the form
    /// that resolution produced (a CIDv0 `Qm…` or a CIDv1 `bafybe…`): the store
    /// records what it was given and canonicalises only to COMPARE
    /// ([`same_content_root`]).
    pub cid: String,
    /// When it was blessed, in whole seconds since the Unix epoch (UTC).
    pub blessed_at: u64,
    /// The [`TrustPosture`] werust was showing for that load at bless time.
    pub posture: TrustPosture,
}

impl TrustedNamePin {
    /// The calendar day this pin was blessed on (UTC, `YYYY-MM-DD`), the `<date>`
    /// the change warning quotes back to the user.
    #[must_use]
    pub fn blessed_on(&self) -> String {
        format_utc_date(self.blessed_at)
    }
}

// ---------------------------------------------------------------------------
// The CID comparison: one content root, more than one spelling.
// ---------------------------------------------------------------------------

/// Whether two CID strings name the SAME content root: the comparison the whole
/// change warning rests on ([`MutableNameTrust::is_changed`] /
/// [`is_unchanged`](MutableNameTrust::is_unchanged)).
///
/// It is deliberately NOT `==`. One content root has more than one legal
/// spelling, and werust meets more than one of them (the module's CID-spelling
/// note): the ENSIP-7 decoder emits whatever form the contenthash carried, and
/// the Android edge hands the core the lowercase base32 CIDv1 of the same root.
/// Comparing strings therefore warned about a REPUBLISH OF IDENTICAL CONTENT.
///
/// The rule, in the order it applies:
///
/// 1. **The same string is the same root.** This is the pre-existing rule, kept
///    whole, so nothing that matched before this change stops matching, and so a
///    root werust cannot parse still compares as it always did.
/// 2. **Otherwise, both sides must PARSE, and their canonical forms must match.**
///    Canonical here is the CIDv1 of the same multihash and codec
///    ([`canonical_root`]), which is spelling-independent (any multibase) and
///    version-independent (a CIDv0 `Qm…` and its `bafybe…`), while a genuinely
///    different root (different bytes, or the same bytes addressed under a
///    different codec) stays different.
/// 3. **A string that does not parse is never canonicalised**, so two spellings
///    werust could not read are equal only when they are literally the same
///    string, and a real CID is never equal to an unreadable one.
///
/// Public because it is the ONE place this comparison is made: a later reader of
/// the store (the withhold-on-change work) must reuse it rather than mint a
/// second `==` that reintroduces the false positive.
#[must_use]
pub fn same_content_root(recorded: &str, current: &str) -> bool {
    if recorded == current {
        return true;
    }
    match (canonical_root(recorded), canonical_root(current)) {
        (Some(recorded), Some(current)) => recorded == current,
        // Not a CID werust can read: rule 3. Never a panic, and never an
        // equality canonicalisation invented.
        _ => false,
    }
}

/// ONE canonical form for a CID string, its CIDv1, or `None` when the string is
/// not a CID werust can read.
///
/// The parse and the conversion are the vetted `cid` crate's (the SAME `cid 0.11`
/// lineage the `fetcher` verify boundary and the Android origin map use, reached
/// through [`fetcher::Cid`] so this module cannot drift onto a second one): a
/// multibase decode is exactly the kind of thing this project binds rather than
/// hand-rolls (`docs/adr/0001`). Comparing the PARSED values rather than
/// re-rendered strings is what makes every multibase spelling of one CIDv1 equal
/// for free.
///
/// A CIDv0 converts to the CIDv1 of the same dag-pb sha2-256 multihash, which is
/// the identity ENSIP-7 and the Android edge disagree about the spelling of.
fn canonical_root(cid: &str) -> Option<Cid> {
    Cid::try_from(cid).ok()?.into_v1().ok()
}

/// The MUTABLE-NAME identity of the page currently shown, paired with whatever
/// the user has blessed for that name.
///
/// This is the CHROME's view of the TOFU state (the orthogonal axis
/// [`ChromeState::mutable_name`](crate::ChromeState::mutable_name) carries), and
/// deliberately NOT the store: the shell reads the store once per load and hands
/// the chrome a plain value, so every presentation rule is a pure function of
/// [`ChromeState`](crate::ChromeState) with no filesystem in the paint path.
///
/// `None` on the [`ChromeState`](crate::ChromeState) means the current page is
/// not a name-resolved load at all (a direct `ipfs://<cid>`, an ordinary
/// `https://` page, a failed load): nothing to bless, nothing to warn about.
///
/// The two CIDs here are compared by [`same_content_root`], never by `==`: the
/// blessed one and the live one can be two spellings of ONE root (see the
/// module's CID-spelling note).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutableNameTrust {
    /// The mutable name this site is trusted UNDER: the ROOT name (e.g.
    /// `ronan.eth`, never a sub-path display) in its canonical [`pin_key`] form,
    /// so `ronan.eth/blog/` and `ronan.eth` share one pin — and so do two
    /// spellings of one identity that normalize together.
    pub name: String,
    /// The ROOT CID this name resolves to on THIS load.
    pub cid: String,
    /// The pin on file for [`name`](MutableNameTrust::name), or `None` when the
    /// user has never blessed it (in which case werust behaves exactly as it did
    /// before this module existed).
    pub blessed: Option<TrustedNamePin>,
}

impl MutableNameTrust {
    /// Whether the user has blessed this name at all.
    #[must_use]
    pub fn is_blessed(&self) -> bool {
        self.blessed.is_some()
    }

    /// The TOFU warning condition: the name IS blessed, and it now resolves to a
    /// DIFFERENT CID than the blessed one.
    ///
    /// Strictly stronger than the plain `MutableName` / `NameViaTrustedRpc`
    /// warnings and never flattened into either (settled decision 3): those say
    /// the name *could* change, this says it *did*.
    #[must_use]
    pub fn is_changed(&self) -> bool {
        self.blessed
            .as_ref()
            .is_some_and(|pin| !same_content_root(&pin.cid, &self.cid))
    }

    /// Whether the name is blessed AND still resolves to the blessed CID.
    ///
    /// "The same CID" means the same CONTENT ROOT, not the same string: a
    /// republish of identical content under another CID spelling is unchanged
    /// (see [`same_content_root`] and the module's CID-spelling note).
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        self.blessed
            .as_ref()
            .is_some_and(|pin| same_content_root(&pin.cid, &self.cid))
    }

    /// Whether blessing would record something NEW: either the name has no pin
    /// yet (first use), or it has one that no longer matches (the user has looked
    /// at the change and decided to accept the new content).
    ///
    /// This is what makes the action a TOFU bless rather than a no-op button: a
    /// name already blessed to exactly this CID has nothing left to record.
    ///
    /// Says nothing about whether the store could be READ, which is a store-wide
    /// fact rather than a per-name one
    /// ([`ChromeState::trust_undeterminable`](crate::ChromeState::trust_undeterminable)).
    /// [`ChromeState::can_bless_name`](crate::ChromeState::can_bless_name) is the
    /// gate that combines the two, and it is what an edge paints its button from.
    #[must_use]
    pub fn is_blessable(&self) -> bool {
        !self.is_unchanged()
    }

    /// The calendar day the current pin was blessed on, or `None` when unblessed.
    #[must_use]
    pub fn blessed_on(&self) -> Option<String> {
        self.blessed.as_ref().map(TrustedNamePin::blessed_on)
    }
}

// ---------------------------------------------------------------------------
// The THIRD state: "cannot determine trust".
// ---------------------------------------------------------------------------

/// WHY werust cannot determine what the user has blessed: the store's THIRD
/// state, distinct from "no records" and never flattened into it.
///
/// A fresh install has NO records, which is a fact werust knows. This type is the
/// other thing: werust cannot tell, and says so. It carries a legible reason
/// because "your trust store is unreadable" with no WHICH is not actionable, and
/// because the reason is what the trust surface shows the user
/// ([`crate::trust_pin_detail`]).
///
/// While a read yields one of these, the WRITE side refuses
/// ([`TrustedNamePins::save_to`]) and the chrome offers no bless: the two halves
/// of one rule (`docs/adr/0014`), because a reader that merely NOTICES the state
/// while the writer overwrites the file anyway loses every record it could not
/// read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndeterminableTrust {
    /// `pins.json` exists but could not be READ (a permission, an I/O error, a
    /// non-UTF-8 document). A MISSING file is NOT this: it is an empty store.
    Unreadable(String),
    /// The document is not this store's wire form at all (not JSON, or no `pins`
    /// array): werust cannot tell whether it holds one record or a hundred.
    Unparseable(String),
    /// One ENTRY cannot be read honestly: no name, no CID, no bless timestamp, or
    /// a [`TrustPosture`] spelling this build does not know (a store written by a
    /// LATER werust). Reported rather than dropped, because the next save would
    /// persist the survivors and make the drop permanent.
    UnreadableEntry(String),
    /// Two entries record the SAME name. Silently keeping one of them lets a
    /// hand-edited or half-merged file choose for the user.
    DuplicateName(String),
}

impl std::fmt::Display for UndeterminableTrust {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(detail) => write!(f, "{PINS_FILE} could not be read: {detail}"),
            Self::Unparseable(detail) => write!(
                f,
                "{PINS_FILE} is not a trusted-name store werust can read: {detail}"
            ),
            Self::UnreadableEntry(detail) => write!(
                f,
                "{PINS_FILE} records something werust cannot read honestly: {detail}"
            ),
            Self::DuplicateName(name) => {
                write!(f, "{PINS_FILE} records two trusted-name pins for `{name}`")
            }
        }
    }
}

/// The outcome of READING the trusted-name pin store: the three answers a caller
/// must tell apart.
///
/// [`NoStore`](PinStoreRead::NoStore) and
/// [`Undeterminable`](PinStoreRead::Undeterminable) are BOTH distinct from an
/// empty [`Pins`](PinStoreRead::Pins): "there is nowhere to read from" means a
/// caller's in-memory pins are still the truth (no file could have superseded
/// them), "cannot determine trust" means werust must neither claim a name is
/// unblessed nor write, and an EMPTY store means the user has genuinely blessed
/// nothing yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinStoreRead {
    /// There is nowhere durable to read from at all (no settings directory on this
    /// system, or a caller that asked for no store). NOT an empty store.
    NoStore,
    /// The store as recorded. An ABSENT file is an empty one: a fresh install.
    Pins(TrustedNamePins),
    /// werust cannot determine what is blessed, and will not write until it can.
    Undeterminable(UndeterminableTrust),
}

impl From<Result<TrustedNamePins, UndeterminableTrust>> for PinStoreRead {
    fn from(read: Result<TrustedNamePins, UndeterminableTrust>) -> Self {
        match read {
            Ok(pins) => Self::Pins(pins),
            Err(why) => Self::Undeterminable(why),
        }
    }
}

// ---------------------------------------------------------------------------
// The WRITE outcome.
// ---------------------------------------------------------------------------

/// The outcome of WRITING the trusted-name pin store: the read side's
/// [`PinStoreRead`] seen from the other direction, and the answer
/// [`save_to`](TrustedNamePins::save_to) and
/// [`BrowserShell::bless_current_name`](crate::BrowserShell::bless_current_name)
/// both speak.
///
/// It is an OUTCOME, never an error: a store that cannot be written must not
/// break browsing, and a bless that could not be recorded still holds for THIS
/// session (the chrome updates; only a relaunch loses it). What it is NOT is a
/// bare boolean, because "there was nothing to record", "I could not write it"
/// and "I refuse to write over a store I cannot read" are three different
/// sentences and a `false` says none of them. Only the last two are worth
/// showing anyone, which is what [`problem`](PinSaveOutcome::problem) answers.
///
/// werust deliberately builds no user-facing surface for this yet (there is no
/// trust-management UI at all, and `docs/adr/0014` is why the bless affordance
/// withdraws instead): the taxonomy exists so the surface that eventually shows
/// it is not blocked on a plumbing change. Decisions:
/// `docs/spikes/trust-store-writes-atomically-and-keeps-fields-it-does-not-know/DECISIONS.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinSaveOutcome {
    /// The document is on disk: the only success, and the only outcome that
    /// survives a relaunch.
    Recorded,
    /// There was nothing to record in the first place (no mutable name on this
    /// page, or a name already blessed at exactly this CID). The CALLER's answer,
    /// never [`save_to`](TrustedNamePins::save_to)'s, and the one uninteresting
    /// non-success: nothing was lost, because nothing was pending.
    NothingToRecord,
    /// There was something to record and it did not reach disk: no settings
    /// directory on this system, no durable store on this shell, a full disk, a
    /// permission error, an interrupted write. Carries a legible reason.
    ///
    /// NOT a policy decision: werust WOULD have written it. That distinction is
    /// the whole reason [`Refused`](PinSaveOutcome::Refused) is a separate
    /// variant.
    CouldNotPersist(String),
    /// werust REFUSED to write, because the store on disk cannot be READ
    /// (`docs/adr/0014`): overwriting it is how ONE transient failure permanently
    /// replaces every record with a single fresh one. Carries the same
    /// [`UndeterminableTrust`] the read side reports, so a caller says WHY without
    /// re-reading the file.
    Refused(UndeterminableTrust),
    /// The NAME has no store key at all ([`pin_key`]), so there is nothing that
    /// could be recorded for it and nothing was written.
    ///
    /// A refusal, like [`Refused`](PinSaveOutcome::Refused), and deliberately not
    /// [`CouldNotPersist`](PinSaveOutcome::CouldNotPersist) (werust would NOT
    /// have written this) nor
    /// [`NothingToRecord`](PinSaveOutcome::NothingToRecord) (there WAS something
    /// the user asked to record). Unreachable through the browser — an
    /// unnormalizable name fails resolution before a page exists to bless — so it
    /// exists to make sure the write path has no silent "cannot happen" branch,
    /// which in a TOFU store is a lost warning.
    Unkeyable(UnkeyableName),
}

impl PinSaveOutcome {
    /// Whether the pins reached DISK: the old boolean, for a caller that only
    /// needs "will this survive a relaunch?" (the mobile FFI boundaries, which
    /// hand a `bool` to Kotlin and Swift, are exactly that caller).
    #[must_use]
    pub fn is_recorded(&self) -> bool {
        matches!(self, Self::Recorded)
    }

    /// The sentence worth putting in front of a user, or `None` when there is
    /// nothing to say (it was recorded, or there was nothing to record).
    ///
    /// The ONE place the two interesting outcomes are put into words, so a future
    /// surface cannot mint a second wording, the same discipline the chrome's
    /// derived strings follow (`docs/adr/0011`).
    #[must_use]
    pub fn problem(&self) -> Option<String> {
        match self {
            Self::Recorded | Self::NothingToRecord => None,
            Self::CouldNotPersist(detail) => {
                Some(format!("{PINS_FILE} could not be written: {detail}"))
            }
            Self::Refused(why) => Some(format!(
                "nothing was written, because {why}, so the trusted names already recorded \
                 there are not lost"
            )),
            Self::Unkeyable(why) => Some(format!("nothing was written, because {why}")),
        }
    }
}

// ---------------------------------------------------------------------------
// The store.
// ---------------------------------------------------------------------------

/// A name that has NO store key, because ENSIP-15 normalization refuses it: the
/// legible half of the fallible key (see [`pin_key`]).
///
/// It exists so a refusal is a SENTENCE at the call site rather than a bare
/// `None` a later caller reads as "the user has not blessed this" — the one
/// reading that would turn "werust cannot key this name" into a missing warning.
/// It is the store's statement of the same fact
/// [`ResolutionError::UnnormalizableName`](crate::ens::ResolutionError::UnnormalizableName)
/// reports on the resolution path, and it carries that normalizer's own reason
/// rather than wording a second one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnkeyableName {
    /// The name as it was handed to [`pin_key`] (trimmed).
    pub name: String,
    /// The normalizer's own reason.
    pub detail: String,
}

impl std::fmt::Display for UnkeyableName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self { name, detail } = self;
        write!(
            f,
            "`{name}` is not a name werust can normalize, so it has no trusted-name key: {detail}"
        )
    }
}

/// The canonical store key for a mutable name: its ENSIP-15-NORMALIZED form —
/// the SAME identity the resolution path produced
/// ([`ResolvedName::normalized_name`](crate::name_resolution::ResolvedName::normalized_name)).
///
/// # Why normalization and not case folding
///
/// The key used to be `trim().to_lowercase()`, an ASCII case fold, while the
/// resolution normalized the very same name through ENSIP-15 (inside the
/// namehash) and discarded the result. Normalization is much more than case: it
/// folds fullwidth and circled Latin onto plain letters and strips an emoji's
/// invisible U+FE0F variation selector, so for any name whose normalization is
/// not plain case folding ONE resolved identity produced TWO store keys — the
/// user blesses a name, sees it again under its other spelling, and werust says
/// nothing. That is the one failure mode a TOFU store cannot have, and this
/// module's own doc used to claim a casing guard that covered ASCII only (task
/// `trust-store-keys-on-the-resolved-normalized-name-and-migrates`).
///
/// The derivation lives HERE, in one place, reaching the ONE bound-normalizer
/// call site ([`ens::normalize_name`](crate::ens::normalize_name)). One place is
/// load-bearing twice over: the key and the resolved identity cannot drift, and
/// the follow-on that records WHICH normalization version wrote a store has a
/// single site to stamp.
///
/// # The key is FALLIBLE
///
/// A name ENSIP-15 refuses has NO key: no pin, and therefore no warning. It does
/// NOT fall back to the old fold (that is the second key space this change
/// removes) and it gets no separate marked key space. Nothing is lost by that:
/// [`ens::resolve`](crate::ens::resolve) refuses an unnormalizable name with its
/// own typed error long before a load, so such a name never reaches a page there
/// is anything to bless. The refusal is returned as an [`UnkeyableName`] rather
/// than a silent `None`, so no caller mistakes it for "unblessed".
///
/// The trim is the store's own tolerance for a stray space around a typed name
/// (` ronan.eth ` is `ronan.eth`, as it always was); the normalizer itself takes
/// the name exactly as spelled.
pub fn pin_key(name: &str) -> Result<String, UnkeyableName> {
    let name = name.trim();
    let unkeyable = |detail: String| UnkeyableName {
        name: name.to_string(),
        detail,
    };
    // The normalizer accepts the EMPTY string (it is the ENS root, which has a
    // well-defined node), but the root is not a name anybody browses to and an
    // empty key would collide with every other empty-ish spelling.
    if name.is_empty() {
        return Err(unkeyable("a name cannot be empty".to_string()));
    }
    let key = crate::ens::normalize_name(name).map_err(|e| {
        unkeyable(match e {
            crate::ens::ResolutionError::UnnormalizableName { detail, .. } => detail,
            // `normalize_name` returns no other variant; carry whatever it says
            // rather than asserting the shape of somebody else's error.
            other => other.to_string(),
        })
    })?;
    if key.is_empty() {
        return Err(unkeyable("it normalizes to an empty name".to_string()));
    }
    Ok(key)
}

/// The persisted trusted-name pins: one small JSON file, isolatable via the
/// [`retrieval`](crate::retrieval) settings mechanism's
/// [`SETTINGS_DIR_ENV`](crate::retrieval::SETTINGS_DIR_ENV) lever.
///
/// Deliberately minimal, exactly like [`RetrievalSettings`](crate::retrieval::RetrievalSettings)
/// (settled decision 2 is "reuse that mechanism verbatim", not "build a
/// database"): a sorted list of pins, [`load`](TrustedNamePins::load) /
/// [`save`](TrustedNamePins::save) — the ONE pair that knows the store lives in
/// the settings directory — plus the directory-taking cores tests drive.
/// A MISSING file loads as EMPTY (a fresh install), while an UNREADABLE one
/// yields [`UndeterminableTrust`] rather than a silent empty store, and blocks
/// the write while it holds (see the module's fail-safe note).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedNamePins {
    /// The pins, kept sorted by [`pin_key`] so the persisted document is stable
    /// (a re-save with no change rewrites identical bytes).
    pins: Vec<TrustedNamePin>,
    /// Members of the loaded DOCUMENT that this build does not know, carried
    /// through untouched so a rewrite cannot strip them (see the module's
    /// unknown-member note). Empty for a store built in memory, which is what
    /// keeps `TrustedNamePins`'s equality meaning "the same pins, plus whatever
    /// else the same document carried".
    unknown_document_members: Map<String, Value>,
    /// Members of a loaded ENTRY that this build does not know, keyed by that
    /// entry's [`pin_key`], so they are re-attached to the SAME pin on the way
    /// out. Keyed rather than stored on [`TrustedNamePin`] because that type's
    /// fields are public and constructed by literal in three other crates' tests;
    /// see the decisions doc.
    unknown_entry_members: BTreeMap<String, Map<String, Value>>,
    /// The [`NORMALIZATION_VERSION`] the document this store was READ from was
    /// written by, or `None` when it carried no stamp (it predates stamping, or
    /// this store was built in memory and has never been read from a file).
    ///
    /// Provenance of the FILE, not content of the store: a save always stamps
    /// the document with THIS build's version, because this build is the one
    /// writing it. Reading it back is how a mismatch becomes visible
    /// ([`normalization_version_mismatch`](TrustedNamePins::normalization_version_mismatch)).
    normalization_version: Option<String>,
}

impl TrustedNamePins {
    /// Read the USER's store: the [`PinStoreRead`] for `pins.json` in the settings
    /// directory, or [`NoStore`](PinStoreRead::NoStore) when this system has no
    /// settings directory at all.
    ///
    /// The three answers are deliberately distinct (the module's fail-safe note):
    /// "there is nowhere to read from" is not "an empty store" (a caller holding
    /// pins in memory keeps them, because no file could have superseded them), and
    /// neither of those is "werust cannot determine what is blessed".
    ///
    /// This is the ONLY way to read the USER's store: the shell reaches it through
    /// its own `PinStoreLocation::Settings`, which delegates here rather than
    /// re-deriving `settings_dir().map(load_from)` itself, so there is exactly one
    /// site that knows where the user's `pins.json` is (task
    /// `pin-warning-reads-a-stale-cache-so-another-windows-bless-never-warns`).
    #[must_use]
    pub fn load() -> PinStoreRead {
        match crate::retrieval::settings_dir() {
            Some(dir) => Self::load_from(&dir).into(),
            None => PinStoreRead::NoStore,
        }
    }

    /// Load the pins from a SPECIFIC directory (the directory-taking core
    /// [`load`](TrustedNamePins::load) delegates to), or say WHY trust cannot be
    /// determined from what is there.
    ///
    /// The explicit-directory seam, mirroring
    /// [`RetrievalSettings::load_from`](crate::retrieval::RetrievalSettings::load_from):
    /// tests pass their OWN scratch directory so they isolate the store WITHOUT
    /// mutating process-global env, and the real `pins.json` is never touched.
    ///
    /// A MISSING file is `Ok` and EMPTY — a fresh install must behave exactly as
    /// it always has. Every OTHER read failure is [`UndeterminableTrust`], because
    /// the alternative (reading an unreadable store as "nothing trusted") is the
    /// silent re-trust this module refuses to perform.
    pub fn load_from(dir: &std::path::Path) -> Result<Self, UndeterminableTrust> {
        match std::fs::read_to_string(dir.join(PINS_FILE)) {
            Ok(text) => Self::from_json(&text),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(UndeterminableTrust::Unreadable(err.to_string())),
        }
    }

    /// Persist the pins to the settings directory, creating it if needed, and say
    /// WHAT happened ([`PinSaveOutcome`]): recorded, could not be persisted (no
    /// settings directory on this system, or the write failed), or REFUSED because
    /// the store on disk cannot be read (see [`save_to`](TrustedNamePins::save_to)).
    ///
    /// Only the first survives a relaunch; in every other case the bless still
    /// took effect for THIS session, it just could not be recorded.
    pub fn save(&self) -> PinSaveOutcome {
        match crate::retrieval::settings_dir() {
            Some(dir) => self.save_to(&dir),
            None => {
                PinSaveOutcome::CouldNotPersist("this system has no settings directory".to_string())
            }
        }
    }

    /// Persist the pins to a SPECIFIC directory (the directory-taking core
    /// [`save`](TrustedNamePins::save) delegates to), creating it if needed, and
    /// say WHAT happened ([`PinSaveOutcome`]).
    ///
    /// # The write is ATOMIC: a temp file beside the store, renamed over it
    ///
    /// The document is written to a temp file in the SAME directory and then
    /// RENAMED onto `pins.json`, so a reader observes the old document or the new
    /// one and never a truncated one. A crash, an OOM kill or a full disk between
    /// the first byte and the last is the case: a bare whole-file write leaves a
    /// half-written store, which is the worst possible state for a record the
    /// browser is about to trust, and (since `docs/adr/0014`) one that blocks
    /// every later write too, because an unreadable store refuses them.
    ///
    /// SAME directory is load-bearing, not tidiness: a rename is atomic only
    /// within one filesystem, and a temp directory is routinely a different one,
    /// where the rename degrades into a copy and the guarantee is gone. The temp
    /// file is removed on the failure path, so nothing is left beside the store.
    ///
    /// # The write REFUSES while trust cannot be determined
    ///
    /// This is the write half of the fail-closed rule (`docs/adr/0014`), and it
    /// lives HERE rather than at each caller so no writer (today's
    /// `bless_current_name`, or a later "forget this pin") can forget it: the
    /// save re-reads the document it is about to replace, and returns
    /// [`Refused`](PinSaveOutcome::Refused) without touching a byte when that read
    /// is [`UndeterminableTrust`]. Overwriting a store werust could not read is
    /// how ONE transient failure permanently destroys every record it holds.
    ///
    /// Neither the refusal nor a failed write is an ERROR: the bless holds for
    /// THIS session and simply cannot survive a relaunch, because a store werust
    /// cannot read or write must not break browsing.
    pub fn save_to(&self, dir: &std::path::Path) -> PinSaveOutcome {
        self.save_to_through(dir, write_document)
    }

    /// [`save_to`](TrustedNamePins::save_to)'s body, with the step that puts BYTES
    /// in the temp file supplied by the caller: the seam a test INTERRUPTS.
    ///
    /// Injecting the write step is what lets the mid-write failure (a full disk, a
    /// power cut) be exercised through the real temp-then-rename path, and lets a
    /// test assert from INSIDE that step that the live document has not been
    /// touched yet. The alternative shapes were both worse: a `cfg!(test)` branch
    /// is production behaviour that differs in a test build (this repo retired its
    /// only one, and `crates/werust-core/tests/pin_store_edge_wiring_shape.rs`
    /// reds the gate if it returns), and asserting only the observable end states
    /// would never prove the ORDER, which is the entire property.
    fn save_to_through(
        &self,
        dir: &std::path::Path,
        write_temp: impl FnOnce(&std::path::Path, &str) -> std::io::Result<()>,
    ) -> PinSaveOutcome {
        if dir.as_os_str().is_empty() {
            return PinSaveOutcome::CouldNotPersist(
                "there is no directory to record into".to_string(),
            );
        }
        if let Err(err) = std::fs::create_dir_all(dir) {
            return PinSaveOutcome::CouldNotPersist(err.to_string());
        }
        if let Err(why) = Self::load_from(dir) {
            return PinSaveOutcome::Refused(why);
        }
        let temp = dir.join(temp_document_name());
        match write_temp(&temp, &self.to_json())
            .and_then(|()| std::fs::rename(&temp, dir.join(PINS_FILE)))
        {
            Ok(()) => {
                // The rename is the commit point; flushing the DIRECTORY is what
                // makes it durable across a power cut on the filesystems that need
                // it. Best-effort: it is not openable as a file on every platform
                // (Windows), and failing to fsync a directory does not make the
                // store any less correct for a reader.
                let _ = std::fs::File::open(dir).map(|dir| dir.sync_all());
                PinSaveOutcome::Recorded
            }
            Err(err) => {
                // Leave nothing behind: a half-written temp file beside the store
                // is litter at best, and at worst the next reader's puzzle.
                let _ = std::fs::remove_file(&temp);
                PinSaveOutcome::CouldNotPersist(err.to_string())
            }
        }
    }

    /// The pin for `name`, or `None` when it has never been blessed — or has no
    /// [`pin_key`] at all, which is the same answer to the question asked here
    /// ("what has the user blessed for this name?") and never a claim that the
    /// name is fine.
    ///
    /// Looked up by [`pin_key`], so no spelling of one identity can split it
    /// across two pins.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TrustedNamePin> {
        let key = pin_key(name).ok()?;
        self.pins.iter().find(|pin| pin.name == key)
    }

    /// Record (or RE-record) `name`'s current `cid` as blessed, with the posture
    /// werust was showing and the moment the user did it — or say WHY the name
    /// cannot be recorded at all.
    ///
    /// Re-blessing a changed name REPLACES its pin: the SSH-host-key model's
    /// "I have looked at the change and I accept the new content". The store is
    /// therefore always at most one pin per name.
    ///
    /// A name with no [`pin_key`] records NOTHING and returns the
    /// [`UnkeyableName`] saying so. Silently doing nothing is the one shape this
    /// must not have: the user would click bless, the store would report a
    /// successful save of an unchanged document, and no warning would ever
    /// follow. It is unreachable through the browser (an unnormalizable name
    /// fails resolution long before a page exists to bless), which is why it is a
    /// refusal rather than a fallback key.
    pub fn bless(
        &mut self,
        name: &str,
        cid: &str,
        posture: TrustPosture,
        blessed_at: u64,
    ) -> Result<(), UnkeyableName> {
        let pin = TrustedNamePin {
            name: pin_key(name)?,
            cid: cid.to_string(),
            blessed_at,
            posture,
        };
        match self
            .pins
            .iter_mut()
            .find(|existing| existing.name == pin.name)
        {
            Some(existing) => *existing = pin,
            None => {
                self.pins.push(pin);
                self.pins.sort_by(|a, b| a.name.cmp(&b.name));
            }
        }
        Ok(())
    }

    /// The [`MutableNameTrust`] for a name resolving to `cid` right now: the
    /// chrome axis value, pairing the live identity with whatever is on file.
    ///
    /// This is the ONE place the store is consulted per load, so no presentation
    /// rule ever reads the filesystem.
    ///
    /// `None` when the name has no [`pin_key`]: there is no identity to pair, so
    /// there is no axis — no pin, no warning, and no bless offer werust would
    /// then have to refuse. That is deliberately NOT a `MutableNameTrust` with
    /// `blessed: None`, which reads as "this name is simply unblessed" and would
    /// offer a bless that could never be recorded.
    #[must_use]
    pub fn check(&self, name: &str, cid: &str) -> Option<MutableNameTrust> {
        let key = pin_key(name).ok()?;
        Some(MutableNameTrust {
            name: key,
            cid: cid.to_string(),
            blessed: self.get(name).cloned(),
        })
    }

    /// The normalization version the document this store was read from was
    /// WRITTEN by ([`NORMALIZATION_VERSION`]), or `None` when it carried no
    /// stamp.
    ///
    /// `None` is NOT an error and not a mismatch: a store written before
    /// stamping existed simply says nothing about it, and it loads exactly as it
    /// always did (the module's stamp note). A store built in memory has never
    /// been read from a file and says nothing either.
    #[must_use]
    pub fn normalization_version(&self) -> Option<&str> {
        self.normalization_version.as_deref()
    }

    /// The recorded normalization version when it is NOT this build's, i.e. the
    /// records in this file were keyed by a DIFFERENT normalization than the one
    /// [`pin_key`] applies today.
    ///
    /// `None` covers both "the same version wrote it" and "it carries no stamp",
    /// which are the two cases with nothing to report.
    ///
    /// Reading this is deliberately all that happens to a mismatch. Nothing in
    /// werust acts on it: it does not make the store [`UndeterminableTrust`], it
    /// does not refuse a write, it does not re-key and it does not change what
    /// the chrome says (the module's stamp note has the reasoning, and
    /// `a_mismatched_stamp_is_recorded_and_readable_and_changes_nothing_else`
    /// asserts it). It exists so the surface or migration that eventually DOES
    /// act on it inherits a fact instead of a guess.
    #[must_use]
    pub fn normalization_version_mismatch(&self) -> Option<&str> {
        self.normalization_version()
            .filter(|recorded| *recorded != NORMALIZATION_VERSION)
    }

    /// How many names are blessed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pins.len()
    }

    /// Whether nothing is blessed — a fresh install. NEVER the answer for a store
    /// werust could not read: that one is [`UndeterminableTrust`], which is not a
    /// [`TrustedNamePins`] at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pins.is_empty()
    }

    /// Serialize to the persisted wire form:
    /// `{"pins":[{"name":…,"cid":…,"blessedAt":…,"posture":"<wire name>"}]}`,
    /// PLUS every member this build does not know, exactly as it was read.
    ///
    /// The posture uses the ONE shared wire vocabulary
    /// ([`trust_posture_wire_name`]) the chrome JSON and the debug view's Network
    /// tab already speak (`docs/adr/0006`), so the store never mints a second
    /// spelling of a posture.
    ///
    /// The document also records WHICH normalization wrote it
    /// (`"normalizationVersion":"<`[`NORMALIZATION_VERSION`]`>"`), always this
    /// build's: a save is this build writing the file, so the stamp it leaves is
    /// its own, whatever the document it replaces said (the module's stamp note).
    ///
    /// The unknown members are written FIRST and the fields werust owns second,
    /// so a preserved member can never shadow a field this build is authoritative
    /// for. (Key order in the document itself is `serde_json`'s, which sorts.)
    #[must_use]
    pub fn to_json(&self) -> String {
        let pins: Vec<Value> = self
            .pins
            .iter()
            .map(|pin| {
                let mut entry = self
                    .unknown_entry_members
                    .get(&pin.name)
                    .cloned()
                    .unwrap_or_default();
                entry.insert("name".to_string(), json!(pin.name));
                entry.insert("cid".to_string(), json!(pin.cid));
                entry.insert("blessedAt".to_string(), json!(pin.blessed_at));
                entry.insert(
                    "posture".to_string(),
                    json!(trust_posture_wire_name(pin.posture)),
                );
                Value::Object(entry)
            })
            .collect();
        let mut document = self.unknown_document_members.clone();
        document.insert("pins".to_string(), Value::Array(pins));
        document.insert(
            NORMALIZATION_VERSION_MEMBER.to_string(),
            json!(NORMALIZATION_VERSION),
        );
        Value::Object(document).to_string()
    }

    /// Parse the persisted wire form, or say WHY trust cannot be determined from
    /// this document.
    ///
    /// Every failure REPORTS instead of shrinking the store (the module's
    /// fail-safe note): a document that is not this wire form is
    /// [`Unparseable`](UndeterminableTrust::Unparseable), an entry werust cannot
    /// read honestly (no name, no CID, no timestamp, a posture spelling this build
    /// does not know) is [`UnreadableEntry`](UndeterminableTrust::UnreadableEntry),
    /// and two entries for one key are
    /// [`DuplicateName`](UndeterminableTrust::DuplicateName). A
    /// `normalizationVersion` that is not a STRING is not this wire form either,
    /// and is reported for the same reason the `pins` member's type is: this
    /// build owns that member's spelling and would otherwise overwrite a value it
    /// could not read. A MISSING stamp is not a failure at all — it predates
    /// stamping (the module's stamp note). Dropping any of them
    /// would be silent: the next save persists the survivors, so a build that met
    /// a fifth [`TrustPosture`] would un-trust every name recorded under it.
    pub fn from_json(text: &str) -> Result<Self, UndeterminableTrust> {
        let value: Value = serde_json::from_str(text)
            .map_err(|err| UndeterminableTrust::Unparseable(err.to_string()))?;
        let entries = value
            .get("pins")
            .and_then(Value::as_array)
            .ok_or_else(|| UndeterminableTrust::Unparseable("no `pins` array".to_string()))?;
        let mut pins: Vec<TrustedNamePin> = Vec::with_capacity(entries.len());
        let mut unknown_entry_members: BTreeMap<String, Map<String, Value>> = BTreeMap::new();
        for (index, entry) in entries.iter().enumerate() {
            let (pin, unknown) = read_entry(index, entry)?;
            if !unknown.is_empty() {
                unknown_entry_members.insert(pin.name.clone(), unknown);
            }
            pins.push(pin);
        }
        pins.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(duplicate) = pins.windows(2).find(|pair| pair[0].name == pair[1].name) {
            return Err(UndeterminableTrust::DuplicateName(
                duplicate[0].name.clone(),
            ));
        }
        // WHICH normalization wrote this document, if it says. Absent is the
        // ordinary case for a store written before stamping existed, and is not a
        // failure; present-but-not-a-string is a document that is not this wire
        // form, reported rather than silently replaced on the next save.
        let normalization_version = match value.get(NORMALIZATION_VERSION_MEMBER) {
            None => None,
            Some(Value::String(recorded)) => Some(recorded.clone()),
            Some(other) => {
                return Err(UndeterminableTrust::Unparseable(format!(
                    "`{NORMALIZATION_VERSION_MEMBER}` is not a string: `{other}`"
                )))
            }
        };
        // Everything BESIDE the members this build OWNS: carried, not understood,
        // and written back untouched (the module's unknown-member note). `value`
        // is an object here, because a non-object has no `pins` member to have got
        // this far.
        let unknown_document_members = value
            .as_object()
            .map(|document| {
                document
                    .iter()
                    .filter(|(member, _)| !KNOWN_DOCUMENT_MEMBERS.contains(&member.as_str()))
                    .map(|(member, carried)| (member.clone(), carried.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            pins,
            unknown_document_members,
            unknown_entry_members,
            normalization_version,
        })
    }
}

/// One persisted entry -> one [`TrustedNamePin`] PLUS the members of that entry
/// this build does not know, or WHY werust cannot read it honestly. The entry's
/// position is carried in the reason because the name is exactly the field that
/// may be missing.
fn read_entry(
    index: usize,
    entry: &Value,
) -> Result<(TrustedNamePin, Map<String, Value>), UndeterminableTrust> {
    let unreadable = |what: &str| {
        UndeterminableTrust::UnreadableEntry(format!("entry {index} {what}", index = index + 1))
    };
    // The RE-KEY: an entry recorded under an older key derivation (the ASCII
    // `trim().to_lowercase()` fold) is read under TODAY's [`pin_key`], in place.
    // That is the whole migration: a non-ASCII record written by an older build
    // is found by ONE lookup here, so the next bless RE-blesses it instead of
    // adding a second record for a name the user already trusts. It is idempotent
    // by construction (normalizing an already-normalized key returns it), it
    // reaches disk only through the ordinary atomic `save_to` (which refuses
    // while the store is unreadable), and two old records that collapse onto ONE
    // new key fall straight into the existing duplicate-key rule in `from_json`
    // rather than being merged behind the user's back.
    let recorded = entry
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| unreadable("records no name"))?;
    let name = pin_key(recorded).map_err(|why| {
        // Reported, never dropped: the same rule the unknown-posture case follows.
        // A recorded name this build cannot key is a record werust cannot honour,
        // and quietly discarding it would let the next save delete a pin the user
        // is still relying on.
        unreadable(&format!("records a name werust cannot key: {why}"))
    })?;
    let cid = entry
        .get("cid")
        .and_then(Value::as_str)
        .filter(|cid| !cid.is_empty())
        .ok_or_else(|| unreadable(&format!("(`{name}`) records no cid")))?
        .to_string();
    let blessed_at = entry
        .get("blessedAt")
        .and_then(Value::as_u64)
        .ok_or_else(|| unreadable(&format!("(`{name}`) records no bless timestamp")))?;
    let spelling = entry
        .get("posture")
        .and_then(Value::as_str)
        .ok_or_else(|| unreadable(&format!("(`{name}`) records no trust posture")))?;
    let posture = trust_posture_from_wire_name(spelling).ok_or_else(|| {
        unreadable(&format!(
            "(`{name}`) records a trust posture this build does not know: `{spelling}`"
        ))
    })?;
    // Whatever else this entry carried: a LATER build's field, kept so an older
    // one cannot silently delete it (the module's unknown-member note).
    let unknown = entry
        .as_object()
        .map(|members| {
            members
                .iter()
                .filter(|(member, _)| !KNOWN_ENTRY_MEMBERS.contains(&member.as_str()))
                .map(|(member, carried)| (member.clone(), carried.clone()))
                .collect()
        })
        .unwrap_or_default();
    Ok((
        TrustedNamePin {
            name,
            cid,
            blessed_at,
            posture,
        },
        unknown,
    ))
}

/// The entry members THIS build owns and re-serializes from
/// [`TrustedNamePin`]. Everything else in an entry is a later build's, and is
/// carried through untouched.
const KNOWN_ENTRY_MEMBERS: [&str; 4] = ["name", "cid", "blessedAt", "posture"];

/// The DOCUMENT members THIS build owns and re-serializes itself: the pins, and
/// the stamp saying which normalization wrote them. Everything else at the top
/// level is a later build's, and is carried through untouched.
///
/// Every werust before the stamp existed knew only `pins`, so `normalizationVersion`
/// was — to it — an unknown member, which is exactly why an older build
/// preserves a newer one's stamp instead of stripping it.
const KNOWN_DOCUMENT_MEMBERS: [&str; 2] = ["pins", NORMALIZATION_VERSION_MEMBER];

/// The name of the temp file a save writes before renaming it onto `pins.json`,
/// in the SAME directory (see [`TrustedNamePins::save_to`]).
///
/// Unique per process and per call, so two windows saving at the same moment
/// cannot write each other's temp file. It is deliberately NOT unique enough to
/// serialize them: two concurrent read-modify-writes can still lose an update,
/// which is an advisory LOCK's job (task
/// `trust-store-serialises-read-modify-write-so-no-bless-is-lost`), not a file
/// name's. Atomicity makes that loss clean instead of corrupt.
fn temp_document_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{PINS_FILE}.tmp-{pid}-{n}", pid = std::process::id(),)
}

/// Put the document in a file and FLUSH it to the storage device: the default
/// write step [`TrustedNamePins::save_to`] renames into place.
///
/// The `sync_all` is the point of doing this by hand rather than with
/// `fs::write`: renaming a file whose bytes are still only in the page cache
/// gives an atomic swap to a document that may itself be empty after a power
/// cut, which is the very state the temp-then-rename exists to prevent.
fn write_document(path: &std::path::Path, document: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)?;
    file.write_all(document.as_bytes())?;
    file.sync_all()
}

/// The full path to the pin-store file, or `None` if there is no settings dir.
/// The sibling of [`retrieval::settings_file_path`](crate::retrieval::settings_file_path),
/// so the two files are visibly one mechanism.
#[must_use]
pub fn pins_file_path() -> Option<std::path::PathBuf> {
    crate::retrieval::settings_dir().map(|dir| dir.join(PINS_FILE))
}

/// The current moment in whole seconds since the Unix epoch (UTC), for stamping
/// a bless. `0` if the system clock is before the epoch (which only makes the
/// recorded day wrong, never the CID comparison the warning rests on).
#[must_use]
pub fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A Unix timestamp as a legible UTC calendar day, `YYYY-MM-DD`.
///
/// Hand-computed rather than binding a date crate for a formatting concern: the
/// warning quotes ONE day back to the user ("the version you trusted on
/// 2026-07-30"), and a proleptic-Gregorian civil-from-days conversion is a
/// closed-form arithmetic identity (Howard Hinnant's `civil_from_days`, the same
/// algorithm every date library implements) with an exhaustive round-trip test
/// below. This is emphatically NOT the "never hand-roll" rule's territory
/// (`docs/adr/0001` is about crypto and TLS); a timezone-aware, locale-aware date
/// would be, and is deliberately not what this is.
#[must_use]
pub fn format_utc_date(secs: u64) -> String {
    const SECS_PER_DAY: u64 = 86_400;
    let (year, month, day) = civil_from_days((secs / SECS_PER_DAY) as i64);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 -> `(year, month, day)` in the proleptic Gregorian
/// calendar (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    // Shift the epoch to 0000-03-01 so leap days land at the END of the cycle.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11], March = 0
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = yoe as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unique scratch directory under the OS temp dir, isolated per test, that
    /// removes itself on drop, the same shape `retrieval`'s tests use, so a
    /// persistence test writes ONLY here and NEVER the real pin store (the
    /// shared-write rule), with no `tempfile` dependency and no env mutation.
    struct ScratchDir {
        path: std::path::PathBuf,
    }

    impl ScratchDir {
        fn new(tag: &str) -> Self {
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "werust-pins-test-{tag}-{pid}-{n}",
                pid = std::process::id(),
            ));
            let _ = std::fs::remove_dir_all(&path);
            Self { path }
        }
    }

    impl Drop for ScratchDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    /// The REAL `pins.json`'s bytes, or `None` when the developer has none (or
    /// there is no settings directory at all): the before/after snapshot a test
    /// asserts the suite never writes the developer's own pin store with.
    fn real_pin_store_snapshot() -> Option<Vec<u8>> {
        pins_file_path().and_then(|path| std::fs::read(path).ok())
    }

    /// A REAL dag-pb content root, DERIVED from `content` rather than pasted: the
    /// shape an ENS `ipfs-ns` contenthash carries when it holds a CIDv0, and the
    /// ONE root every spelling in the comparison tests names.
    ///
    /// Built from the same [`fetcher::cid_v1_raw_sha256`] helper the verified
    /// retrieval path derives with (the `cid 0.11` lineage the verify boundary
    /// owns), re-wrapped as a CIDv0 over that exact sha2-256 multihash: the same
    /// derivation `contenthash`'s own CIDv0 test uses. No hand-built digest, no
    /// pasted fixture string.
    fn dag_pb_root(content: &[u8]) -> fetcher::Cid {
        let raw = fetcher::cid_v1_raw_sha256(content).expect("derive a sha2-256 root");
        let raw = fetcher::Cid::try_from(raw.as_str()).expect("the derived root parses");
        fetcher::Cid::new_v0(*raw.hash()).expect("a CIDv0 over the same sha2-256 multihash")
    }

    /// A DIFFERENT content root whose canonical spelling differs from `root`'s
    /// only at the very END: the same multihash with the LAST digest byte flipped.
    ///
    /// base32 packs five bits per character, so flipping the last bit moves only
    /// the final character or two: the pair a comparison that gave up early (a
    /// prefix match, a truncated key) would call equal.
    fn root_with_a_late_difference(root: &fetcher::Cid) -> fetcher::Cid {
        let mut digest = root.hash().digest().to_vec();
        *digest.last_mut().expect("a sha2-256 digest is 32 bytes") ^= 0x01;
        let hash = cid::multihash::Multihash::<64>::wrap(root.hash().code(), &digest)
            .expect("a 32-byte sha2-256 multihash");
        fetcher::Cid::new_v1(root.codec(), hash)
    }

    /// `pins` as it READS BACK after this build saves it: the same value, plus
    /// the normalization stamp the save writes ([`NORMALIZATION_VERSION`], the
    /// module's stamp note).
    ///
    /// A save STAMPS the document with the version that wrote it, so a store
    /// built in memory (no stamp) or loaded from a document written before
    /// stamping existed is deliberately NOT equal to what comes back off disk:
    /// the file now records something it did not record before. The tests that
    /// assert a save round-trips say so through this helper rather than dropping
    /// the stamp out of the comparison, which would stop them noticing if a save
    /// ever failed to stamp.
    fn as_read_back(pins: &TrustedNamePins) -> TrustedNamePins {
        TrustedNamePins {
            normalization_version: Some(NORMALIZATION_VERSION.to_string()),
            ..pins.clone()
        }
    }

    /// Every file name in a scratch directory, sorted: what a test asserts a save
    /// left behind, so a temp file that survives (a successful save's, or an
    /// interrupted one's) is a FAILURE rather than something nobody looked for.
    fn dir_entries(dir: &std::path::Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .expect("the scratch dir exists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_blessed_name_persists_across_launches_in_the_isolated_store() {
        // Acceptance: the pin (name -> CID + timestamp + posture) persists across
        // launches, isolated to a scratch dir through the directory-taking core.
        let scratch = ScratchDir::new("persist");
        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a fresh install reads");
        assert!(pins.is_empty(), "a fresh install has no pins");

        pins.bless(
            "ronan.eth",
            "bafyone",
            TrustPosture::NameViaTrustedRpc,
            1_800_000_000,
        )
        .expect("a name werust can key");
        assert!(pins.save_to(&scratch.path).is_recorded());
        assert!(
            scratch.path.join(PINS_FILE).is_file(),
            "the store is `pins.json`, in the scratch dir only"
        );

        // A fresh load (a new "launch") reads the SAME pin back, all three facts.
        let reloaded = TrustedNamePins::load_from(&scratch.path).expect("the store reads back");
        let pin = reloaded
            .get("ronan.eth")
            .expect("the pin survived a reload");
        assert_eq!(pin.cid, "bafyone");
        assert_eq!(pin.blessed_at, 1_800_000_000);
        assert_eq!(pin.posture, TrustPosture::NameViaTrustedRpc);
    }

    #[test]
    fn the_pin_store_writes_only_under_its_own_directory_and_beside_retrieval_json() {
        // The shared-write rule, asserted rather than assumed: a save touches the
        // scratch dir and nothing else, and the REAL store is untouched — which is
        // ASSERTED here (a before/after snapshot of the real `pins.json`), not
        // merely argued from "this test drives the directory-taking core". And the
        // file sits BESIDE `retrieval.json` (one mechanism, settled decision 2),
        // which is what `pins_file_path` promises.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("isolation");
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert!(pins.save_to(&scratch.path).is_recorded());
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );

        assert_eq!(dir_entries(&scratch.path), vec![PINS_FILE.to_string()]);

        // Both files resolve under the SAME directory, whatever it is.
        if let (Some(pins_path), Some(settings_path)) =
            (pins_file_path(), crate::retrieval::settings_file_path())
        {
            assert_eq!(pins_path.parent(), settings_path.parent());
            assert_ne!(pins_path, settings_path);
        }
    }

    #[test]
    fn a_later_resolution_to_a_different_cid_is_a_change_not_a_silent_accept() {
        // Acceptance: the warning condition is blessed AND different. An unblessed
        // name is NOT a change (fail-safe: it behaves exactly as before).
        let pin = TrustedNamePin {
            name: "ronan.eth".into(),
            cid: "bafyold".into(),
            blessed_at: 1_800_000_000,
            posture: TrustPosture::NameViaTrustedRpc,
        };
        let changed = MutableNameTrust {
            name: "ronan.eth".into(),
            cid: "bafynew".into(),
            blessed: Some(pin),
        };
        assert!(changed.is_changed());
        assert!(changed.is_blessed());
        assert!(!changed.is_unchanged());
        assert!(
            changed.is_blessable(),
            "the user can look, then accept the new content"
        );
        assert_eq!(changed.blessed_on().as_deref(), Some("2027-01-15"));

        let same = MutableNameTrust {
            cid: "bafyold".into(),
            ..changed.clone()
        };
        assert!(!same.is_changed());
        assert!(same.is_unchanged());
        assert!(
            !same.is_blessable(),
            "an already-blessed, unchanged name has nothing left to record"
        );

        let unblessed = MutableNameTrust {
            blessed: None,
            ..changed.clone()
        };
        assert!(!unblessed.is_changed(), "an unblessed name never warns");
        assert!(!unblessed.is_blessed());
        assert!(unblessed.is_blessable(), "first use is the bless offer");
        assert_eq!(unblessed.blessed_on(), None);
    }

    #[test]
    fn re_blessing_replaces_the_pin_rather_than_growing_a_second_one() {
        // The SSH-host-key model's "I looked, and I accept the new content": at
        // most ONE pin per name, so the next change is measured against what the
        // user last accepted.
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafyold", TrustPosture::MutableName, 10)
            .expect("a name werust can key");
        pins.bless("ronan.eth", "bafynew", TrustPosture::NameViaTrustedRpc, 20)
            .expect("a name werust can key");
        assert_eq!(pins.len(), 1);
        let pin = pins.get("ronan.eth").expect("still one pin");
        assert_eq!(pin.cid, "bafynew");
        assert_eq!(pin.blessed_at, 20);
        assert_eq!(pin.posture, TrustPosture::NameViaTrustedRpc);
    }

    #[test]
    fn a_names_casing_cannot_split_it_across_two_pins() {
        // ENS names are case-insensitive, so `Ronan.eth` and `ronan.eth` are ONE
        // name. Two pins would make the warning MISS, the one failure a TOFU store
        // cannot have.
        let mut pins = TrustedNamePins::default();
        pins.bless("Ronan.ETH", "bafyone", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert_eq!(pins.len(), 1);
        assert_eq!(
            pins.get(" ronan.eth ").map(|p| p.cid.as_str()),
            Some("bafyone")
        );
        assert!(pins
            .check("RONAN.eth", "bafyother")
            .expect("a name werust can key")
            .is_changed());
    }

    #[test]
    fn both_ens_and_ipns_style_names_are_blessable_and_checked_the_same_way() {
        // Acceptance (settled decision 3): the store keys on the NAME, whatever
        // kind it is: an `ipfs-ns` ENS name, an `ipns-ns` ENS name, or a bare
        // IPNS name. Both axes' names are controller-repointable, so both are
        // blessable and both warn identically.
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafyens", TrustPosture::NameViaTrustedRpc, 1)
            .expect("an ENS name has a key");
        pins.bless(IPNS_KEY_FIXTURE, "bafyipns", TrustPosture::MutableName, 2)
            .expect("a bare IPNS key has a key too");
        assert!(checked(&pins, "ronan.eth", "bafyens").is_unchanged());
        assert!(checked(&pins, "ronan.eth", "bafyelse").is_changed());
        assert!(checked(&pins, IPNS_KEY_FIXTURE, "bafyipns").is_unchanged());
        assert!(checked(&pins, IPNS_KEY_FIXTURE, "bafyelse").is_changed());
        // An unknown name is simply unblessed on either axis.
        assert!(!checked(&pins, "stranger.eth", "bafyany").is_blessed());
    }

    /// A real base36 IPNS key (the `k51qzi…` form an `ipns-ns` contenthash
    /// decodes to), for the decision-2 assertion that a NON-ENS name goes through
    /// the same normalization and comes out unchanged.
    const IPNS_KEY_FIXTURE: &str = "k51qzi5uqu5dlvj2baxnqndepeb86cbk3ng7n3i46uzyxzyqj2xjonzllnv0v8";

    /// ONE identity spelled two ways that ENSIP-15 normalization COLLAPSES and
    /// ASCII case folding does not: an emoji with and without the U+FE0F
    /// variation selector. Both are already lower case, so `to_lowercase()`
    /// leaves them as two DIFFERENT strings — the store's old key space split one
    /// blessed identity in two, and the warning missed.
    const HEART_WITH_SELECTOR: &str = "❤\u{fe0f}.eth";
    const HEART_WITHOUT_SELECTOR: &str = "❤.eth";

    /// [`TrustedNamePins::check`] for a name the test asserts IS keyable, so the
    /// existing assertions read as they did before the key became fallible.
    fn checked(pins: &TrustedNamePins, name: &str, cid: &str) -> MutableNameTrust {
        pins.check(name, cid)
            .unwrap_or_else(|| panic!("`{name}` has a store key"))
    }

    #[test]
    fn one_resolved_identity_is_one_key_even_when_normalizing_is_not_case_folding() {
        // Acceptance: the key is the ENSIP-15-normalized name the resolution
        // produced, so a name whose normalization is NOT plain case folding is
        // looked up under exactly ONE key. The pair here differs only by an
        // INVISIBLE U+FE0F, which the old `trim().to_lowercase()` key kept as two
        // records: one blessed identity, two pins, and a warning that misses.
        assert_ne!(
            HEART_WITH_SELECTOR.trim().to_lowercase(),
            HEART_WITHOUT_SELECTOR.trim().to_lowercase(),
            "the OLD ASCII fold really did split this identity in two"
        );
        assert_eq!(
            pin_key(HEART_WITH_SELECTOR).expect("a normalizable name"),
            pin_key(HEART_WITHOUT_SELECTOR).expect("a normalizable name"),
            "one identity, one key"
        );

        let mut pins = TrustedNamePins::default();
        pins.bless(
            HEART_WITH_SELECTOR,
            "bafyone",
            TrustPosture::NameViaTrustedRpc,
            1,
        )
        .expect("a normalizable name records a pin");
        assert_eq!(pins.len(), 1);
        assert!(
            checked(&pins, HEART_WITHOUT_SELECTOR, "bafyone").is_unchanged(),
            "the OTHER spelling finds the SAME record"
        );
        assert!(checked(&pins, HEART_WITHOUT_SELECTOR, "bafytwo").is_changed());

        // Re-blessing under the other spelling REPLACES that one record rather
        // than growing a second key space.
        pins.bless(
            HEART_WITHOUT_SELECTOR,
            "bafytwo",
            TrustPosture::MutableName,
            2,
        )
        .expect("a normalizable name records a pin");
        assert_eq!(pins.len(), 1, "still ONE record: {pins:?}");
        assert_eq!(
            pins.get(HEART_WITH_SELECTOR).map(|pin| pin.cid.as_str()),
            Some("bafytwo")
        );
    }

    #[test]
    fn a_bare_ipns_key_normalizes_to_itself_so_it_needs_no_second_key_space() {
        // Settled decision 2, asserted rather than argued: a NON-ENS name goes
        // through the SAME normalization, and that is a no-op on it. A base36
        // IPNS key is a lower-case ASCII, dot-less label, so it comes back
        // unchanged — and a future normalizer that started mangling such keys
        // would red the gate here instead of silently orphaning every IPNS pin.
        assert_eq!(
            pin_key(IPNS_KEY_FIXTURE).expect("a bare IPNS key is a name werust can key"),
            IPNS_KEY_FIXTURE,
            "normalization is the identity on a bare IPNS key"
        );
    }

    #[test]
    fn a_name_that_cannot_be_normalized_gets_no_key_and_records_no_pin() {
        // Settled decision 1: the key is FALLIBLE. A name ENSIP-15 normalization
        // refuses has NO key, so it has no pin and no warning — it does NOT fall
        // back to the old trimmed-and-lower-cased form (that is the second key
        // space this task exists to remove). The refusal is LEGIBLE at the call
        // site rather than a silent `None` a later caller reads as "unblessed":
        // the write says why, and the check has no axis to offer a bless from.
        //
        // Nothing can reach a load under such a name anyway — `ens::resolve`
        // refuses it with its own typed `UnnormalizableName` long before a bless
        // is possible — which is exactly why no fallback key is needed.
        for refused in ["a..b.eth", "under_score.eth", "not a name", "", "   "] {
            let why = pin_key(refused).expect_err("not a name werust can key");
            assert!(
                why.to_string().contains("key"),
                "the refusal is legible: {why}"
            );

            let mut pins = TrustedNamePins::default();
            assert_eq!(
                pins.bless(refused, "bafy", TrustPosture::MutableName, 1),
                Err(why),
                "no key, no pin — and the write SAYS so"
            );
            assert!(pins.is_empty(), "nothing was recorded under a fallback key");
            assert!(pins.get(refused).is_none());
            assert!(
                pins.check(refused, "bafy").is_none(),
                "no key means no TOFU axis at all, never a bless offer"
            );
        }
    }

    #[test]
    fn a_store_written_under_the_old_ascii_fold_is_re_keyed_on_load_and_stays_one_record() {
        // Acceptance (the MIGRATION): a non-ASCII record written under the old
        // `trim().to_lowercase()` key would MISS after this change, and the next
        // bless would write a SECOND record for a name the user already trusts —
        // precisely the missed warning the spec exists to close. So the store is
        // re-keyed as it is READ, one lookup finds it, and the first bless after
        // the upgrade creates no second entry. The rewrite reaches disk through
        // the ordinary atomic save, which refuses while the store is unreadable.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("re-key");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let old_key = HEART_WITH_SELECTOR.trim().to_lowercase();
        let new_key = pin_key(HEART_WITH_SELECTOR).expect("a normalizable name");
        assert_ne!(old_key, new_key, "the fixture really is a re-keying case");
        std::fs::write(
            scratch.path.join(PINS_FILE),
            format!(
                r#"{{"pins":[{{"name":"{old_key}","cid":"bafyold","blessedAt":1800000000,"posture":"name-via-trusted-rpc"}}]}}"#
            ),
        )
        .unwrap();

        // ONE lookup, under the name the resolution produces today, finds it.
        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        assert_eq!(pins.len(), 1);
        let pin = pins
            .get(HEART_WITHOUT_SELECTOR)
            .expect("the old record is found under the NEW key");
        assert_eq!(pin.name, new_key, "re-keyed in place");
        assert_eq!(pin.cid, "bafyold", "and it is the SAME record");
        assert_eq!(pin.blessed_at, 1_800_000_000);

        // The first bless after the upgrade RE-blesses that one record; it does
        // not add a second one for a name the user already trusts.
        pins.bless(
            HEART_WITHOUT_SELECTOR,
            "bafynew",
            TrustPosture::NameViaTrustedRpc,
            2,
        )
        .expect("a normalizable name records a pin");
        assert_eq!(pins.len(), 1, "no second record: {pins:?}");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);
        assert_eq!(
            dir_entries(&scratch.path),
            vec![PINS_FILE.to_string()],
            "the re-key goes through the ordinary atomic save: no temp file left"
        );
        let document = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        assert!(
            document.contains(&new_key) && !document.contains(&old_key),
            "the persisted document holds ONE key space: {document}"
        );

        // IDEMPOTENT: a second launch re-keys nothing, and a re-save is byte for
        // byte the same document.
        let reloaded = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        assert_eq!(reloaded, as_read_back(&pins));
        assert_eq!(reloaded.to_json(), pins.to_json());
        assert_eq!(reloaded.save_to(&scratch.path), PinSaveOutcome::Recorded);
        assert_eq!(
            std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store"),
            document,
            "running the migration twice changes nothing"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn no_re_key_is_written_while_the_store_cannot_be_read() {
        // The migration is a WRITE, so it obeys the write rules it inherited: a
        // store werust cannot read is never overwritten, not even to re-key it.
        // A re-key that ignored this would be the worst version of the failure
        // `docs/adr/0014` exists to prevent — a rewrite of every record in a file
        // werust could not read in the first place.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("re-key-refused");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let corrupt = br#"{"pins":[{"name":"ronan.eth","cid":"bafyold","blessedAt":1,"posture":"from-the-future"}]}"#;
        std::fs::write(scratch.path.join(PINS_FILE), corrupt).unwrap();

        // There is nothing to re-key, because there is nothing werust could read.
        assert!(TrustedNamePins::load_from(&scratch.path).is_err());
        let mut pins = TrustedNamePins::default();
        pins.bless(HEART_WITH_SELECTOR, "bafynew", TrustPosture::MutableName, 2)
            .expect("a normalizable name");
        assert!(matches!(
            pins.save_to(&scratch.path),
            PinSaveOutcome::Refused(_)
        ));
        assert_eq!(
            std::fs::read(scratch.path.join(PINS_FILE)).unwrap(),
            corrupt,
            "the records werust could not read are still on disk, byte for byte"
        );
        assert_eq!(
            dir_entries(&scratch.path),
            vec![PINS_FILE.to_string()],
            "and no temp file was left beside them"
        );
        assert_eq!(real_pin_store_snapshot(), real_before);
    }

    #[test]
    fn two_old_records_that_re_key_onto_one_key_are_reported_never_silently_merged() {
        // Acceptance: the collapse this change can produce is the very bug being
        // fixed (two records for ONE identity), so it is REPORTED through the
        // store's existing duplicate-key outcome rather than resolved by picking
        // whichever entry happened to sort first. Silently choosing would decide
        // for the user WHICH content they trusted, and the write refuses while it
        // holds, so nothing overwrites the two records either.
        let scratch = ScratchDir::new("re-key-collision");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let with = HEART_WITH_SELECTOR.trim().to_lowercase();
        let without = HEART_WITHOUT_SELECTOR.trim().to_lowercase();
        std::fs::write(
            scratch.path.join(PINS_FILE),
            format!(
                r#"{{"pins":[
                    {{"name":"{with}","cid":"bafyone","blessedAt":1,"posture":"mutable-name"}},
                    {{"name":"{without}","cid":"bafytwo","blessedAt":2,"posture":"mutable-name"}}
                ]}}"#
            ),
        )
        .unwrap();

        let why =
            TrustedNamePins::load_from(&scratch.path).expect_err("two old records, one new key");
        assert!(
            matches!(&why, UndeterminableTrust::DuplicateName(name) if *name == without),
            "the existing duplicate-key rule reports it: {why:?}"
        );
        assert!(why.to_string().contains(&without), "legible: {why}");
    }

    #[test]
    fn one_content_root_in_another_cid_spelling_is_not_a_change() {
        // Acceptance: a REPUBLISH OF IDENTICAL CONTENT under a different CID
        // spelling must not warn. The ENSIP-7 decoder emits whatever form the
        // contenthash carried (a CIDv0 `Qm…` here), while the Android edge hands
        // the core the lowercase base32 CIDv1 of the SAME root, so one content
        // root reaches this comparison under more than one string. Raw string
        // equality called that a change: the false positive that trains a user to
        // click through the warning that matters.
        let root = dag_pb_root(b"the version the user blessed");
        let v0 = root.to_string();
        let v1 = root.into_v1().expect("a CIDv0 converts to CIDv1");
        let android = v1.to_string();
        assert!(v0.starts_with("Qm"), "the CIDv0 form is base58btc: {v0}");
        assert!(
            android.starts_with("bafybe"),
            "the Android/ENS canonical form is lowercase base32 CIDv1: {android}"
        );
        assert_ne!(v0, android, "one root, two strings: the whole problem");

        // Blessed in the RESOLUTION's form, seen in the form the Android edge
        // hands the core (`crates/werust-android/rust/src/origin_map.rs`).
        let mut pins = TrustedNamePins::default();
        pins.bless(
            "ronan.eth",
            &v0,
            TrustPosture::NameViaTrustedRpc,
            1_800_000_000,
        )
        .expect("a name werust can key");
        let seen = pins
            .check("ronan.eth", &android)
            .expect("a name werust can key");
        assert!(
            seen.is_unchanged(),
            "the CIDv0 pin and the base32 CIDv1 of the SAME root are one content root"
        );
        assert!(!seen.is_changed(), "so nothing warns");
        assert!(
            !seen.is_blessable(),
            "and there is nothing left for the user to record"
        );

        // The mapping is symmetric (blessed on Android, seen on desktop) and
        // covers every multibase spelling of the same CIDv1, not just the two
        // werust happens to produce today.
        let mut mobile_first = TrustedNamePins::default();
        mobile_first
            .bless("ronan.eth", &android, TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        for spelling in [
            v0.clone(),
            android.clone(),
            v1.to_string_of_base(cid::multibase::Base::Base32Upper)
                .expect("an uppercase base32 CIDv1"),
            v1.to_string_of_base(cid::multibase::Base::Base58Btc)
                .expect("a base58btc CIDv1"),
        ] {
            let seen = mobile_first
                .check("ronan.eth", &spelling)
                .expect("a name werust can key");
            assert!(
                seen.is_unchanged() && !seen.is_changed(),
                "`{spelling}` names the blessed root, so it is not a change"
            );
        }
    }

    #[test]
    fn two_genuinely_different_roots_still_compare_unequal() {
        // The other half, and the one that must not be traded away: canonicalising
        // may not make two DIFFERENT roots equal. Including the pair a comparison
        // that gave up early would miss: identical but for the last character.
        let blessed = dag_pb_root(b"the version the user blessed");
        let unrelated = dag_pb_root(b"a DIFFERENT version, published later");
        let late = root_with_a_late_difference(&blessed.into_v1().expect("a CIDv1"));
        let blessed_v1 = blessed.into_v1().expect("a CIDv1").to_string();
        let late = late.to_string();
        let shared = blessed_v1
            .chars()
            .zip(late.chars())
            .take_while(|(a, b)| a == b)
            .count();
        assert!(
            shared > blessed_v1.len() - 4 && blessed_v1 != late,
            "the late-difference pair shares all but the tail: {blessed_v1} vs {late}"
        );

        let mut pins = TrustedNamePins::default();
        pins.bless(
            "ronan.eth",
            &blessed.to_string(),
            TrustPosture::NameViaTrustedRpc,
            1,
        )
        .expect("a name werust can key");
        for changed in [unrelated.to_string(), late] {
            let seen = pins
                .check("ronan.eth", &changed)
                .expect("a name werust can key");
            assert!(
                seen.is_changed() && !seen.is_unchanged(),
                "`{changed}` is a different root, so it still warns"
            );
            assert!(seen.is_blessable(), "the user can look, then accept it");
        }
    }

    #[test]
    fn a_cid_werust_cannot_parse_is_compared_literally_and_never_canonicalised() {
        // The stated rule for a string that is not a CID at all (decisions doc at
        // `docs/spikes/trust-store-compares-cids-by-canonical-form/DECISIONS.md`):
        // it is compared BYTE FOR BYTE, exactly as before this change, and never
        // canonicalised. So canonicalisation can never invent an equality between
        // two spellings werust could not read, and a root werust cannot parse is
        // not condemned to a warning no re-bless can ever clear. Never a panic,
        // whatever is in the file.
        let real = dag_pb_root(b"a real root").to_string();
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "not-a-cid", TrustPosture::MutableName, 1)
            .expect("a name werust can key");

        // Two DIFFERENT unreadable strings are never equal, however alike.
        for current in [
            "not-a-cid-either",
            "not-a-cid ",
            "NOT-A-CID",
            "not-a-ci",
            "",
            &real,
        ] {
            let seen = pins
                .check("ronan.eth", current)
                .expect("a name werust can key");
            assert!(
                seen.is_changed() && !seen.is_unchanged(),
                "`{current}` is not the recorded string, so it is a change"
            );
        }
        // The SAME unreadable string is the same string: the rule this change
        // inherits untouched, so nobody who blessed a root werust cannot parse
        // starts seeing a warning that re-blessing cannot clear.
        let seen = pins
            .check("ronan.eth", "not-a-cid")
            .expect("a name werust can key");
        assert!(seen.is_unchanged() && !seen.is_changed());

        // And a REAL root is never equal to an unreadable one, in either position.
        let mut real_pins = TrustedNamePins::default();
        real_pins
            .bless("ronan.eth", &real, TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert!(real_pins
            .check("ronan.eth", "not-a-cid")
            .expect("a name werust can key")
            .is_changed());
        assert!(!real_pins
            .check("ronan.eth", "")
            .expect("a name werust can key")
            .is_unchanged());
    }

    #[test]
    fn a_pin_recorded_under_the_old_rule_still_matches_and_is_never_rewritten() {
        // The migration question, answered by NOT having one: canonicalisation
        // happens at COMPARISON time, so the store keeps the spelling it was given
        // and a pin recorded by an earlier build matches the moment this build
        // runs: no rewrite, no upgrade step, and nobody loses a warning to this
        // task. Driven through the directory-taking cores against a scratch dir,
        // with the developer's own store asserted untouched.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("old-rule");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let root = dag_pb_root(b"the version the user blessed");
        let recorded = root.to_string();
        let today = root.into_v1().expect("a CIDv1").to_string();

        // Exactly what a build BEFORE this change wrote: the CID verbatim, in
        // whatever form the contenthash decoder produced.
        std::fs::write(
            scratch.path.join(PINS_FILE),
            format!(
                r#"{{"pins":[{{"name":"ronan.eth","cid":"{recorded}","blessedAt":1800000000,"posture":"name-via-trusted-rpc"}}]}}"#
            ),
        )
        .unwrap();

        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        assert!(
            pins.check("ronan.eth", &today)
                .expect("a name werust can key")
                .is_unchanged(),
            "the pin written under the OLD rule matches the form seen today"
        );
        assert!(
            pins.check("ronan.eth", &recorded)
                .expect("a name werust can key")
                .is_unchanged(),
            "and still matches its own form, exactly as it always did"
        );

        // A read-modify-write (blessing some OTHER name) leaves the recorded
        // spelling byte for byte: werust records what it was given.
        pins.bless("stranger.eth", &today, TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let document = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        assert!(
            document.contains(&recorded),
            "the recorded CID is kept VERBATIM, never canonicalised on the way out: {document}"
        );
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path)
                .expect("a readable store")
                .get("ronan.eth")
                .map(|pin| pin.cid.clone()),
            Some(recorded),
            "and reads back unchanged"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_missing_store_is_empty_but_an_unreadable_one_cannot_determine_trust() {
        // The INVERSION of the old
        // `a_missing_or_corrupt_store_degrades_to_no_pins_never_to_a_broken_load`
        // (task `trust-store-fails-closed-instead-of-reading-as-nothing-trusted`,
        // `docs/adr/0014`): a store werust cannot read is NO LONGER "nothing
        // blessed", because reading it as an empty store and then writing into
        // that apparent emptiness is how one transient failure destroys every
        // record. What still holds: no panic, no invented pin, and a MISSING file
        // is an EMPTY store (a fresh install is not an error).
        let scratch = ScratchDir::new("undeterminable");
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(TrustedNamePins::default()),
            "no settings directory at all is a fresh install, not a failure"
        );
        std::fs::create_dir_all(&scratch.path).unwrap();
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(TrustedNamePins::default()),
            "a directory with no `pins.json` is a fresh install too"
        );

        // A document that is not this store's wire form at all.
        for bad in [
            "not json {",
            "",
            "[]",
            "{}",
            r#"{"pins":"nope"}"#,
            r#"{"pins":{}}"#,
            // TRUNCATED mid-document: the shape a killed or full-disk write leaves.
            r#"{"pins":[{"name":"a.eth","cid":"baf"#,
        ] {
            std::fs::write(scratch.path.join(PINS_FILE), bad).unwrap();
            let why =
                TrustedNamePins::load_from(&scratch.path).expect_err("not a store werust can read");
            assert!(
                matches!(why, UndeterminableTrust::Unparseable(_)),
                "`{bad}` is unparseable, got {why:?}"
            );
            assert!(!why.to_string().is_empty(), "the reason is legible");
        }

        // Well-formed JSON whose ENTRIES are unreadable: no cid, no timestamp, an
        // unknown posture spelling, an empty name. Each REPORTS rather than
        // quietly shrinking the file, so a future fifth trust posture cannot
        // un-trust every name recorded under it.
        for bad in [
            r#"{"pins":[{"name":"a.eth","blessedAt":1,"posture":"mutable-name"}]}"#,
            r#"{"pins":[{"name":"a.eth","cid":"bafy","posture":"mutable-name"}]}"#,
            r#"{"pins":[{"name":"a.eth","cid":"bafy","blessedAt":1}]}"#,
            r#"{"pins":[{"name":"a.eth","cid":"bafy","blessedAt":1,"posture":"totally-trusted"}]}"#,
            r#"{"pins":[{"name":"  ","cid":"bafy","blessedAt":1,"posture":"mutable-name"}]}"#,
        ] {
            std::fs::write(scratch.path.join(PINS_FILE), bad).unwrap();
            let why = TrustedNamePins::load_from(&scratch.path)
                .expect_err("an entry werust cannot read honestly");
            assert!(
                matches!(why, UndeterminableTrust::UnreadableEntry(_)),
                "`{bad}` is an unreadable entry, got {why:?}"
            );
            assert!(!why.to_string().is_empty(), "the reason is legible");
        }

        // Two entries for ONE key: reported, never silently resolved to whichever
        // one happened to sort first.
        std::fs::write(
            scratch.path.join(PINS_FILE),
            r#"{"pins":[
                {"name":"a.eth","cid":"bafyone","blessedAt":1,"posture":"mutable-name"},
                {"name":"A.eth","cid":"bafytwo","blessedAt":2,"posture":"mutable-name"}
            ]}"#,
        )
        .unwrap();
        let why = TrustedNamePins::load_from(&scratch.path).expect_err("two entries, one name");
        assert!(
            matches!(&why, UndeterminableTrust::DuplicateName(name) if name == "a.eth"),
            "a duplicate key is reported, got {why:?}"
        );

        // A file that EXISTS and cannot be read at all (here: a directory where
        // the document should be) is undeterminable too, not a fresh install.
        std::fs::remove_file(scratch.path.join(PINS_FILE)).unwrap();
        std::fs::create_dir_all(scratch.path.join(PINS_FILE)).unwrap();
        assert!(
            matches!(
                TrustedNamePins::load_from(&scratch.path),
                Err(UndeterminableTrust::Unreadable(_))
            ),
            "an unreadable file is not an empty store"
        );
    }

    #[test]
    fn no_write_reaches_a_store_werust_cannot_read() {
        // The half a naive fix forgets, and the exploitable one: inserting into
        // what merely LOOKS like an empty store is how ONE transient read failure
        // permanently replaces every record with a single fresh one. So the write
        // REFUSES while trust cannot be determined, and the bytes on disk are
        // still there afterwards, byte for byte (`docs/adr/0014`).
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("refuse-write");
        std::fs::create_dir_all(&scratch.path).unwrap();
        // A store recorded by a build that knows a posture this one does not: the
        // records are real, werust simply cannot read them honestly.
        let corrupt = br#"{"pins":[{"name":"ronan.eth","cid":"bafyold","blessedAt":1,"posture":"from-the-future"}]}"#;
        std::fs::write(scratch.path.join(PINS_FILE), corrupt).unwrap();

        let mut pins = TrustedNamePins::default();
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        assert!(
            !pins.save_to(&scratch.path).is_recorded(),
            "the write refuses while the store cannot be read"
        );
        assert_eq!(
            std::fs::read(scratch.path.join(PINS_FILE)).unwrap(),
            corrupt,
            "the records werust could not read are still on disk, byte for byte"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );

        // And the refusal is SPECIFIC to that state: once the document is readable
        // again the very same save lands.
        std::fs::write(scratch.path.join(PINS_FILE), r#"{"pins":[]}"#).unwrap();
        assert!(pins.save_to(&scratch.path).is_recorded());
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(as_read_back(&pins)),
            "a readable store still round-trips"
        );
    }

    #[test]
    fn the_json_wire_form_round_trips_every_posture_in_the_shared_vocabulary() {
        // The store speaks the ONE posture vocabulary (`docs/adr/0006`) the chrome
        // JSON and the debug view already use, so a persisted posture cannot be a
        // second spelling. Driven over `TrustPosture::ALL`, which a compile-time
        // check keeps complete: a fifth posture cannot land unreadable here.
        let mut pins = TrustedNamePins::default();
        for (i, posture) in TrustPosture::ALL.into_iter().enumerate() {
            pins.bless(
                &format!("name{i}.eth"),
                &format!("bafy{i}"),
                posture,
                i as u64,
            )
            .expect("a name werust can key");
        }
        let round_tripped = TrustedNamePins::from_json(&pins.to_json()).expect("a readable store");
        assert_eq!(round_tripped, as_read_back(&pins));
        assert_eq!(round_tripped.len(), TrustPosture::ALL.len());

        // The document is STABLE: an unchanged store re-serializes byte-identically
        // (the pins are kept sorted), so a save with nothing new rewrites nothing new.
        assert_eq!(round_tripped.to_json(), pins.to_json());
    }

    #[test]
    fn saving_without_a_directory_is_a_refusal_not_a_panic() {
        // No settings directory is an in-memory interim (the bless holds for this
        // session but cannot be recorded), exactly as the retrieval settings do.
        // It reports [`PinSaveOutcome::CouldNotPersist`] and NOT the taxonomy's
        // `Refused`, which is reserved for the one POLICY refusal: a store werust
        // cannot read.
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        let outcome = pins.save_to(std::path::Path::new(""));
        assert!(!outcome.is_recorded());
        assert!(
            matches!(outcome, PinSaveOutcome::CouldNotPersist(_)),
            "nowhere to write is not a panic and not a policy refusal: {outcome:?}"
        );
    }

    #[test]
    fn a_save_reaches_the_live_document_only_by_renaming_a_sibling_temp_file() {
        // Acceptance: the write is ATOMIC. A reader observes the OLD document or
        // the NEW one and never a truncated one, which is exactly what
        // temp-file-plus-rename buys, and the temp file must be a SIBLING,
        // because a rename ACROSS directories is not atomic at all (it degrades to
        // a copy). The ordering is asserted from INSIDE the write step, which is
        // the same seam the interrupted-write test below fails, so production code
        // needs no test-only branch (this repo has no `cfg!(test)` branch left and
        // is not growing one).
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("atomic");
        let live = scratch.path.join(PINS_FILE);

        let mut previous = TrustedNamePins::default();
        previous
            .bless("ronan.eth", "bafyold", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert_eq!(previous.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let old_bytes = std::fs::read(&live).expect("the previous document");

        let mut next = previous.clone();
        next.bless(
            "stranger.eth",
            "bafynew",
            TrustPosture::NameViaTrustedRpc,
            2,
        )
        .expect("a name werust can key");
        let outcome = next.save_to_through(&scratch.path, |temp, document| {
            assert_eq!(
                temp.parent(),
                Some(scratch.path.as_path()),
                "the temp file is a SIBLING of the store: a cross-directory rename is not atomic"
            );
            assert_ne!(
                temp,
                live.as_path(),
                "the live document is never written in place"
            );
            assert_eq!(
                std::fs::read(&live).expect("the previous document"),
                old_bytes,
                "nothing reaches the live document before the rename"
            );
            std::fs::write(temp, document)
        });

        assert_eq!(outcome, PinSaveOutcome::Recorded);
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(as_read_back(&next)),
            "the renamed document IS the store"
        );
        assert_eq!(
            dir_entries(&scratch.path),
            vec![PINS_FILE.to_string()],
            "no temp file survives a successful save"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_write_interrupted_mid_document_leaves_the_previous_store_intact_and_no_temp_behind() {
        // The day it matters: a full disk, an OOM kill or a power cut between the
        // first byte and the last. With a bare whole-file write that leaves a
        // TRUNCATED `pins.json`, the worst possible state for a record the
        // browser is about to trust, and (since the third state landed) one that
        // blocks every later write too. With temp-plus-rename the half-written
        // bytes are in the temp file, the live document is untouched, and the temp
        // file does not survive.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("interrupted");
        let live = scratch.path.join(PINS_FILE);

        let mut previous = TrustedNamePins::default();
        previous
            .bless("ronan.eth", "bafyold", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert_eq!(previous.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let old_bytes = std::fs::read(&live).expect("the previous document");

        let mut next = previous.clone();
        next.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        let outcome = next.save_to_through(&scratch.path, |temp, document| {
            std::fs::write(temp, &document.as_bytes()[..document.len() / 2])?;
            Err(std::io::Error::other("the disk filled up mid-write"))
        });

        assert!(
            matches!(outcome, PinSaveOutcome::CouldNotPersist(_)),
            "a failed write is reported, never swallowed: {outcome:?}"
        );
        assert!(
            outcome
                .problem()
                .is_some_and(|why| why.contains("the disk filled up mid-write")),
            "and it carries WHY: {outcome:?}"
        );
        assert_eq!(
            std::fs::read(&live).expect("the previous document"),
            old_bytes,
            "the PREVIOUS document is intact, byte for byte"
        );
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(as_read_back(&previous)),
            "and still readable, so the next write is not blocked either"
        );
        assert_eq!(
            dir_entries(&scratch.path),
            vec![PINS_FILE.to_string()],
            "the half-written temp file is cleaned up on the failure path"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn members_this_build_does_not_know_survive_a_read_modify_write() {
        // Two werust versions are two processes (this module's docs contemplate it
        // and the store's whole shape follows from it), so an OLDER build must
        // never silently strip what a NEWER one wrote, at BOTH levels the
        // document has: the document itself, and each entry.
        //
        // The document-level fixture member is deliberately one NO build will
        // ever own. It used to be `normalizationVersion`, which the very next
        // task in this chain made REAL: the moment it did, this test stopped
        // asserting what it says, because its example of "a member this build
        // does not know" was one this build knows. A fixture whose whole job is
        // to be unknown must not be a name anybody would plausibly implement.
        let scratch = ScratchDir::new("unknown-members");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let written_by_a_later_build = r#"{
            "pins": [
                {
                    "name": "ronan.eth",
                    "cid": "bafyold",
                    "blessedAt": 1,
                    "posture": "mutable-name",
                    "retainedContent": {"path": "blobs/abc", "bytes": 1234}
                }
            ],
            "somethingNoWerustWillEverImplement": {"kind": "a later build's own", "count": 2},
            "writtenBy": "werust 9.9.9"
        }"#;
        std::fs::write(scratch.path.join(PINS_FILE), written_by_a_later_build).unwrap();

        // Exactly the read-modify-write a bless performs: load the document, record
        // into it, save it back.
        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        pins.bless("ronan.eth", "bafynewer", TrustPosture::NameViaTrustedRpc, 3)
            .expect("a name werust can key");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);

        let text = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        let document: Value = serde_json::from_str(&text).expect("still one JSON document");
        assert_eq!(
            document["somethingNoWerustWillEverImplement"],
            json!({"kind": "a later build's own", "count": 2}),
            "an unknown TOP-LEVEL member survives, unchanged: {text}"
        );
        assert_eq!(document["writtenBy"], json!("werust 9.9.9"));
        // The other side of the same rule: a member werust DOES own is werust's
        // to write, and this save is the one that wrote the file.
        assert_eq!(
            document[NORMALIZATION_VERSION_MEMBER],
            json!(NORMALIZATION_VERSION),
            "the stamp names the normalization that wrote THIS document: {text}"
        );

        let entries = document["pins"].as_array().expect("the `pins` array");
        assert_eq!(entries.len(), 2);
        let ronan = entries
            .iter()
            .find(|entry| entry["name"] == json!("ronan.eth"))
            .expect("the re-blessed entry");
        assert_eq!(
            ronan["retainedContent"],
            json!({"path": "blobs/abc", "bytes": 1234}),
            "an unknown ENTRY member survives, unchanged, even across a RE-bless: {text}"
        );
        // And werust still owns the spelling of the fields werust owns: the shared
        // posture vocabulary, not a second one.
        assert_eq!(ronan["cid"], json!("bafynewer"));
        assert_eq!(ronan["blessedAt"], json!(3));
        assert_eq!(ronan["posture"], json!("name-via-trusted-rpc"));
        let stranger = entries
            .iter()
            .find(|entry| entry["name"] == json!("stranger.eth"))
            .expect("the newly blessed entry");
        assert_eq!(
            stranger.as_object().expect("an entry is an object").len(),
            4,
            "a NEW entry invents no members: {text}"
        );

        // The residue is READ back too, so it survives any number of round trips
        // rather than only the first.
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path),
            Ok(as_read_back(&pins))
        );
    }

    #[test]
    fn the_store_records_which_normalization_version_wrote_it() {
        // Acceptance: the persisted document records the normalization that wrote
        // it, so a mapping change is DETECTABLE from the file rather than inferred
        // from whichever build somebody remembers running. The keys in a
        // `pins.json` are a LIBRARY's output (`pin_key` -> `ens-normalize`), and
        // nothing else in the file says which library produced them.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("stamp");
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafyone", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert_eq!(
            pins.normalization_version(),
            None,
            "a store built in memory has been read from no document, so it reports no stamp"
        );
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);

        let text = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        let document: Value = serde_json::from_str(&text).expect("one JSON document");
        assert_eq!(
            document[NORMALIZATION_VERSION_MEMBER],
            json!(NORMALIZATION_VERSION),
            "the document names the normalization that wrote it: {text}"
        );
        assert_eq!(
            NORMALIZATION_VERSION_MEMBER, "normalizationVersion",
            "the member name is a WIRE contract: an older build carries it by name"
        );

        let reloaded = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        assert_eq!(
            reloaded.normalization_version(),
            Some(NORMALIZATION_VERSION)
        );
        assert_eq!(
            reloaded.normalization_version_mismatch(),
            None,
            "the version that wrote it IS this build's: nothing to report"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_store_with_no_stamp_still_loads_because_it_predates_stamping() {
        // Acceptance: the stamp must not invalidate what is already on disk. Every
        // `pins.json` written before this change carries no `normalizationVersion`
        // at all, and "written before stamping existed" is a fact, not a fault: it
        // loads, its pins are read, and it is not a mismatch either (there is
        // nothing to mismatch WITH). Treating it as corrupt would be this task
        // destroying the very records it exists to protect.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("unstamped");
        std::fs::create_dir_all(&scratch.path).unwrap();
        std::fs::write(
            scratch.path.join(PINS_FILE),
            r#"{"pins":[{"name":"ronan.eth","cid":"bafyold","blessedAt":1800000000,"posture":"name-via-trusted-rpc"}]}"#,
        )
        .unwrap();

        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("an unstamped store reads");
        assert_eq!(pins.len(), 1, "and its records are all there");
        assert_eq!(pins.normalization_version(), None, "it predates stamping");
        assert_eq!(
            pins.normalization_version_mismatch(),
            None,
            "no stamp is not a MISMATCH: there is nothing recorded to disagree with"
        );
        assert!(
            pins.check("ronan.eth", "bafyold")
                .expect("a name werust can key")
                .is_unchanged(),
            "and it answers exactly as it always did"
        );

        // The next ordinary write stamps it: nothing is migrated, the file simply
        // records the build that last wrote it.
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);
        assert_eq!(
            TrustedNamePins::load_from(&scratch.path)
                .expect("a readable store")
                .normalization_version(),
            Some(NORMALIZATION_VERSION)
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_mismatched_stamp_is_recorded_and_readable_and_changes_nothing_else() {
        // The stated decision, asserted so the CODE matches the statement (the
        // module's stamp note): a stamp naming a normalization that is not this
        // build's is RECORDED and READABLE, and nothing acts on it. It does not
        // make the store undeterminable, does not refuse a write, does not re-key
        // and does not change a single answer the store gives. Acting on it
        // silently would be the trust reset the corpus + stamp exist to prevent;
        // reporting it is a surface's job, and there is no trust-management
        // surface yet.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("stamp-mismatch");
        std::fs::create_dir_all(&scratch.path).unwrap();
        std::fs::write(
            scratch.path.join(PINS_FILE),
            r#"{"pins":[{"name":"ronan.eth","cid":"bafyold","blessedAt":1800000000,"posture":"name-via-trusted-rpc"}],
                "normalizationVersion":"ens-normalize 0.0.1"}"#,
        )
        .unwrap();

        let mut pins = TrustedNamePins::load_from(&scratch.path)
            .expect("a store another normalization wrote is READABLE, not corrupt");
        assert_eq!(
            pins.normalization_version(),
            Some("ens-normalize 0.0.1"),
            "what wrote it is readable from the file"
        );
        assert_eq!(
            pins.normalization_version_mismatch(),
            Some("ens-normalize 0.0.1"),
            "and it is legible AS a mismatch, without anybody deriving it twice"
        );
        assert_eq!(pins.len(), 1, "every record is still read");
        assert!(
            pins.check("ronan.eth", "bafyold")
                .expect("a name werust can key")
                .is_unchanged(),
            "and the store's answers are untouched by the mismatch"
        );

        // The write is not refused either, and the document it leaves names the
        // build that wrote it — this one.
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let reloaded = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        assert_eq!(
            reloaded.normalization_version(),
            Some(NORMALIZATION_VERSION)
        );
        assert_eq!(
            reloaded.normalization_version_mismatch(),
            None,
            "the stamp follows the WRITER, so it stops disagreeing once this build has written"
        );
        assert_eq!(reloaded.len(), 2);
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_stamp_survives_a_rewrite_by_a_build_that_does_not_know_it() {
        // Acceptance: an OLDER werust rewriting the store must not STRIP the stamp
        // a newer one wrote — the same two-versions-as-two-processes rule the
        // unknown-member preservation follows. That older build cannot be linked
        // into this test, so what is asserted is the MECHANISM it applies, in the
        // two halves the claim rests on:
        //
        // 1. What a build carries is decided by ONE thing, its known-member set.
        //    This change added exactly ONE member to it, so every werust before it
        //    knew only `pins` — to all of them the stamp is an unknown member.
        // 2. An unknown top-level member survives a read-modify-write untouched.
        //    Exercised here on a member shaped exactly like the stamp (a plain
        //    top-level string this build does not know), through the very carrier
        //    an older build would run
        //    (`trust-store-writes-atomically-and-keeps-fields-it-does-not-know`).
        assert_eq!(
            KNOWN_DOCUMENT_MEMBERS,
            ["pins", NORMALIZATION_VERSION_MEMBER],
            "the stamp is the ONE document member this change added, so every earlier build \
             treats it as unknown and therefore carries it"
        );
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("stamp-carried");
        std::fs::create_dir_all(&scratch.path).unwrap();
        // A stamp-SHAPED member this build does not know: what `normalizationVersion`
        // itself looks like to a build written before it existed.
        std::fs::write(
            scratch.path.join(PINS_FILE),
            r#"{"pins":[{"name":"ronan.eth","cid":"bafyold","blessedAt":1,"posture":"mutable-name"}],
                "someLaterBuildsVersionStamp":"a-normalizer 9.9.9"}"#,
        )
        .unwrap();

        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2)
            .expect("a name werust can key");
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);

        let text = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        let document: Value = serde_json::from_str(&text).expect("one JSON document");
        assert_eq!(
            document["someLaterBuildsVersionStamp"],
            json!("a-normalizer 9.9.9"),
            "a version stamp this build does not know rides the carrier untouched: {text}"
        );
        // And the stamp really is a member of exactly that class: one plain
        // top-level string, no nesting and no sidecar, so there is nothing about
        // it an older reader could fail to carry.
        assert!(
            document[NORMALIZATION_VERSION_MEMBER].is_string(),
            "the stamp is a plain top-level string member: {text}"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn a_stamp_that_is_not_a_string_is_reported_rather_than_silently_replaced() {
        // A member werust OWNS, carrying a value werust cannot read, is a document
        // that is not this wire form — the same answer a `pins` member that is not
        // an array gets. It matters because this build would otherwise overwrite
        // that value with its own stamp on the next save: reporting it keeps the
        // store's rule (report, never silently shrink) whole, and the write
        // refuses while it holds, so the bytes stay on disk for whoever looks.
        let real_before = real_pin_store_snapshot();
        let scratch = ScratchDir::new("stamp-not-a-string");
        std::fs::create_dir_all(&scratch.path).unwrap();
        let odd = br#"{"pins":[],"normalizationVersion":15}"#;
        std::fs::write(scratch.path.join(PINS_FILE), odd).unwrap();

        let why = TrustedNamePins::load_from(&scratch.path).expect_err("not this wire form");
        assert!(
            matches!(&why, UndeterminableTrust::Unparseable(detail)
                if detail.contains(NORMALIZATION_VERSION_MEMBER)),
            "it says WHICH member it could not read: {why:?}"
        );
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafynew", TrustPosture::MutableName, 1)
            .expect("a name werust can key");
        assert!(matches!(
            pins.save_to(&scratch.path),
            PinSaveOutcome::Refused(_)
        ));
        assert_eq!(
            std::fs::read(scratch.path.join(PINS_FILE)).unwrap(),
            odd,
            "and nothing is overwritten while it holds"
        );
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );
    }

    #[test]
    fn the_save_outcome_tells_recorded_from_could_not_persist_from_refused() {
        // "There was nothing to record", "I could not write it" and "I refuse to
        // write over a store I cannot read" are three different sentences, and
        // only the last two are worth showing anyone. A bare boolean cannot tell
        // them apart, so the surface that eventually says one of them would have
        // been blocked on a plumbing change; it is not.
        let scratch = ScratchDir::new("outcomes");
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1)
            .expect("a name werust can key");

        let recorded = pins.save_to(&scratch.path);
        assert_eq!(recorded, PinSaveOutcome::Recorded);
        assert!(recorded.is_recorded());
        assert_eq!(recorded.problem(), None, "a save that landed says nothing");

        // A store werust cannot read: REFUSED, carrying WHY, and nothing written
        // (`docs/adr/0014`): distinguishable now, rather than the same `false` a
        // full disk produces.
        let corrupt = br#"{"pins":[{"name":"a.eth","cid":"bafy","blessedAt":1,"posture":"from-the-future"}]}"#;
        std::fs::write(scratch.path.join(PINS_FILE), corrupt).unwrap();
        let refused = pins.save_to(&scratch.path);
        assert!(
            matches!(
                refused,
                PinSaveOutcome::Refused(UndeterminableTrust::UnreadableEntry(_))
            ),
            "the refusal names the store's third state: {refused:?}"
        );
        assert!(!refused.is_recorded());
        assert!(refused.problem().is_some_and(|why| why.contains(PINS_FILE)));
        assert_eq!(
            std::fs::read(scratch.path.join(PINS_FILE)).unwrap(),
            corrupt,
            "a refusal touches nothing"
        );

        // The fourth answer is the CALLER's, not the file's: there was nothing to
        // record at all (no mutable name, or a name already blessed at exactly this
        // CID). Uninteresting, and never a problem to put in front of anyone.
        assert!(!PinSaveOutcome::NothingToRecord.is_recorded());
        assert_eq!(PinSaveOutcome::NothingToRecord.problem(), None);
    }

    #[test]
    fn the_blessed_date_is_a_legible_calendar_day() {
        // The `<date>` the warning quotes back to the user.
        assert_eq!(format_utc_date(0), "1970-01-01");
        assert_eq!(format_utc_date(1_800_000_000), "2027-01-15");
        // A leap day and the day after, and a century non-leap boundary.
        assert_eq!(format_utc_date(1_709_164_800), "2024-02-29");
        assert_eq!(format_utc_date(1_709_251_200), "2024-03-01");
        // 2000 IS a leap year (divisible by 400), the case a naive rule gets wrong.
        assert_eq!(format_utc_date(951_782_400), "2000-02-29");
    }

    #[test]
    fn the_calendar_conversion_walks_every_day_for_four_centuries_without_a_gap() {
        // The closed-form conversion is arithmetic, so it is checked exhaustively
        // rather than argued: walk 1970..2370 day by day and assert the sequence
        // is a real Gregorian calendar (months in range, days in range, each day
        // exactly one after the last, leap years where the rule says).
        let (mut y, mut m, mut d) = (1970i64, 1u32, 1u32);
        for day in 0..146_097i64 {
            let (year, month, dom) = civil_from_days(day);
            assert_eq!(
                (year, month, dom),
                (y, m, d),
                "day {day} broke the sequence"
            );
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let last = match m {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                _ if leap => 29,
                _ => 28,
            };
            if d == last {
                d = 1;
                if m == 12 {
                    m = 1;
                    y += 1;
                } else {
                    m += 1;
                }
            } else {
                d += 1;
            }
        }
    }

    #[test]
    fn now_is_a_plausible_present_day_timestamp() {
        // The bless stamp comes from the system clock; assert only that it is a
        // sane epoch second (not zero, not a millisecond value), so a unit mix-up
        // cannot silently record dates in the year 58000.
        let now = now_unix_secs();
        assert!(now > 1_700_000_000, "not before 2023: {now}");
        assert!(now < 4_000_000_000, "not a millisecond value: {now}");
    }
}
