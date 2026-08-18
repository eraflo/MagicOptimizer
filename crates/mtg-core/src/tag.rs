//! Functional tags: what a card *does*, as opposed to what it costs.
//!
//! # Why this exists
//!
//! Everything the optimizer measures — mana base, curve, opening hands — is blind to a card's
//! effect. Lightning Bolt, Counterspell and a vanilla 2/2 at one mana are the same object to
//! it, which is why it will happily offer to trade one for a Mountain. A tag is the smallest
//! thing that fixes that: not an understanding of the card, but a claim about its role.
//!
//! # Why a fixed vocabulary rather than free-form strings
//!
//! Tags are compared, counted and stored per card. A `u64` of flags is 8 bytes a card, and
//! comparing two cards' roles is one bitwise `and`. Measured, adding the field grew
//! `cards.rkyv` from 24.7 MB to 26.4 MB — more than the 282 KB the raw field accounts for,
//! because it shifts the archive's alignment. Free strings would cost far more, and worse,
//! would let a typo become a silently missing role.
//!
//! Every name below was checked against Scryfall's tagger before being added. Names that sound
//! plausible but do not exist there — `wincon`, `stax`, `token`, `pump`, `reanimation` — are
//! deliberately absent, and `from_scryfall_tag` refuses them rather than guessing.

/// One functional role a card can have.
///
/// The discriminants are **stable**: they index bits in a [`TagSet`] that is written into the
/// card catalog. Reordering them silently changes the meaning of every published artifact, so
/// add new variants at the end and bump `mtg_data::FORMAT_VERSION`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Tag {
    // --- Cards and selection ---
    CardAdvantage = 0,
    Draw = 1,
    Cantrip = 2,
    Scry = 3,
    ImpulseDraw = 4,
    Wheel = 5,
    Tutor = 6,

    // --- Mana ---
    Ramp = 7,
    LandRamp = 8,
    ManaRock = 9,
    ManaDork = 10,
    Ritual = 11,
    CostReducer = 12,

    // --- Interaction ---
    Removal = 13,
    SpotRemoval = 14,
    BoardWipe = 15,
    Counterspell = 16,
    Protection = 17,
    Bounce = 18,
    Discard = 19,
    HandDisruption = 20,
    GraveyardHate = 21,
    CombatTrick = 22,

    // --- Engines and payoffs ---
    Recursion = 23,
    SacrificeOutlet = 24,
    DeathTrigger = 25,
    Blink = 26,
    CheatIntoPlay = 27,
    Lifegain = 28,
    Anthem = 29,
    Mill = 30,
    SelfMill = 31,
    Evasion = 32,
    ExtraTurn = 33,
    ExtraCombat = 34,
}

impl Tag {
    /// Every tag, in discriminant order.
    pub const ALL: [Tag; 35] = [
        Tag::CardAdvantage,
        Tag::Draw,
        Tag::Cantrip,
        Tag::Scry,
        Tag::ImpulseDraw,
        Tag::Wheel,
        Tag::Tutor,
        Tag::Ramp,
        Tag::LandRamp,
        Tag::ManaRock,
        Tag::ManaDork,
        Tag::Ritual,
        Tag::CostReducer,
        Tag::Removal,
        Tag::SpotRemoval,
        Tag::BoardWipe,
        Tag::Counterspell,
        Tag::Protection,
        Tag::Bounce,
        Tag::Discard,
        Tag::HandDisruption,
        Tag::GraveyardHate,
        Tag::CombatTrick,
        Tag::Recursion,
        Tag::SacrificeOutlet,
        Tag::DeathTrigger,
        Tag::Blink,
        Tag::CheatIntoPlay,
        Tag::Lifegain,
        Tag::Anthem,
        Tag::Mill,
        Tag::SelfMill,
        Tag::Evasion,
        Tag::ExtraTurn,
        Tag::ExtraCombat,
    ];

    /// The name Scryfall's tagger uses, which is also what `build-artifacts` queries.
    pub const fn scryfall_tag(self) -> &'static str {
        match self {
            Tag::CardAdvantage => "card-advantage",
            Tag::Draw => "draw",
            Tag::Cantrip => "cantrip",
            Tag::Scry => "scry",
            Tag::ImpulseDraw => "impulse-draw",
            Tag::Wheel => "wheel",
            Tag::Tutor => "tutor",
            Tag::Ramp => "ramp",
            Tag::LandRamp => "land-ramp",
            Tag::ManaRock => "mana-rock",
            Tag::ManaDork => "mana-dork",
            Tag::Ritual => "ritual",
            Tag::CostReducer => "cost-reducer",
            Tag::Removal => "removal",
            Tag::SpotRemoval => "spot-removal",
            Tag::BoardWipe => "board-wipe",
            Tag::Counterspell => "counterspell",
            Tag::Protection => "protection",
            Tag::Bounce => "bounce",
            Tag::Discard => "discard",
            Tag::HandDisruption => "hand-disruption",
            Tag::GraveyardHate => "graveyard-hate",
            Tag::CombatTrick => "combat-trick",
            Tag::Recursion => "recursion",
            Tag::SacrificeOutlet => "sacrifice-outlet",
            Tag::DeathTrigger => "death-trigger",
            Tag::Blink => "blink",
            Tag::CheatIntoPlay => "cheat-into-play",
            Tag::Lifegain => "lifegain",
            Tag::Anthem => "anthem",
            Tag::Mill => "mill",
            Tag::SelfMill => "self-mill",
            Tag::Evasion => "evasion",
            Tag::ExtraTurn => "extra-turn",
            Tag::ExtraCombat => "extra-combat",
        }
    }

    /// A label for a person to read.
    pub const fn label(self) -> &'static str {
        match self {
            Tag::CardAdvantage => "Card advantage",
            Tag::Draw => "Draw",
            Tag::Cantrip => "Cantrip",
            Tag::Scry => "Scry",
            Tag::ImpulseDraw => "Impulse draw",
            Tag::Wheel => "Wheel",
            Tag::Tutor => "Tutor",
            Tag::Ramp => "Ramp",
            Tag::LandRamp => "Land ramp",
            Tag::ManaRock => "Mana rock",
            Tag::ManaDork => "Mana dork",
            Tag::Ritual => "Ritual",
            Tag::CostReducer => "Cost reducer",
            Tag::Removal => "Removal",
            Tag::SpotRemoval => "Spot removal",
            Tag::BoardWipe => "Board wipe",
            Tag::Counterspell => "Counterspell",
            Tag::Protection => "Protection",
            Tag::Bounce => "Bounce",
            Tag::Discard => "Discard",
            Tag::HandDisruption => "Hand disruption",
            Tag::GraveyardHate => "Graveyard hate",
            Tag::CombatTrick => "Combat trick",
            Tag::Recursion => "Recursion",
            Tag::SacrificeOutlet => "Sacrifice outlet",
            Tag::DeathTrigger => "Death trigger",
            Tag::Blink => "Blink",
            Tag::CheatIntoPlay => "Cheat into play",
            Tag::Lifegain => "Lifegain",
            Tag::Anthem => "Anthem",
            Tag::Mill => "Mill",
            Tag::SelfMill => "Self-mill",
            Tag::Evasion => "Evasion",
            Tag::ExtraTurn => "Extra turn",
            Tag::ExtraCombat => "Extra combat",
        }
    }

    /// Parses a Scryfall tag name, or reports that the vocabulary has drifted.
    ///
    /// Returning `None` rather than guessing is deliberate: a tag that quietly stops matching
    /// would remove a whole role from every deck's analysis with nothing to explain it, which
    /// is the same failure the legality-key warning exists to catch.
    pub fn from_scryfall_tag(name: &str) -> Option<Tag> {
        Tag::ALL.into_iter().find(|tag| tag.scryfall_tag() == name)
    }

    pub const fn bit(self) -> u64 {
        1u64 << (self as u8)
    }
}

/// The roles one card has, as a bitset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TagSet(u64);

impl TagSet {
    pub const NONE: TagSet = TagSet(0);

    pub const fn from_bits(bits: u64) -> TagSet {
        TagSet(bits)
    }

    pub const fn bits(self) -> u64 {
        self.0
    }

    pub fn insert(&mut self, tag: Tag) {
        self.0 |= tag.bit();
    }

    pub const fn contains(self, tag: Tag) -> bool {
        self.0 & tag.bit() != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    pub const fn union(self, other: TagSet) -> TagSet {
        TagSet(self.0 | other.0)
    }

    pub const fn intersects(self, other: TagSet) -> bool {
        self.0 & other.0 != 0
    }

    pub fn iter(self) -> impl Iterator<Item = Tag> {
        Tag::ALL.into_iter().filter(move |tag| self.contains(*tag))
    }
}

impl FromIterator<Tag> for TagSet {
    fn from_iter<I: IntoIterator<Item = Tag>>(tags: I) -> TagSet {
        let mut set = TagSet::NONE;
        for tag in tags {
            set.insert(tag);
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tag_fits_in_the_bitset() {
        // The catalog stores this as a u64. A 65th tag would wrap around and alias an existing
        // one rather than failing, which is the sort of thing nobody notices for months.
        assert!(Tag::ALL.len() <= 64);
        for tag in Tag::ALL {
            assert!((tag as u8) < 64, "{tag:?} does not fit");
        }
    }

    #[test]
    fn discriminants_are_dense_and_in_order() {
        // They index bits in a published artifact. A gap or a reorder changes what every
        // existing `cards.rkyv` means.
        for (index, tag) in Tag::ALL.into_iter().enumerate() {
            assert_eq!(tag as usize, index, "{tag:?} is out of order");
        }
    }

    #[test]
    fn scryfall_names_are_unique() {
        // Two tags sharing a name would make one unreachable: the build would fetch the same
        // card list twice while a role stayed permanently empty.
        let mut names: Vec<&str> = Tag::ALL.iter().map(|tag| tag.scryfall_tag()).collect();
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn a_name_round_trips() {
        for tag in Tag::ALL {
            assert_eq!(Tag::from_scryfall_tag(tag.scryfall_tag()), Some(tag));
        }
    }

    #[test]
    fn an_unknown_name_is_refused_rather_than_guessed() {
        // Every one of these sounds like it should exist and does not; they were checked
        // against the tagger. A fuzzy match would hide the drift instead of reporting it.
        for name in ["wincon", "stax", "token", "pump", "reanimation", ""] {
            assert_eq!(Tag::from_scryfall_tag(name), None, "{name} was accepted");
        }
    }

    #[test]
    fn a_set_holds_what_was_put_in_it() {
        let set: TagSet = [Tag::Removal, Tag::Draw].into_iter().collect();
        assert!(set.contains(Tag::Removal));
        assert!(set.contains(Tag::Draw));
        assert!(!set.contains(Tag::Ramp));
        assert_eq!(set.len(), 2);
        assert_eq!(
            set.iter().collect::<Vec<_>>(),
            vec![Tag::Draw, Tag::Removal]
        );
    }

    #[test]
    fn an_empty_set_is_the_normal_case_for_a_vanilla_card() {
        // Grizzly Bears really does have no functional role — measured, it carries none of the
        // twelve broadest tags. That is information, not a gap in the data.
        assert!(TagSet::NONE.is_empty());
        assert_eq!(TagSet::NONE.len(), 0);
        assert!(TagSet::NONE.iter().next().is_none());
    }

    #[test]
    fn bits_survive_a_round_trip_through_the_catalog_representation() {
        let set: TagSet = [Tag::Evasion, Tag::ExtraCombat, Tag::CardAdvantage]
            .into_iter()
            .collect();
        assert_eq!(TagSet::from_bits(set.bits()), set);
    }
}
