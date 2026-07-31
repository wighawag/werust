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
//! # Fail-safe
//!
//! The pin store is ADVISORY and one-directional: it can only make werust say
//! MORE, never less. An unblessed name behaves exactly as it did before; a
//! blessed-and-unchanged name behaves exactly as it did before; a blessed-then-
//! CHANGED name adds a louder warning. Nothing here authorises a load, relaxes a
//! verification, or feeds the retrieval path; a missing, unreadable or corrupt
//! `pins.json` therefore degrades to "no pins" (the pre-TOFU behaviour) rather
//! than failing a load, exactly as [`RetrievalSettings`](crate::retrieval::RetrievalSettings)
//! re-defaults. The blessed CID is never used to CHOOSE what to load either: the
//! name still resolves normally and the bytes are still hash-verified, so a pin
//! can never cause unverified content to render.
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
/// [`save`](TrustedNamePins::save), plus the directory-taking cores tests drive.
/// A missing or corrupt file loads as EMPTY rather than failing, because an
/// unreadable pin store must degrade to the pre-TOFU behaviour, never to a
/// broken browser (see the module's fail-safe note).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TrustedNamePins {
    /// The pins, kept sorted by [`pin_key`] so the persisted document is stable
    /// (a re-save with no change rewrites identical bytes).
    pins: Vec<TrustedNamePin>,
}

impl TrustedNamePins {
    /// Load the pins from the settings directory, or an EMPTY store if there is
    /// no directory, no file (nothing blessed yet), or the file is
    /// unreadable/corrupt.
    #[must_use]
    pub fn load() -> Self {
        match crate::retrieval::settings_dir() {
            Some(dir) => Self::load_from(&dir),
            None => Self::default(),
        }
    }

    /// Load the pins from a SPECIFIC directory (the directory-taking core
    /// [`load`](TrustedNamePins::load) delegates to).
    ///
    /// The explicit-directory seam, identical to
    /// [`RetrievalSettings::load_from`](crate::retrieval::RetrievalSettings::load_from):
    /// tests pass their OWN scratch directory so they isolate the store WITHOUT
    /// mutating process-global env, and the real `pins.json` is never touched.
    #[must_use]
    pub fn load_from(dir: &std::path::Path) -> Self {
        let Ok(text) = std::fs::read_to_string(dir.join(PINS_FILE)) else {
            return Self::default();
        };
        Self::from_json(&text).unwrap_or_default()
    }

    /// Persist the pins to the settings directory, creating it if needed. Returns
    /// `false` when there is no settings directory or the write failed: the
    /// bless still took effect for THIS session, it just could not be recorded.
    pub fn save(&self) -> bool {
        match crate::retrieval::settings_dir() {
            Some(dir) => self.save_to(&dir),
            None => false,
        }
    }

    /// Persist the pins to a SPECIFIC directory (the directory-taking core
    /// [`save`](TrustedNamePins::save) delegates to), creating it if needed.
    pub fn save_to(&self, dir: &std::path::Path) -> bool {
        if dir.as_os_str().is_empty() || std::fs::create_dir_all(dir).is_err() {
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

    /// Whether nothing is blessed (a fresh install, or an unreadable store).
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

    /// Parse the persisted wire form. Returns `None` only on a JSON parse error;
    /// individual entries that are malformed (no name, no CID, an unknown posture
    /// spelling) are DROPPED rather than defaulted, because a pin werust cannot
    /// read honestly is a pin it must not claim the user made.
    #[must_use]
    pub fn from_json(text: &str) -> Option<Self> {
        let value: Value = serde_json::from_str(text).ok()?;
        let entries = value.get("pins").and_then(Value::as_array)?;
        let mut pins: Vec<TrustedNamePin> = entries
            .iter()
            .filter_map(|entry| {
                let name = pin_key(entry.get("name").and_then(Value::as_str)?);
                let cid = entry.get("cid").and_then(Value::as_str)?.to_string();
                if name.is_empty() || cid.is_empty() {
                    return None;
                }
                let blessed_at = entry.get("blessedAt").and_then(Value::as_u64)?;
                let posture =
                    trust_posture_from_wire_name(entry.get("posture").and_then(Value::as_str)?)?;
                Some(TrustedNamePin {
                    name,
                    cid,
                    blessed_at,
                    posture,
                })
            })
            .collect();
        pins.sort_by(|a, b| a.name.cmp(&b.name));
        pins.dedup_by(|a, b| a.name == b.name);
        Some(Self { pins })
    }
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
        let mut pins = TrustedNamePins::load_from(&scratch.path);
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
        let reloaded = TrustedNamePins::load_from(&scratch.path);
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
    fn a_missing_or_corrupt_store_degrades_to_no_pins_never_to_a_broken_load() {
        // Fail-safe: the pin store can only make werust say MORE. A fresh install,
        // a corrupt file, or an entry werust cannot read must all read as "nothing
        // blessed" (the exact pre-TOFU behaviour), never a panic and never a
        // claimed pin nobody made.
        let scratch = ScratchDir::new("corrupt");
        assert!(TrustedNamePins::load_from(&scratch.path).is_empty());

        std::fs::create_dir_all(&scratch.path).unwrap();
        for bad in [
            "not json {",
            "[]",
            "{}",
            r#"{"pins":"nope"}"#,
            // Well-formed JSON whose ENTRIES are unreadable: no cid, no posture,
            // an unknown posture spelling, an empty name.
            r#"{"pins":[{"name":"a.eth","blessedAt":1,"posture":"mutable-name"}]}"#,
            r#"{"pins":[{"name":"a.eth","cid":"bafy","blessedAt":1}]}"#,
            r#"{"pins":[{"name":"a.eth","cid":"bafy","blessedAt":1,"posture":"totally-trusted"}]}"#,
            r#"{"pins":[{"name":"  ","cid":"bafy","blessedAt":1,"posture":"mutable-name"}]}"#,
        ] {
            std::fs::write(scratch.path.join(PINS_FILE), bad).unwrap();
            assert!(
                TrustedNamePins::load_from(&scratch.path).is_empty(),
                "`{bad}` must degrade to no pins"
            );
        }
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
        let round_tripped = TrustedNamePins::from_json(&pins.to_json()).expect("valid JSON");
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
