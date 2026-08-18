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
//! # Vocabulary note: "pin"
//!
//! `pin` is already used loosely in this crate for "held in place" (the shell
//! PINS a `.eth` name in the URL bar, a `pinned_root_key`, a pinned record
//! source). The TOFU sense is a DIFFERENT, durable thing, so it is always spelled
//! out as a **trusted name pin** ([`TrustedNamePin`], [`TrustedNamePins`],
//! `pins.json`) and the verb for creating one is **bless**, never "pin". The
//! spelling is the settled decision 2's (`pins.json`); the discipline is so the
//! two senses cannot be confused at a call site.

use serde_json::{json, Value};

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

    /// Persist the pins to the settings directory, creating it if needed. Returns
    /// `false` when there is no settings directory, when the write REFUSES because
    /// the store on disk cannot be read (see [`save_to`](TrustedNamePins::save_to)),
    /// or when the write itself failed: the bless still took effect for THIS
    /// session, it just could not be recorded.
    pub fn save(&self) -> bool {
        match crate::retrieval::settings_dir() {
            Some(dir) => self.save_to(&dir),
            None => false,
        }
    }

    /// Persist the pins to a SPECIFIC directory (the directory-taking core
    /// [`save`](TrustedNamePins::save) delegates to), creating it if needed.
    ///
    /// # The write REFUSES while trust cannot be determined
    ///
    /// This is the write half of the fail-closed rule (`docs/adr/0014`), and it
    /// lives HERE rather than at each caller so no writer — today's
    /// `bless_current_name`, or a later "forget this pin" — can forget it: the
    /// save re-reads the document it is about to replace, and returns `false`
    /// without touching a byte when that read is
    /// [`UndeterminableTrust`]. Overwriting a store werust could not read is how
    /// ONE transient failure permanently destroys every record it holds, and the
    /// whole-file write below is exactly the mechanism that would do it.
    ///
    /// The refusal is reported the same way "there is no settings directory" is:
    /// `false`, meaning the bless holds for THIS session but could not be
    /// recorded. It is never an error, because a store werust cannot read must not
    /// break browsing.
    pub fn save_to(&self, dir: &std::path::Path) -> bool {
        if dir.as_os_str().is_empty() || std::fs::create_dir_all(dir).is_err() {
            return false;
        }
        if Self::load_from(dir).is_err() {
            return false;
        }
        std::fs::write(dir.join(PINS_FILE), self.to_json()).is_ok()
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
    /// `{"pins":[{"name":…,"cid":…,"blessedAt":…,"posture":"<wire name>"}]}`.
    ///
    /// The posture uses the ONE shared wire vocabulary
    /// ([`trust_posture_wire_name`]) the chrome JSON and the debug view's Network
    /// tab already speak (`docs/adr/0006`), so the store never mints a second
    /// spelling of a posture.
    #[must_use]
    pub fn to_json(&self) -> String {
        let pins: Vec<Value> = self
            .pins
            .iter()
            .map(|pin| {
                json!({
                    "name": pin.name,
                    "cid": pin.cid,
                    "blessedAt": pin.blessed_at,
                    "posture": trust_posture_wire_name(pin.posture),
                })
            })
            .collect();
        json!({ "pins": pins }).to_string()
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
        let mut pins: Vec<TrustedNamePin> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| read_entry(index, entry))
            .collect::<Result<_, _>>()?;
        pins.sort_by(|a, b| a.name.cmp(&b.name));
        if let Some(duplicate) = pins.windows(2).find(|pair| pair[0].name == pair[1].name) {
            return Err(UndeterminableTrust::DuplicateName(
                duplicate[0].name.clone(),
            ));
        }
        Ok(Self { pins })
    }
}

/// One persisted entry -> one [`TrustedNamePin`], or WHY werust cannot read it
/// honestly. The entry's position is carried in the reason because the name is
/// exactly the field that may be missing.
fn read_entry(index: usize, entry: &Value) -> Result<TrustedNamePin, UndeterminableTrust> {
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
    Ok(TrustedNamePin {
        name,
        cid,
        blessed_at,
        posture,
    })
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
        assert!(pins.save_to(&scratch.path));
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
        assert!(pins.save_to(&scratch.path));
        assert_eq!(
            real_pin_store_snapshot(),
            real_before,
            "the developer's own `pins.json` is never written by this suite"
        );

        let written: Vec<String> = std::fs::read_dir(&scratch.path)
            .expect("the scratch dir exists")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(written, vec![PINS_FILE.to_string()]);

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
            !pins.save_to(&scratch.path),
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
        assert!(pins.save_to(&scratch.path));
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
        let mut pins = TrustedNamePins::default();
        pins.bless("ronan.eth", "bafy", TrustPosture::MutableName, 1);
        assert!(!pins.save_to(std::path::Path::new("")));
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
