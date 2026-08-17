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
///
/// 2 — added `produced_mana`, without which counting a deck's colour sources would have to
/// guess from rules text.
pub const FORMAT_VERSION: u32 = 2;

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
    /// Normalised full name to id. Built eagerly because deck import resolves thousands of
    /// names in a row and cannot afford a scan each time.
    by_name: HashMap<String, CardId>,
    /// Normalised face names of multi-faced cards. Kept separate from `by_name` so a real card
    /// always wins over a face that happens to share its name.
    ///
    /// A `Vec` because face names are not unique: `Fire` is a half of both `Fire // Ice` and
    /// `Start // Fire`. Silently picking one would put the wrong card in an imported deck.
    by_alias: HashMap<String, Vec<CardId>>,
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
        let mut by_alias = HashMap::new();
        for (position, card) in data.cards.iter().enumerate() {
            let id = CardId(position as u32);
            // Duplicate names should not happen in oracle data; if they do, first wins and
            // the later one stays reachable by id.
            by_name.entry(normalize_name(card.name())).or_insert(id);

            // Face names, so a decklist saying "Bonecrusher Giant" finds
            // "Bonecrusher Giant // Stomp". Only useful for multi-faced cards, and only when
            // the face name is not itself a real card — "Fire" and "Ice" are both.
            if card.is_multi_faced() {
                for face in card.faces() {
                    let candidates: &mut Vec<CardId> =
                        by_alias.entry(normalize_name(face.name())).or_default();
                    if !candidates.contains(&id) {
                        candidates.push(id);
                    }
                }
            }
        }

        Ok(Catalog {
            backing,
            by_name,
            by_alias,
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

    /// Looks up a card by its full name.
    ///
    /// Matching ignores case, accents and punctuation, so `"Ajani's Pridemate"`,
    /// `"ajanis pridemate"` and `"Ajani’s Pridemate"` with a typographic apostrophe all find the
    /// same card. It does **not** accept a single face of a multi-part card — use
    /// [`Catalog::resolve_name`] for that.
    pub fn find_by_name(&self, name: &str) -> Option<(CardId, &ArchivedCard)> {
        let id = *self.by_name.get(&normalize_name(name))?;
        self.get(id).map(|card| (id, card))
    }

    /// Looks up a card the way a decklist writes it.
    ///
    /// Decklists exported by other tools rarely spell multi-part cards in full: they say
    /// `Bonecrusher Giant`, not `Bonecrusher Giant // Stomp`. Resolution goes, in order:
    ///
    /// 1. the full name;
    /// 2. the part before `//`, for lists that write only the front face;
    /// 3. any face name.
    ///
    /// The order matters. `Fire` and `Ice` are each a real card as well as a face of
    /// `Fire // Ice`, and a real card must always win over a face of something else.
    pub fn resolve_name(&self, name: &str) -> Option<(CardId, &ArchivedCard)> {
        match self.resolve(name) {
            Resolution::Found(id, card) => Some((id, card)),
            Resolution::Ambiguous(_) | Resolution::NotFound => None,
        }
    }

    /// Resolves a name, reporting ambiguity instead of guessing.
    ///
    /// Importers should use this rather than [`Catalog::resolve_name`]: a face name can belong
    /// to more than one card, and picking one at random puts a card the user never chose into
    /// their deck, with nothing on screen to say so.
    pub fn resolve(&self, name: &str) -> Resolution<'_> {
        match self.lookup(name) {
            Resolution::NotFound => {}
            other => return other,
        }
        // Only reached when the whole string is not a card: an exporter may have written
        // "Front // Back" using face names that do not form the card's actual full name.
        match name.split_once("//") {
            Some((front, _)) => self.lookup(front.trim()),
            None => Resolution::NotFound,
        }
    }

    /// Full names first, then face names.
    ///
    /// The order is the whole point: a card printed under its own name has to win over a face
    /// of some other card that happens to share it.
    fn lookup(&self, name: &str) -> Resolution<'_> {
        let key = normalize_name(name);

        if let Some(&id) = self.by_name.get(&key) {
            if let Some(card) = self.get(id) {
                return Resolution::Found(id, card);
            }
        }

        let Some(candidates) = self.by_alias.get(&key) else {
            return Resolution::NotFound;
        };
        let found: Vec<(CardId, &ArchivedCard)> = candidates
            .iter()
            .filter_map(|&id| self.get(id).map(|card| (id, card)))
            .collect();

        match found.len() {
            0 => Resolution::NotFound,
            1 => {
                let (id, card) = found[0];
                Resolution::Found(id, card)
            }
            _ => Resolution::Ambiguous(found),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = (CardId, &ArchivedCard)> {
        self.data()
            .cards
            .iter()
            .enumerate()
            .map(|(position, card)| (CardId(position as u32), card))
    }
}

/// What looking a name up produced.
#[derive(Debug)]
pub enum Resolution<'a> {
    Found(CardId, &'a ArchivedCard),
    /// The name matches a face shared by several cards, e.g. `Fire`, which is half of both
    /// `Fire // Ice` and `Start // Fire`. The caller has to ask rather than pick.
    Ambiguous(Vec<(CardId, &'a ArchivedCard)>),
    NotFound,
}

impl Resolution<'_> {
    pub fn is_found(&self) -> bool {
        matches!(self, Resolution::Found(..))
    }
}

/// Folds a card name into the form used as an index key.
///
/// Lowercases, strips accents, drops punctuation and collapses whitespace, so all of
/// `"Lim-Dûl's Vault"`, `"lim-duls vault"` and `"Lim-Dul’s  Vault"` land on the same key. Card
/// names arrive from decklists other tools exported, from users typing on a phone keyboard
/// with no easy circumflex, and from Scryfall itself — matching has to survive all three.
fn normalize_name(name: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    let mut out = String::with_capacity(name.len());
    let mut pending_space = false;

    // NFD splits "û" into "u" plus a combining circumflex, which the filter below then drops.
    for ch in name.nfd() {
        // Combining diacritical marks.
        if ('\u{0300}'..='\u{036F}').contains(&ch) {
            continue;
        }

        for lower in ch.to_lowercase() {
            // Ligatures and strokes do not decompose under NFD, so they are spelled out here.
            // "Æther" was the old spelling of what Scryfall now writes "Aether".
            let expansion = match lower {
                'æ' => Some("ae"),
                'œ' => Some("oe"),
                'ø' => Some("o"),
                'ß' => Some("ss"),
                'đ' | 'ð' => Some("d"),
                'ł' => Some("l"),
                'þ' => Some("th"),
                _ => None,
            };

            if let Some(expansion) = expansion {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push_str(expansion);
            } else if lower.is_alphanumeric() {
                if pending_space && !out.is_empty() {
                    out.push(' ');
                }
                pending_space = false;
                out.push(lower);
            } else if lower.is_whitespace() || lower == '/' {
                // Slashes count as separators, not as droppable punctuation: without this,
                // "Bonecrusher Giant//Stomp" would fold to "bonecrusher giantstomp" and match
                // nothing, while the spaced spelling folded correctly.
                pending_space = true;
            }
            // Everything else — apostrophes, commas, hyphens — is dropped, so the straight and
            // typographic apostrophes cannot disagree.
        }
    }

    out
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
            produced_mana: 0,
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
    fn name_lookup_folds_accents_and_punctuation() {
        let catalog = round_trip(vec![
            card("Lim-Dûl's Vault", "{1}{U}{B}", "Instant"),
            card("Æther Vial", "{1}", "Artifact"),
        ]);

        // A phone keyboard with no circumflex, a straight apostrophe, a typographic one, and a
        // decklist that dropped the hyphen: all the same card.
        for query in [
            "Lim-Dûl's Vault",
            "Lim-Dul's Vault",
            "lim-duls vault",
            "Lim-Dûl’s Vault",
            "  Lim-Dûl's   Vault  ",
        ] {
            assert!(catalog.find_by_name(query).is_some(), "{query:?}");
        }

        // The ligature does not decompose under NFD, so it is spelled out explicitly.
        assert!(catalog.find_by_name("Aether Vial").is_some());
        assert!(catalog.find_by_name("æther vial").is_some());
    }

    #[test]
    fn resolve_accepts_a_single_face_of_a_multi_part_card() {
        // The reason this exists: decklists exported by other tools write "Bonecrusher Giant",
        // never "Bonecrusher Giant // Stomp".
        let mut giant = card(
            "Bonecrusher Giant // Stomp",
            "{2}{R} // {1}{R}",
            "Creature — Giant // Instant — Adventure",
        );
        giant.layout = Layout::Adventure;
        giant.faces = vec![
            face("Bonecrusher Giant", "{2}{R}", "Creature — Giant"),
            face("Stomp", "{1}{R}", "Instant — Adventure"),
        ];

        let catalog = round_trip(vec![giant]);

        for query in [
            "Bonecrusher Giant // Stomp",
            "Bonecrusher Giant",
            "bonecrusher giant",
            "Stomp",
        ] {
            let (_, found) = catalog.resolve_name(query).expect(query);
            assert_eq!(found.name(), "Bonecrusher Giant // Stomp");
        }

        // The strict lookup still refuses a partial name.
        assert!(catalog.find_by_name("Bonecrusher Giant").is_none());
    }

    #[test]
    fn a_real_card_wins_over_a_face_of_the_same_name() {
        let mut split = card("Fire // Ice", "{1}{R} // {1}{U}", "Instant // Instant");
        split.layout = Layout::Split;
        split.faces = vec![
            face("Fire", "{1}{R}", "Instant"),
            face("Ice", "{1}{U}", "Instant"),
        ];

        let catalog = round_trip(vec![split, card("Fire", "{1}{R}", "Instant")]);

        assert_eq!(catalog.resolve_name("Fire").unwrap().0, CardId(1));
        assert_eq!(catalog.resolve_name("Fire // Ice").unwrap().0, CardId(0));
        // "Ice" has no standalone printing here, so it falls through to the face.
        assert_eq!(catalog.resolve_name("Ice").unwrap().0, CardId(0));
    }

    #[test]
    fn a_face_name_shared_by_several_cards_is_ambiguous_not_guessed() {
        // Real case: in the oracle catalog there is no standalone "Fire" card, but "Fire" is a
        // half of both "Fire // Ice" and "Start // Fire". Picking one would silently put a card
        // the user never asked for into their deck.
        let mut fire_ice = card("Fire // Ice", "{1}{R} // {1}{U}", "Instant // Instant");
        fire_ice.layout = Layout::Split;
        fire_ice.faces = vec![
            face("Fire", "{1}{R}", "Instant"),
            face("Ice", "{1}{U}", "Instant"),
        ];

        let mut start_fire = card("Start // Fire", "{2}{W} // {1}{R}", "Sorcery // Sorcery");
        start_fire.layout = Layout::Split;
        start_fire.faces = vec![
            face("Start", "{2}{W}", "Sorcery"),
            face("Fire", "{1}{R}", "Sorcery"),
        ];

        let catalog = round_trip(vec![fire_ice, start_fire]);

        match catalog.resolve("Fire") {
            Resolution::Ambiguous(candidates) => {
                let names: Vec<&str> = candidates.iter().map(|(_, c)| c.name()).collect();
                assert_eq!(names, ["Fire // Ice", "Start // Fire"]);
            }
            other => panic!("expected ambiguity, got {other:?}"),
        }

        // The convenience wrapper refuses to guess.
        assert!(catalog.resolve_name("Fire").is_none());

        // Unambiguous faces still resolve straight through.
        assert_eq!(catalog.resolve_name("Ice").unwrap().0, CardId(0));
        assert_eq!(catalog.resolve_name("Start").unwrap().0, CardId(1));
    }

    #[test]
    fn resolve_falls_back_to_the_front_half_of_a_written_out_name() {
        // Some exporters write "Front // Back" using the *face* names of a card whose full
        // name differs, or with odd spacing around the slashes.
        let mut giant = card(
            "Bonecrusher Giant // Stomp",
            "{2}{R} // {1}{R}",
            "Creature — Giant // Instant",
        );
        giant.layout = Layout::Adventure;
        giant.faces = vec![
            face("Bonecrusher Giant", "{2}{R}", "Creature — Giant"),
            face("Stomp", "{1}{R}", "Instant"),
        ];
        let catalog = round_trip(vec![giant]);

        assert!(catalog.resolve_name("Bonecrusher Giant//Stomp").is_some());
        assert!(catalog
            .resolve_name("Bonecrusher Giant // Whatever")
            .is_some());
    }

    #[test]
    fn unknown_names_resolve_to_nothing() {
        let catalog = round_trip(vec![card("Sol Ring", "{1}", "Artifact")]);
        assert!(catalog.resolve_name("Not A Card").is_none());
        assert!(catalog.resolve_name("").is_none());
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
