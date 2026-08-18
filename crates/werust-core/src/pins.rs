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

use renderer::TrustPosture;

use crate::debug::{trust_posture_from_wire_name, trust_posture_wire_name};

/// The pin-store file name, under the SAME settings directory
/// [`retrieval::settings_dir`](crate::retrieval::settings_dir) resolves (settled
/// decision 2: `pins.json` lives NEXT TO `retrieval.json`, one mechanism, one
/// `WERUST_SETTINGS_DIR` lever, not a second location).
pub const PINS_FILE: &str = "pins.json";

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
    /// The mutable name, in the store's canonical (lower-cased, trimmed) key
    /// form; see [`pin_key`].
    pub name: String,
    /// The CID the name resolved to when it was blessed.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutableNameTrust {
    /// The mutable name the user sees in the URL bar for this site (the ROOT
    /// name, e.g. `ronan.eth`, never a sub-path display): the identity a pin is
    /// keyed on, so `ronan.eth/blog/` and `ronan.eth` share one pin.
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
        self.blessed.as_ref().is_some_and(|pin| pin.cid != self.cid)
    }

    /// Whether the name is blessed AND still resolves to the blessed CID.
    #[must_use]
    pub fn is_unchanged(&self) -> bool {
        self.blessed.as_ref().is_some_and(|pin| pin.cid == self.cid)
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
        }
    }
}

// ---------------------------------------------------------------------------
// The store.
// ---------------------------------------------------------------------------

/// The canonical store key for a mutable name: trimmed and lower-cased.
///
/// ENS names are case-insensitive (ENSIP-1 normalization lower-cases them before
/// the namehash), so `Ronan.eth` and `ronan.eth` are ONE name and must not be two
/// pins: a second pin under a different casing would silently make the warning
/// miss, which is the one failure mode a TOFU store cannot have.
#[must_use]
pub fn pin_key(name: &str) -> String {
    name.trim().to_lowercase()
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

    /// The pin for `name`, or `None` when it has never been blessed. Looked up by
    /// [`pin_key`], so casing cannot split one name across two pins.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&TrustedNamePin> {
        let key = pin_key(name);
        self.pins.iter().find(|pin| pin.name == key)
    }

    /// Record (or RE-record) `name`'s current `cid` as blessed, with the posture
    /// werust was showing and the moment the user did it.
    ///
    /// Re-blessing a changed name REPLACES its pin: the SSH-host-key model's
    /// "I have looked at the change and I accept the new content". The store is
    /// therefore always at most one pin per name.
    pub fn bless(&mut self, name: &str, cid: &str, posture: TrustPosture, blessed_at: u64) {
        let pin = TrustedNamePin {
            name: pin_key(name),
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
    }

    /// The [`MutableNameTrust`] for a name resolving to `cid` right now: the
    /// chrome axis value, pairing the live identity with whatever is on file.
    ///
    /// This is the ONE place the store is consulted per load, so no presentation
    /// rule ever reads the filesystem.
    #[must_use]
    pub fn check(&self, name: &str, cid: &str) -> MutableNameTrust {
        MutableNameTrust {
            name: name.to_string(),
            cid: cid.to_string(),
            blessed: self.get(name).cloned(),
        }
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
    /// [`DuplicateName`](UndeterminableTrust::DuplicateName). Dropping any of them
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
        // Everything BESIDE `pins`: carried, not understood, and written back
        // untouched (the module's unknown-member note). `value` is an object here,
        // because a non-object has no `pins` member to have got this far.
        let unknown_document_members = value
            .as_object()
            .map(|document| {
                document
                    .iter()
                    .filter(|(member, _)| member.as_str() != "pins")
                    .map(|(member, carried)| (member.clone(), carried.clone()))
                    .collect()
            })
            .unwrap_or_default();
        Ok(Self {
            pins,
            unknown_document_members,
            unknown_entry_members,
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
    let name = pin_key(
        entry
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| unreadable("records no name"))?,
    );
    if name.is_empty() {
        return Err(unreadable("records an empty name"));
    }
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
        );
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
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1);
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
        pins.bless("ronan.eth", "bafyold", TrustPosture::MutableName, 10);
        pins.bless("ronan.eth", "bafynew", TrustPosture::NameViaTrustedRpc, 20);
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
        pins.bless("Ronan.ETH", "bafyone", TrustPosture::MutableName, 1);
        assert_eq!(pins.len(), 1);
        assert_eq!(
            pins.get(" ronan.eth ").map(|p| p.cid.as_str()),
            Some("bafyone")
        );
        assert!(pins.check("RONAN.eth", "bafyother").is_changed());
    }

    #[test]
    fn both_ens_and_ipns_style_names_are_blessable_and_checked_the_same_way() {
        // Acceptance (settled decision 3): the store keys on the NAME, whatever
        // kind it is: an `ipfs-ns` ENS name, an `ipns-ns` ENS name, or a bare
        // IPNS name. Both axes' names are controller-repointable, so both are
        // blessable and both warn identically.
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafyens", TrustPosture::NameViaTrustedRpc, 1);
        pins.bless("k51qzifixture", "bafyipns", TrustPosture::MutableName, 2);
        assert!(pins.check("ronan.eth", "bafyens").is_unchanged());
        assert!(pins.check("ronan.eth", "bafyelse").is_changed());
        assert!(pins.check("k51qzifixture", "bafyipns").is_unchanged());
        assert!(pins.check("k51qzifixture", "bafyelse").is_changed());
        // An unknown name is simply unblessed on either axis.
        assert!(!pins.check("stranger.eth", "bafyany").is_blessed());
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
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2);
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
            Ok(pins),
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
            );
        }
        let round_tripped = TrustedNamePins::from_json(&pins.to_json()).expect("a readable store");
        assert_eq!(round_tripped, pins);
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
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1);
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
        previous.bless("ronan.eth", "bafyold", TrustPosture::MutableName, 1);
        assert_eq!(previous.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let old_bytes = std::fs::read(&live).expect("the previous document");

        let mut next = previous.clone();
        next.bless(
            "stranger.eth",
            "bafynew",
            TrustPosture::NameViaTrustedRpc,
            2,
        );
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
            Ok(next),
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
        previous.bless("ronan.eth", "bafyold", TrustPosture::MutableName, 1);
        assert_eq!(previous.save_to(&scratch.path), PinSaveOutcome::Recorded);
        let old_bytes = std::fs::read(&live).expect("the previous document");

        let mut next = previous.clone();
        next.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2);
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
            Ok(previous),
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
            "normalizationVersion": "ensip15-2026",
            "writtenBy": "werust 9.9.9"
        }"#;
        std::fs::write(scratch.path.join(PINS_FILE), written_by_a_later_build).unwrap();

        // Exactly the read-modify-write a bless performs: load the document, record
        // into it, save it back.
        let mut pins = TrustedNamePins::load_from(&scratch.path).expect("a readable store");
        pins.bless("stranger.eth", "bafynew", TrustPosture::MutableName, 2);
        pins.bless("ronan.eth", "bafynewer", TrustPosture::NameViaTrustedRpc, 3);
        assert_eq!(pins.save_to(&scratch.path), PinSaveOutcome::Recorded);

        let text = std::fs::read_to_string(scratch.path.join(PINS_FILE)).expect("the store");
        let document: Value = serde_json::from_str(&text).expect("still one JSON document");
        assert_eq!(
            document["normalizationVersion"],
            json!("ensip15-2026"),
            "an unknown TOP-LEVEL member survives, unchanged: {text}"
        );
        assert_eq!(document["writtenBy"], json!("werust 9.9.9"));

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
        assert_eq!(TrustedNamePins::load_from(&scratch.path), Ok(pins));
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
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1);

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
