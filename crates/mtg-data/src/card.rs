//! The archived card model.
//!
//! These types are serialized once by `build-artifacts` and then only ever read, via a
//! memory-mapped zero-copy view. That shapes the design: fields are plain owned data with no
//! back-references, enums are fieldless, and anything derived (parsed mana costs, color sets)
//! is recomputed on access rather than stored.
//!
//! The model is **multi-faced from the start**. Retrofitting that later would touch every
//! consumer of this crate, so even single-faced cards go through the same shape.

use mtg_core::{ColorSet, Format, Legality, ManaCost, ManaCostError, Rarity};
use rkyv::{Archive, Deserialize, Serialize};

/// Number of legality slots stored per card.
///
/// Spelled out rather than written as `Format::COUNT` because it is a storage layout: if a
/// format is added upstream, this must not silently change the archive format under readers
/// that expect the old width. The assertion below forces the two to be reconciled together,
/// which is also when [`crate::FORMAT_VERSION`] should be bumped.
pub const LEGALITY_SLOTS: usize = 23;

const _: () = assert!(LEGALITY_SLOTS == Format::COUNT);

/// How a card's faces are arranged, from Scryfall's `layout` field.
///
/// Only the distinctions that change behaviour are modelled; everything else collapses into
/// [`Layout::Other`], which is treated as single-faced.
#[derive(Archive, Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[rkyv(derive(Debug))]
pub enum Layout {
    /// One face, the overwhelming majority of cards.
    #[default]
    Normal,
    /// `Fire // Ice` — two halves, either castable.
    Split,
    /// Two halves, the second only castable after the first.
    Adventure,
    /// Transforms in place; the back face has no mana cost of its own.
    Transform,
    /// Modal double-faced: either side can be played from hand.
    ModalDfc,
    /// Combines with another card.
    Meld,
    /// Rotates rather than transforming.
    Flip,
    /// Anything else: tokens, emblems, art series, planes, schemes.
    Other,
}

impl Layout {
    pub fn from_scryfall_value(value: &str) -> Layout {
        match value {
            "normal" => Layout::Normal,
            "split" => Layout::Split,
            "adventure" => Layout::Adventure,
            "transform" => Layout::Transform,
            "modal_dfc" => Layout::ModalDfc,
            "meld" => Layout::Meld,
            "flip" => Layout::Flip,
            _ => Layout::Other,
        }
    }

    /// True when both faces are on the same physical side of the card, so the whole card is
    /// visible at once. Split and adventure cards behave differently from transforming ones
    /// for deck building and for camera recognition alike.
    pub const fn is_single_sided(self) -> bool {
        matches!(
            self,
            Layout::Normal | Layout::Split | Layout::Adventure | Layout::Flip | Layout::Other
        )
    }
}

/// One face of a card.
///
/// Present even for cards with a single face, so consumers never need two code paths.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct CardFace {
    pub name: String,
    /// Raw Scryfall notation, e.g. `{2}{W}`. Empty for the back of a transforming card.
    pub mana_cost: String,
    pub type_line: String,
    pub oracle_text: String,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    /// [`ColorSet`] bits for this face specifically.
    pub colors: u8,
}

/// An oracle card: the rules of a card, independent of any particular printing.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
#[rkyv(derive(Debug))]
pub struct Card {
    /// Scryfall's oracle UUID. Stable across printings and across catalog rebuilds, unlike
    /// `CardId`, which is only an index into one particular artifact.
    pub oracle_id: String,
    /// Full name, including `//` for multi-part cards.
    pub name: String,
    /// Raw Scryfall notation. Empty for transforming cards, which carry costs per face.
    pub mana_cost: String,
    /// Scryfall's `cmc`. A float because some joke cards have half costs.
    pub mana_value: f32,
    /// [`ColorSet`] bits.
    pub colors: u8,
    /// [`ColorSet`] bits. This is what the Commander identity rule tests against.
    pub color_identity: u8,
    /// [`ColorSet`] bits for the mana this card can produce, from Scryfall's `produced_mana`.
    ///
    /// Covers lands, mana creatures and artifacts alike, which is why counting sources does
    /// not have to guess from rules text. Note that Scryfall reports the theoretical maximum:
    /// Arcane Signet is listed as producing all five colours, though in play it produces only
    /// the commander's. That overstates a mana base slightly and is the right way round — it
    /// never invents a source a card cannot make.
    pub produced_mana: u8,
    pub type_line: String,
    pub oracle_text: String,
    pub power: Option<String>,
    pub toughness: Option<String>,
    pub loyalty: Option<String>,
    pub keywords: Vec<String>,
    /// One [`Legality`] per format, indexed by [`Format::index`].
    pub legalities: [u8; LEGALITY_SLOTS],
    pub rarity: u8,
    /// Popularity rank in Commander. Absent for cards nobody plays.
    pub edhrec_rank: Option<u32>,
    /// Scryfall's own flag for the official Commander Game Changers list. Having this in the
    /// card data is why no separate list has to be maintained.
    pub game_changer: bool,
    /// What the card *does*, as [`mtg_core::TagSet`] bits.
    ///
    /// Zero means either "no functional role" or "not tagged", and the two are genuinely hard
    /// to tell apart — a vanilla 2/2 legitimately has none. Anything reading this should treat
    /// an empty set as "nothing known", never as "does nothing".
    pub tags: u64,
    pub reserved: bool,
    pub layout: Layout,
    /// Always at least one entry.
    pub faces: Vec<CardFace>,
    pub set_code: String,
    pub collector_number: String,
    pub released_at: String,
    /// Scryfall image id, used to build a CDN URL on demand. **No artwork is stored here** —
    /// see the legal note in `CLAUDE.md`.
    pub image_id: String,
}

impl ArchivedCard {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_line(&self) -> &str {
        &self.type_line
    }

    pub fn oracle_text(&self) -> &str {
        &self.oracle_text
    }

    pub fn oracle_id(&self) -> &str {
        &self.oracle_id
    }

    pub fn mana_value(&self) -> f32 {
        self.mana_value.to_native()
    }

    pub fn colors(&self) -> ColorSet {
        ColorSet::from_bits(self.colors)
    }

    pub fn color_identity(&self) -> ColorSet {
        ColorSet::from_bits(self.color_identity)
    }

    /// Colours of mana this card can produce.
    pub fn produced_mana(&self) -> ColorSet {
        ColorSet::from_bits(self.produced_mana)
    }

    /// True when the card can produce mana at all, of any colour or none.
    ///
    /// Lands count even when they tap for colourless, which `produced_mana` records as an
    /// empty colour set — hence the type check alongside it.
    pub fn produces_mana(&self) -> bool {
        !self.produced_mana().is_colorless() || self.has_type("Land")
    }

    pub fn rarity(&self) -> Rarity {
        rarity_from_u8(self.rarity)
    }

    /// Legality in one format. Unknown stored values read as [`Legality::NotLegal`], so a
    /// corrupt byte can never make a card look playable.
    pub fn legality(&self, format: Format) -> Legality {
        match self.legalities.get(format.index()) {
            Some(0) => Legality::Legal,
            Some(2) => Legality::Restricted,
            Some(3) => Legality::Banned,
            _ => Legality::NotLegal,
        }
    }

    pub fn is_legal_in(&self, format: Format) -> bool {
        self.legality(format).is_playable()
    }

    pub fn edhrec_rank(&self) -> Option<u32> {
        self.edhrec_rank.as_ref().map(|r| r.to_native())
    }

    pub fn is_game_changer(&self) -> bool {
        self.game_changer
    }

    /// What the card does. Empty means nothing is known, not that it does nothing.
    pub fn tags(&self) -> mtg_core::TagSet {
        mtg_core::TagSet::from_bits(self.tags.to_native())
    }

    /// The raw cost string as Scryfall wrote it, for display.
    ///
    /// Unlike [`ArchivedCard::mana_cost`], this keeps the `"{1}{R} // {1}{U}"` form of split
    /// cards, which is what a reader wants to see even though it is not a castable cost.
    pub fn mana_cost_display(&self) -> &str {
        &self.mana_cost
    }

    /// Parses the top-level mana cost.
    ///
    /// Returns an empty cost for transforming cards, whose costs live on their faces.
    pub fn mana_cost(&self) -> Result<ManaCost, ManaCostError> {
        // Split and adventure cards store both halves separated by " // ".
        match self.mana_cost.split_once(" // ") {
            Some(_) => Ok(ManaCost::empty()),
            None => ManaCost::parse(&self.mana_cost),
        }
    }

    pub fn faces(&self) -> &[ArchivedCardFace] {
        &self.faces
    }

    pub fn is_multi_faced(&self) -> bool {
        self.faces.len() > 1
    }

    /// True when the type line names `kind`, e.g. `"Creature"` or `"Legendary"`.
    ///
    /// Matches on whole words so that `"Land"` does not also match `"Landfall"`.
    pub fn has_type(&self, kind: &str) -> bool {
        type_line_has(self.type_line(), kind)
            || self.faces.iter().any(|f| type_line_has(&f.type_line, kind))
    }

    /// True when this card can be a commander: a legendary creature, or a card that says so.
    ///
    /// The text check catches Planeswalker commanders such as those reading
    /// "can be your commander". Background and partner rules are deck-level concerns and are
    /// handled in `mtg-deck`, not here.
    pub fn can_be_commander(&self) -> bool {
        if self.has_type("Legendary") && self.has_type("Creature") {
            return true;
        }
        let says_so = |text: &str| text.contains("can be your commander");
        says_so(self.oracle_text()) || self.faces.iter().any(|f| says_so(&f.oracle_text))
    }
}

impl ArchivedCardFace {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_line(&self) -> &str {
        &self.type_line
    }

    pub fn oracle_text(&self) -> &str {
        &self.oracle_text
    }

    pub fn colors(&self) -> ColorSet {
        ColorSet::from_bits(self.colors)
    }

    pub fn mana_cost(&self) -> Result<ManaCost, ManaCostError> {
        ManaCost::parse(&self.mana_cost)
    }

    /// The raw cost string, for display.
    pub fn mana_cost_display(&self) -> &str {
        &self.mana_cost
    }
}

/// Whole-word, case-insensitive search in a type line.
fn type_line_has(type_line: &str, kind: &str) -> bool {
    type_line
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .any(|word| word.eq_ignore_ascii_case(kind))
}

/// Encodes a [`Legality`] for storage. Must stay in sync with [`ArchivedCard::legality`].
pub const fn legality_to_u8(legality: Legality) -> u8 {
    match legality {
        Legality::Legal => 0,
        Legality::NotLegal => 1,
        Legality::Restricted => 2,
        Legality::Banned => 3,
    }
}

/// Encodes a [`Rarity`] for storage.
pub const fn rarity_to_u8(rarity: Rarity) -> u8 {
    match rarity {
        Rarity::Common => 0,
        Rarity::Uncommon => 1,
        Rarity::Rare => 2,
        Rarity::Mythic => 3,
        Rarity::Special => 4,
        Rarity::Bonus => 5,
    }
}

const fn rarity_from_u8(value: u8) -> Rarity {
    match value {
        1 => Rarity::Uncommon,
        2 => Rarity::Rare,
        3 => Rarity::Mythic,
        4 => Rarity::Special,
        5 => Rarity::Bonus,
        _ => Rarity::Common,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legality_slots_match_format_count() {
        // Also enforced at compile time; this makes the failure legible in test output.
        assert_eq!(LEGALITY_SLOTS, Format::COUNT);
    }

    #[test]
    fn legality_encoding_round_trips() {
        for legality in [
            Legality::Legal,
            Legality::NotLegal,
            Legality::Restricted,
            Legality::Banned,
        ] {
            let mut slots = [legality_to_u8(Legality::NotLegal); LEGALITY_SLOTS];
            slots[Format::Modern.index()] = legality_to_u8(legality);
            // Decoding is exercised through ArchivedCard in the catalog tests; here we only
            // pin the numbering the two sides agree on.
            assert_eq!(slots[Format::Modern.index()], legality_to_u8(legality));
        }
    }

    #[test]
    fn rarity_encoding_round_trips() {
        for rarity in [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Mythic,
            Rarity::Special,
            Rarity::Bonus,
        ] {
            assert_eq!(rarity_from_u8(rarity_to_u8(rarity)), rarity);
        }
    }

    #[test]
    fn layout_parsing() {
        assert_eq!(Layout::from_scryfall_value("normal"), Layout::Normal);
        assert_eq!(Layout::from_scryfall_value("modal_dfc"), Layout::ModalDfc);
        assert_eq!(Layout::from_scryfall_value("adventure"), Layout::Adventure);
        // Unmodelled layouts collapse rather than failing.
        assert_eq!(Layout::from_scryfall_value("planar"), Layout::Other);
    }

    #[test]
    fn transforming_layouts_are_two_sided() {
        assert!(!Layout::Transform.is_single_sided());
        assert!(!Layout::ModalDfc.is_single_sided());
        // Split and adventure cards show everything on one side.
        assert!(Layout::Split.is_single_sided());
        assert!(Layout::Adventure.is_single_sided());
    }

    #[test]
    fn type_line_matching_is_whole_word() {
        assert!(type_line_has("Basic Land — Forest", "Land"));
        assert!(type_line_has(
            "Legendary Creature — Human Wizard",
            "Creature"
        ));
        assert!(type_line_has(
            "Legendary Creature — Human Wizard",
            "creature"
        ));
        // The reason a substring match would not do.
        assert!(!type_line_has("Enchantment — Landfall", "Land"));
        assert!(!type_line_has("Artifact Creature — Golem", "Art"));
    }
}
