//! The archived combo model.
//!
//! Same shape as the card catalog: written once by `build-artifacts`, memory-mapped and read
//! in place. Combos are keyed on Scryfall `oracle_id`, which is what makes them line up with
//! decks and collections without a translation step.

use std::path::Path;

use rkyv::{Archive, Deserialize, Serialize};

use crate::error::{ComboError, Result};

/// Bumped whenever the archived layout changes in a way older readers cannot handle.
pub const FORMAT_VERSION: u32 = 1;

/// One known combo.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct Combo {
    /// Commander Spellbook's own identifier, so a combo can be looked up on their site.
    pub id: String,
    /// Oracle ids of every card the combo needs. All of them must be present.
    pub oracle_ids: Vec<String>,
    /// Card names, denormalised so a result is readable without a catalog.
    pub card_names: Vec<String>,
    /// What it does — "Infinite colorless mana", "Win the game", and so on.
    pub produces: Vec<String>,
    /// Colour identity, as WUBRG letters.
    pub identity: String,
    pub legal_in_commander: bool,
    /// How often it turns up in decklists. Absent when Spellbook does not say.
    pub popularity: Option<u32>,
    /// Spellbook's own bracket tag, stored **verbatim and uninterpreted**.
    ///
    /// The values are single letters — S, E, R, P, O, C — and their meaning is not documented
    /// anywhere we could find. Rather than guess a mapping onto the official 1–5 brackets and
    /// present the guess as a verdict, the bracket estimate is derived from Wizards' published
    /// criteria instead, and this is kept only so it is available if the meaning is ever
    /// established.
    pub bracket_tag: String,
}

impl ArchivedCombo {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// How many cards it takes. Two is the number the bracket rules care about.
    pub fn card_count(&self) -> usize {
        self.oracle_ids.len()
    }

    /// True when the combo produces something unbounded.
    ///
    /// Matched on Spellbook's feature names, which say "Infinite ..." for the ones that matter.
    /// A combo that merely produces a large amount is not the same thing and does not count
    /// against a bracket.
    pub fn is_infinite(&self) -> bool {
        self.produces
            .iter()
            .any(|feature| feature.to_lowercase().contains("infinite"))
    }

    /// True when it wins outright rather than producing a resource.
    pub fn wins_the_game(&self) -> bool {
        self.produces.iter().any(|feature| {
            let lowered = feature.to_lowercase();
            lowered.contains("win the game") || lowered.contains("each opponent loses the game")
        })
    }

    pub fn names(&self) -> Vec<&str> {
        self.card_names.iter().map(|n| n.as_str()).collect()
    }

    pub fn produces_list(&self) -> Vec<&str> {
        self.produces.iter().map(|f| f.as_str()).collect()
    }
}

/// The root of the combo artifact.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct ComboData {
    pub format_version: u32,
    /// When the snapshot was taken, so the UI can say how stale it is.
    pub fetched_at: String,
    pub combos: Vec<Combo>,
}

/// Serializes combos for `build-artifacts`.
pub fn serialize(data: &ComboData) -> Result<Vec<u8>> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(data)
        .map_err(|e| ComboError::Serialize(e.to_string()))?;
    Ok(bytes.to_vec())
}

enum Backing {
    Mmap(memmap2::Mmap),
    Owned(Vec<u8>),
}

impl Backing {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Backing::Mmap(m) => m,
            Backing::Owned(v) => v,
        }
    }
}

/// A loaded combo database.
pub struct ComboDatabase {
    backing: Backing,
    fetched_at: String,
}

impl ComboDatabase {
    /// Opens a combo artifact by memory-mapping it.
    pub fn open(path: impl AsRef<Path>) -> Result<ComboDatabase> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|source| ComboError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let mmap = raw::map_file(&file).map_err(|source| ComboError::Io {
            path: path.display().to_string(),
            source,
        })?;
        ComboDatabase::build(Backing::Mmap(mmap))
    }

    pub fn from_bytes(bytes: Vec<u8>) -> Result<ComboDatabase> {
        ComboDatabase::build(Backing::Owned(bytes))
    }

    fn build(backing: Backing) -> Result<ComboDatabase> {
        // Validated once here; every later read uses the unchecked accessor, which is only
        // sound because this succeeded. Same contract as the card catalog.
        let data = rkyv::access::<ArchivedComboData, rkyv::rancor::Error>(backing.as_bytes())
            .map_err(|e| ComboError::Corrupt(e.to_string()))?;

        let found = data.format_version.to_native();
        if found != FORMAT_VERSION {
            return Err(ComboError::VersionMismatch {
                expected: FORMAT_VERSION,
                found,
            });
        }

        let fetched_at = data.fetched_at.to_string();
        Ok(ComboDatabase {
            backing,
            fetched_at,
        })
    }

    fn data(&self) -> &ArchivedComboData {
        raw::root(self.backing.as_bytes())
    }

    pub fn fetched_at(&self) -> &str {
        &self.fetched_at
    }

    pub fn len(&self) -> usize {
        self.data().combos.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Option<&ArchivedCombo> {
        self.data().combos.get(index)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ArchivedCombo> {
        self.data().combos.iter()
    }
}

impl std::fmt::Debug for ComboDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComboDatabase")
            .field("combos", &self.len())
            .field("fetched_at", &self.fetched_at)
            .finish_non_exhaustive()
    }
}

/// The only `unsafe` in this crate, kept to one place. Identical contract to `mtg-data`: the
/// file must not change under the map, and the bytes must have been validated first.
mod raw {
    #![allow(unsafe_code)]

    use super::ArchivedComboData;

    /// # Safety contract we accept
    /// The mapped file must not be modified or truncated while the map is alive. Artifacts are
    /// written once and then only read.
    pub(super) fn map_file(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
        // SAFETY: see the contract above.
        unsafe { memmap2::Mmap::map(file) }
    }

    /// # Safety contract we accept
    /// `bytes` must already have been validated by `rkyv::access`, which
    /// `ComboDatabase::build` does once before any call here.
    pub(super) fn root(bytes: &[u8]) -> &ArchivedComboData {
        // SAFETY: see the contract above.
        unsafe { rkyv::access_unchecked::<ArchivedComboData>(bytes) }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn combo(id: &str, cards: &[&str], produces: &[&str]) -> Combo {
        Combo {
            id: id.to_owned(),
            oracle_ids: cards.iter().map(|c| format!("o-{c}")).collect(),
            card_names: cards.iter().map(|c| (*c).to_owned()).collect(),
            produces: produces.iter().map(|p| (*p).to_owned()).collect(),
            identity: "U".to_owned(),
            legal_in_commander: true,
            popularity: Some(100),
            bracket_tag: "S".to_owned(),
        }
    }

    pub(crate) fn database(combos: Vec<Combo>) -> ComboDatabase {
        let data = ComboData {
            format_version: FORMAT_VERSION,
            fetched_at: "2026-08-17".to_owned(),
            combos,
        };
        ComboDatabase::from_bytes(serialize(&data).unwrap()).unwrap()
    }

    #[test]
    fn combos_round_trip() {
        let db = database(vec![combo(
            "513-5034",
            &["Hullbreaker Horror", "Sol Ring"],
            &["Infinite colorless mana", "Infinite storm count"],
        )]);

        assert_eq!(db.len(), 1);
        assert_eq!(db.fetched_at(), "2026-08-17");

        let first = db.get(0).unwrap();
        assert_eq!(first.id(), "513-5034");
        assert_eq!(first.card_count(), 2);
        assert_eq!(first.names(), ["Hullbreaker Horror", "Sol Ring"]);
    }

    #[test]
    fn infinite_is_read_from_what_the_combo_produces() {
        let db = database(vec![
            combo("a", &["X", "Y"], &["Infinite colorless mana"]),
            combo("b", &["X", "Y"], &["Near-infinite mana"]),
            combo("c", &["X", "Y"], &["Two extra cards"]),
        ]);

        assert!(db.get(0).unwrap().is_infinite());
        // "Near-infinite" still contains "infinite", which is the honest reading: Spellbook
        // uses it for loops bounded only by something impractical.
        assert!(db.get(1).unwrap().is_infinite());
        assert!(
            !db.get(2).unwrap().is_infinite(),
            "a large amount is not infinite"
        );
    }

    #[test]
    fn winning_outright_is_told_apart_from_producing_a_resource() {
        let db = database(vec![
            combo("a", &["X", "Y"], &["Win the game"]),
            combo("b", &["X", "Y"], &["Each opponent loses the game"]),
            combo("c", &["X", "Y"], &["Infinite colorless mana"]),
        ]);

        assert!(db.get(0).unwrap().wins_the_game());
        assert!(db.get(1).unwrap().wins_the_game());
        assert!(!db.get(2).unwrap().wins_the_game());
    }

    #[test]
    fn an_empty_database_is_valid() {
        // What the app has before the optional combo artifact is downloaded.
        let db = database(Vec::new());
        assert!(db.is_empty());
        assert!(db.get(0).is_none());
    }

    #[test]
    fn a_version_mismatch_is_reported() {
        let data = ComboData {
            format_version: FORMAT_VERSION + 1,
            fetched_at: String::new(),
            combos: Vec::new(),
        };
        let error = ComboDatabase::from_bytes(serialize(&data).unwrap()).unwrap_err();
        assert!(
            matches!(error, ComboError::VersionMismatch { .. }),
            "{error}"
        );
    }

    #[test]
    fn corrupt_bytes_are_rejected_rather_than_trusted() {
        let error = ComboDatabase::from_bytes(vec![0u8; 64]).unwrap_err();
        assert!(matches!(error, ComboError::Corrupt(_)), "{error}");
    }
}
