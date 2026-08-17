//! Loading and reading the card catalog artifact.
//!
//! The artifact is a single rkyv archive, memory-mapped and read in place. Nothing is parsed
//! at startup, so opening a 25 MB catalog costs about as much as opening a file.

use std::collections::HashMap;
use std::path::Path;

use mtg_core::CardId;
use rkyv::{Archive, Deserialize, Serialize};

use crate::card::{ArchivedCard, Card};
use crate::error::{CatalogError, Result};

/// Bumped whenever the archived layout changes in a way older readers cannot handle.
pub const FORMAT_VERSION: u32 = 1;

/// The root of the catalog artifact.
#[derive(Archive, Serialize, Deserialize, Debug)]
#[rkyv(derive(Debug))]
pub struct CatalogData {
    /// Must equal [`FORMAT_VERSION`] for this build to read the artifact.
    pub format_version: u32,
    /// Scryfall's `updated_at` for the bulk file this was built from, so the UI can say how
    /// stale the data is without a network call.
    pub source_updated_at: String,
    /// Ordered so that a card's position is its [`CardId`].
    pub cards: Vec<Card>,
}

/// Serializes a catalog for `build-artifacts`.
pub fn serialize(data: &CatalogData) -> Result<Vec<u8>> {
    let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(data)
        .map_err(|e| CatalogError::Serialize(e.to_string()))?;
    Ok(bytes.to_vec())
}

/// Where the archive bytes live.
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

/// The card catalog: an archive plus the indexes built over it.
///
/// Cheap to open and cheap to keep around. Card data is read straight out of the mapped file,
/// so pages are only faulted in as they are touched.
pub struct Catalog {
    backing: Backing,
    /// Lowercased full name to id. Built eagerly because deck import resolves thousands of
    /// names in a row and cannot afford a scan each time.
    by_name: HashMap<String, CardId>,
    source_updated_at: String,
}

impl Catalog {
    /// Opens a catalog artifact by memory-mapping it.
    pub fn open(path: impl AsRef<Path>) -> Result<Catalog> {
        let path = path.as_ref();
        let file = std::fs::File::open(path).map_err(|source| CatalogError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let mmap = raw::map_file(&file).map_err(|source| CatalogError::Io {
            path: path.display().to_string(),
            source,
        })?;
        Catalog::build(Backing::Mmap(mmap))
    }

    /// Builds a catalog from bytes already in memory. Used by tests and by callers that
    /// received the artifact over a channel that never touched the filesystem.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Catalog> {
        Catalog::build(Backing::Owned(bytes))
    }

    fn build(backing: Backing) -> Result<Catalog> {
        // Validate once, here. Every later read uses the unchecked accessor, which is only
        // sound because this succeeded — see the `raw` module.
        let data = rkyv::access::<ArchivedCatalogData, rkyv::rancor::Error>(backing.as_bytes())
            .map_err(|e| CatalogError::Corrupt(e.to_string()))?;

        let found = data.format_version.to_native();
        if found != FORMAT_VERSION {
            return Err(CatalogError::VersionMismatch {
                expected: FORMAT_VERSION,
                found,
            });
        }

        let source_updated_at = data.source_updated_at.to_string();
        let mut by_name = HashMap::with_capacity(data.cards.len());
        for (position, card) in data.cards.iter().enumerate() {
            // Duplicate names should not happen in oracle data; if they do, first wins and
            // the later one stays reachable by id.
            by_name
                .entry(card.name().to_lowercase())
                .or_insert(CardId(position as u32));
        }

        Ok(Catalog {
            backing,
            by_name,
            source_updated_at,
        })
    }

    fn data(&self) -> &ArchivedCatalogData {
        raw::root(self.backing.as_bytes())
    }

    /// Scryfall's `updated_at` for the source bulk file.
    pub fn source_updated_at(&self) -> &str {
        &self.source_updated_at
    }

    pub fn len(&self) -> usize {
        self.data().cards.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, id: CardId) -> Option<&ArchivedCard> {
        self.data().cards.get(id.index())
    }

    /// Looks up a card by its exact full name, case-insensitively.
    ///
    /// Multi-part cards are keyed on the joined name, e.g. `"fire // ice"`. Deck lists
    /// commonly write only the first half, so `mtg-deck` will need a friendlier resolver;
    /// this one stays exact on purpose.
    pub fn find_by_name(&self, name: &str) -> Option<(CardId, &ArchivedCard)> {
        let id = *self.by_name.get(&name.to_lowercase())?;
        self.get(id).map(|card| (id, card))
    }

    pub fn iter(&self) -> impl Iterator<Item = (CardId, &ArchivedCard)> {
        self.data()
            .cards
            .iter()
            .enumerate()
            .map(|(position, card)| (CardId(position as u32), card))
    }
}

/// Written by hand rather than derived: a derived impl would dump the whole name index and
/// the mapped bytes, which is useless in a test failure and enormous in a real catalog.
impl std::fmt::Debug for Catalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Catalog")
            .field("cards", &self.len())
            .field("source_updated_at", &self.source_updated_at)
            .finish_non_exhaustive()
    }
}

/// The only place in this crate that uses `unsafe`, kept small on purpose.
///
/// Two operations genuinely require it and have no safe equivalent:
///
/// * memory-mapping a file, which is unsound if another process truncates it underneath us;
/// * reading an rkyv root without re-validating the whole buffer on every access.
///
/// Both are standard practice for this design. The workspace denies `unsafe_code` by default
/// so that any other use has to be argued for explicitly, as this one is.
mod raw {
    #![allow(unsafe_code)]

    use super::ArchivedCatalogData;

    /// Memory-maps a file for reading.
    ///
    /// # Safety contract we accept
    /// The mapped file must not be modified or truncated while the map is alive. Catalog
    /// artifacts are written once by `build-artifacts` and then only read, so this holds in
    /// practice; a corrupted map would surface as a validation failure on the next open.
    pub(super) fn map_file(file: &std::fs::File) -> std::io::Result<memmap2::Mmap> {
        // SAFETY: see the contract above.
        unsafe { memmap2::Mmap::map(file) }
    }

    /// Returns the archive root without revalidating.
    ///
    /// # Safety contract we accept
    /// `bytes` must have already been validated by `rkyv::access`. `Catalog::build` does that
    /// once, before any call to this function, and the bytes are immutable afterwards.
    /// Revalidating on every read would walk the whole 25 MB archive and defeat the point of
    /// memory-mapping it.
    pub(super) fn root(bytes: &[u8]) -> &ArchivedCatalogData {
        // SAFETY: see the contract above.
        unsafe { rkyv::access_unchecked::<ArchivedCatalogData>(bytes) }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::card::{legality_to_u8, rarity_to_u8, CardFace, Layout, LEGALITY_SLOTS};
    use mtg_core::{ColorSet, Format, Legality, Rarity};

    fn face(name: &str, mana_cost: &str, type_line: &str) -> CardFace {
        CardFace {
            name: name.to_owned(),
            mana_cost: mana_cost.to_owned(),
            type_line: type_line.to_owned(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            colors: 0,
        }
    }

    fn card(name: &str, mana_cost: &str, type_line: &str) -> Card {
        Card {
            oracle_id: format!("oracle-{name}"),
            name: name.to_owned(),
            mana_cost: mana_cost.to_owned(),
            mana_value: 0.0,
            colors: 0,
            color_identity: 0,
            type_line: type_line.to_owned(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            keywords: Vec::new(),
            legalities: [legality_to_u8(Legality::NotLegal); LEGALITY_SLOTS],
            rarity: rarity_to_u8(Rarity::Common),
            edhrec_rank: None,
            game_changer: false,
            reserved: false,
            layout: Layout::Normal,
            faces: vec![face(name, mana_cost, type_line)],
            set_code: "tst".to_owned(),
            collector_number: "1".to_owned(),
            released_at: "2026-01-01".to_owned(),
            image_id: String::new(),
        }
    }

    fn round_trip(cards: Vec<Card>) -> Catalog {
        let data = CatalogData {
            format_version: FORMAT_VERSION,
            source_updated_at: "2026-08-17T09:01:54.476+00:00".to_owned(),
            cards,
        };
        Catalog::from_bytes(serialize(&data).unwrap()).unwrap()
    }

    #[test]
    fn empty_catalog_round_trips() {
        let catalog = round_trip(Vec::new());
        assert!(catalog.is_empty());
        assert_eq!(catalog.source_updated_at(), "2026-08-17T09:01:54.476+00:00");
    }

    #[test]
    fn cards_are_readable_after_round_trip() {
        let catalog = round_trip(vec![
            card("Llanowar Elves", "{G}", "Creature — Elf Druid"),
            card("Counterspell", "{U}{U}", "Instant"),
        ]);

        assert_eq!(catalog.len(), 2);
        let counterspell = catalog.get(CardId(1)).unwrap();
        assert_eq!(counterspell.name(), "Counterspell");
        assert_eq!(counterspell.mana_cost().unwrap().mana_value(), 2);
    }

    #[test]
    fn ids_are_positions() {
        let catalog = round_trip(vec![
            card("A", "{W}", "Instant"),
            card("B", "{U}", "Instant"),
            card("C", "{B}", "Instant"),
        ]);
        for (id, c) in catalog.iter() {
            assert_eq!(catalog.get(id).unwrap().name(), c.name());
        }
        assert_eq!(catalog.get(CardId(2)).unwrap().name(), "C");
        assert!(catalog.get(CardId(3)).is_none());
    }

    #[test]
    fn name_lookup_is_case_insensitive() {
        let catalog = round_trip(vec![card("Sol Ring", "{1}", "Artifact")]);

        for query in ["Sol Ring", "sol ring", "SOL RING"] {
            let (id, found) = catalog.find_by_name(query).unwrap();
            assert_eq!(id, CardId(0));
            assert_eq!(found.name(), "Sol Ring");
        }
        assert!(catalog.find_by_name("Sol").is_none());
    }

    #[test]
    fn split_card_mana_cost_is_empty_at_top_level() {
        // "{1}{R} // {1}{U}" is not a castable cost; each half carries its own.
        let mut fire_ice = card("Fire // Ice", "{1}{R} // {1}{U}", "Instant // Instant");
        fire_ice.layout = Layout::Split;
        fire_ice.faces = vec![
            face("Fire", "{1}{R}", "Instant"),
            face("Ice", "{1}{U}", "Instant"),
        ];

        let catalog = round_trip(vec![fire_ice]);
        let archived = catalog.get(CardId(0)).unwrap();

        assert!(archived.is_multi_faced());
        assert!(archived.mana_cost().unwrap().is_empty());
        assert_eq!(archived.faces()[0].mana_cost().unwrap().mana_value(), 2);
        assert_eq!(
            archived.faces()[1].mana_cost().unwrap().colors(),
            ColorSet::from_symbols("U")
        );
    }

    #[test]
    fn legality_reads_back_per_format() {
        let mut sol_ring = card("Sol Ring", "{1}", "Artifact");
        sol_ring.legalities[Format::Commander.index()] = legality_to_u8(Legality::Legal);
        sol_ring.legalities[Format::Vintage.index()] = legality_to_u8(Legality::Restricted);
        sol_ring.legalities[Format::Modern.index()] = legality_to_u8(Legality::Banned);

        let catalog = round_trip(vec![sol_ring]);
        let archived = catalog.get(CardId(0)).unwrap();

        assert_eq!(archived.legality(Format::Commander), Legality::Legal);
        assert_eq!(archived.legality(Format::Vintage), Legality::Restricted);
        assert_eq!(archived.legality(Format::Modern), Legality::Banned);
        assert_eq!(archived.legality(Format::Standard), Legality::NotLegal);

        assert!(archived.is_legal_in(Format::Commander));
        assert!(archived.is_legal_in(Format::Vintage));
        assert!(!archived.is_legal_in(Format::Modern));
    }

    #[test]
    fn commander_detection() {
        let atraxa = card(
            "Atraxa, Praetors' Voice",
            "{G}{W}{U}{B}",
            "Legendary Creature — Phyrexian Angel Horror",
        );
        let elves = card("Llanowar Elves", "{G}", "Creature — Elf Druid");
        let sword = card("Sol Ring", "{1}", "Artifact");

        let catalog = round_trip(vec![atraxa, elves, sword]);
        assert!(catalog.get(CardId(0)).unwrap().can_be_commander());
        assert!(!catalog.get(CardId(1)).unwrap().can_be_commander());
        assert!(!catalog.get(CardId(2)).unwrap().can_be_commander());
    }

    #[test]
    fn planeswalker_commanders_are_detected_by_text() {
        let mut teferi = card(
            "Teferi, Temporal Archmage",
            "{3}{U}{U}",
            "Legendary Planeswalker — Teferi",
        );
        teferi.oracle_text = "Teferi, Temporal Archmage can be your commander.".to_owned();

        let catalog = round_trip(vec![teferi]);
        assert!(catalog.get(CardId(0)).unwrap().can_be_commander());
    }

    #[test]
    fn version_mismatch_is_reported() {
        let data = CatalogData {
            format_version: FORMAT_VERSION + 1,
            source_updated_at: String::new(),
            cards: Vec::new(),
        };
        let err = Catalog::from_bytes(serialize(&data).unwrap()).unwrap_err();
        assert!(matches!(err, CatalogError::VersionMismatch { .. }), "{err}");
    }

    #[test]
    fn corrupt_bytes_are_rejected_rather_than_trusted() {
        // The validation pass is what makes the unchecked reads afterwards sound.
        let err = Catalog::from_bytes(vec![0u8; 64]).unwrap_err();
        assert!(matches!(err, CatalogError::Corrupt(_)), "{err}");
    }
}
