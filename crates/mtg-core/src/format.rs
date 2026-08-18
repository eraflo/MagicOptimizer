//! Play formats and legality.
//!
//! The set of formats mirrors the keys of Scryfall's `legalities` object, so a card's
//! legality table can be stored as a fixed array indexed by [`Format`] with no lookup.

/// A constructed or limited play format.
///
/// This list mirrors the keys of Scryfall's `legalities` object as observed on 2026-08-17.
/// **It drifts**: Wizards adds and retires formats, and Scryfall follows. `explorer`, for one,
/// used to be here and is gone. Rather than hope the list stays correct, `build-artifacts`
/// reports any legality key it cannot map, so drift shows up at build time instead of
/// silently dropping legality data. See [`Format::from_scryfall_key`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Format {
    Standard,
    Future,
    Historic,
    Timeless,
    Gladiator,
    Pioneer,
    Modern,
    Legacy,
    Pauper,
    Vintage,
    Penny,
    Commander,
    Oathbreaker,
    StandardBrawl,
    Brawl,
    CompetitiveBrawl,
    Alchemy,
    PauperCommander,
    Duel,
    OldSchool,
    Premodern,
    Predh,
    /// Scryfall key `tlr`. A singleton, Commander-style format: ~19,000 legal cards, topped by
    /// Commander staples. The expansion of the abbreviation is not documented by Scryfall, so
    /// it is deliberately not spelled out here rather than guessed at.
    Tlr,
}

impl Format {
    /// Whether EDHREC's popularity rank means anything here.
    ///
    /// The rank counts appearances in Commander decks. In a Commander-shaped format that is a
    /// usable, if blunt, stand-in for how good a card is. In Modern it is close to noise:
    /// measured on a real burn list, the deck scored 0.18 because Goblin Guide and Lava Spike
    /// are barely played in Commander, and the optimizer answered by proposing Commander
    /// staples in their place. Anything reading `edhrec_rank` as quality must check this first.
    pub const fn edhrec_rank_is_meaningful(self) -> bool {
        matches!(
            self,
            Format::Commander
                | Format::Oathbreaker
                | Format::Brawl
                | Format::StandardBrawl
                | Format::CompetitiveBrawl
                | Format::PauperCommander
                | Format::Duel
                | Format::Predh
                | Format::Tlr
        )
    }

    /// Every format. Position matches [`Format::index`].
    pub const ALL: [Format; 23] = [
        Format::Standard,
        Format::Future,
        Format::Historic,
        Format::Timeless,
        Format::Gladiator,
        Format::Pioneer,
        Format::Modern,
        Format::Legacy,
        Format::Pauper,
        Format::Vintage,
        Format::Penny,
        Format::Commander,
        Format::Oathbreaker,
        Format::StandardBrawl,
        Format::Brawl,
        Format::CompetitiveBrawl,
        Format::Alchemy,
        Format::PauperCommander,
        Format::Duel,
        Format::OldSchool,
        Format::Premodern,
        Format::Predh,
        Format::Tlr,
    ];

    /// Number of formats, i.e. the size of a legality table.
    pub const COUNT: usize = Format::ALL.len();

    /// Index into a legality array. Matches the position in [`Format::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The key Scryfall uses in its `legalities` object.
    pub const fn scryfall_key(self) -> &'static str {
        match self {
            Format::Standard => "standard",
            Format::Future => "future",
            Format::Historic => "historic",
            Format::Timeless => "timeless",
            Format::Gladiator => "gladiator",
            Format::Pioneer => "pioneer",
            Format::Modern => "modern",
            Format::Legacy => "legacy",
            Format::Pauper => "pauper",
            Format::Vintage => "vintage",
            Format::Penny => "penny",
            Format::Commander => "commander",
            Format::Oathbreaker => "oathbreaker",
            Format::StandardBrawl => "standardbrawl",
            Format::Brawl => "brawl",
            Format::CompetitiveBrawl => "competitivebrawl",
            Format::Alchemy => "alchemy",
            Format::PauperCommander => "paupercommander",
            Format::Duel => "duel",
            Format::OldSchool => "oldschool",
            Format::Premodern => "premodern",
            Format::Predh => "predh",
            Format::Tlr => "tlr",
        }
    }

    /// Maps a Scryfall legality key to a format.
    ///
    /// Returns `None` for keys we do not model. Callers ingesting Scryfall data should surface
    /// that rather than swallow it: an unmapped key means a whole format's legality is being
    /// discarded, and the fix is to add a variant here.
    pub fn from_scryfall_key(key: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.scryfall_key() == key)
    }

    /// Human-readable name, for display.
    pub const fn display_name(self) -> &'static str {
        match self {
            Format::Standard => "Standard",
            Format::Future => "Future Standard",
            Format::Historic => "Historic",
            Format::Timeless => "Timeless",
            Format::Gladiator => "Gladiator",
            Format::Pioneer => "Pioneer",
            Format::Modern => "Modern",
            Format::Legacy => "Legacy",
            Format::Pauper => "Pauper",
            Format::Vintage => "Vintage",
            Format::Penny => "Penny Dreadful",
            Format::Commander => "Commander",
            Format::Oathbreaker => "Oathbreaker",
            Format::StandardBrawl => "Standard Brawl",
            Format::Brawl => "Brawl",
            Format::CompetitiveBrawl => "Competitive Brawl",
            Format::Alchemy => "Alchemy",
            Format::PauperCommander => "Pauper Commander",
            Format::Duel => "Duel Commander",
            Format::OldSchool => "Old School",
            Format::Premodern => "Premodern",
            Format::Predh => "PreDH",
            Format::Tlr => "TLR",
        }
    }

    /// True for formats built around a commander, which carry a color identity restriction.
    pub const fn is_singleton_commander(self) -> bool {
        matches!(
            self,
            Format::Commander
                | Format::Brawl
                | Format::StandardBrawl
                | Format::CompetitiveBrawl
                | Format::Oathbreaker
                | Format::PauperCommander
                | Format::Duel
                | Format::Predh
                | Format::Tlr
        )
    }
}

/// Serialized as the Scryfall key, not as the variant name.
///
/// The wire format therefore matches what the catalog stores and what the frontend already
/// sends, and it stays stable if a variant is ever renamed. A derived impl would emit
/// `"StandardBrawl"` where everything else in the system says `"standardbrawl"`.
impl serde::Serialize for Format {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.scryfall_key())
    }
}

impl<'de> serde::Deserialize<'de> for Format {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Format, D::Error> {
        let key = <std::borrow::Cow<'_, str>>::deserialize(deserializer)?;
        Format::from_scryfall_key(&key)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown format {key:?}")))
    }
}

/// How a card stands in a given format.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Legality {
    /// Playable without restriction.
    Legal,
    /// Not in the format's card pool at all. The default, so an unknown format key on an
    /// unfamiliar card degrades to "cannot play it" rather than "anything goes".
    #[default]
    NotLegal,
    /// Legal, but limited to a single copy. Vintage only.
    Restricted,
    /// In the pool but banned.
    Banned,
}

impl Legality {
    pub fn from_scryfall_value(value: &str) -> Legality {
        match value {
            "legal" => Legality::Legal,
            "restricted" => Legality::Restricted,
            "banned" => Legality::Banned,
            // "not_legal", and anything Scryfall adds later.
            _ => Legality::NotLegal,
        }
    }

    /// Whether the card may appear in a deck at all.
    pub const fn is_playable(self) -> bool {
        matches!(self, Legality::Legal | Legality::Restricted)
    }

    /// Maximum copies allowed by legality alone, before format deck rules apply.
    pub const fn max_copies(self) -> Option<u8> {
        match self {
            Legality::Legal => None,
            Legality::Restricted => Some(1),
            Legality::NotLegal | Legality::Banned => Some(0),
        }
    }
}

/// Rarity of a printing.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Rarity {
    #[default]
    Common,
    Uncommon,
    Rare,
    Mythic,
    Special,
    Bonus,
}

impl Rarity {
    pub fn from_scryfall_value(value: &str) -> Rarity {
        match value {
            "uncommon" => Rarity::Uncommon,
            "rare" => Rarity::Rare,
            "mythic" => Rarity::Mythic,
            "special" => Rarity::Special,
            "bonus" => Rarity::Bonus,
            _ => Rarity::Common,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn format_indexes_match_position_in_all() {
        for (position, format) in Format::ALL.into_iter().enumerate() {
            assert_eq!(format.index(), position, "{format:?}");
        }
    }

    #[test]
    fn scryfall_keys_round_trip() {
        for format in Format::ALL {
            assert_eq!(
                Format::from_scryfall_key(format.scryfall_key()),
                Some(format),
                "{format:?}"
            );
        }
    }

    #[test]
    fn scryfall_keys_are_unique() {
        let mut keys: Vec<&str> = Format::ALL.iter().map(|f| f.scryfall_key()).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count);
    }

    #[test]
    fn unknown_format_keys_are_ignored() {
        assert_eq!(Format::from_scryfall_key("some_new_format_2027"), None);
        // Retired by Scryfall: Explorer no longer appears in the legalities object.
        assert_eq!(Format::from_scryfall_key("explorer"), None);
    }

    #[test]
    fn format_list_matches_scryfall_as_observed() {
        // Pinned against the live `legalities` object on 2026-08-17. This test is expected to
        // fail one day: when it does, Scryfall has changed its format list, and the fix is to
        // update both this list and the enum. build-artifacts reports the same drift at build
        // time, so the two should never disagree for long.
        let observed = [
            "alchemy",
            "brawl",
            "commander",
            "competitivebrawl",
            "duel",
            "future",
            "gladiator",
            "historic",
            "legacy",
            "modern",
            "oathbreaker",
            "oldschool",
            "pauper",
            "paupercommander",
            "penny",
            "pioneer",
            "predh",
            "premodern",
            "standard",
            "standardbrawl",
            "timeless",
            "tlr",
            "vintage",
        ];
        assert_eq!(observed.len(), Format::COUNT);

        let mut ours: Vec<&str> = Format::ALL.iter().map(|f| f.scryfall_key()).collect();
        ours.sort_unstable();
        assert_eq!(ours, observed);
    }

    #[test]
    fn legality_parsing() {
        assert_eq!(Legality::from_scryfall_value("legal"), Legality::Legal);
        assert_eq!(Legality::from_scryfall_value("banned"), Legality::Banned);
        assert_eq!(
            Legality::from_scryfall_value("restricted"),
            Legality::Restricted
        );
        assert_eq!(
            Legality::from_scryfall_value("not_legal"),
            Legality::NotLegal
        );
    }

    #[test]
    fn unknown_legality_values_are_not_playable() {
        // A new legality string must never accidentally read as "legal".
        assert_eq!(
            Legality::from_scryfall_value("provisionally_legal"),
            Legality::NotLegal
        );
        assert!(!Legality::from_scryfall_value("provisionally_legal").is_playable());
    }

    #[test]
    fn restricted_allows_exactly_one_copy() {
        assert_eq!(Legality::Restricted.max_copies(), Some(1));
        assert_eq!(Legality::Banned.max_copies(), Some(0));
        assert_eq!(Legality::Legal.max_copies(), None);
    }

    #[test]
    fn formats_serialize_as_their_scryfall_key() {
        // Not as the variant name: everything else in the system speaks Scryfall keys, and a
        // renamed variant must not change the wire format.
        let json = serde_json::to_string(&Format::StandardBrawl).expect("serialize");
        assert_eq!(json, "\"standardbrawl\"");

        for format in Format::ALL {
            let json = serde_json::to_string(&format).expect("serialize");
            let back: Format = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, format);
        }
    }

    #[test]
    fn an_unknown_format_key_fails_to_deserialize() {
        // Rather than silently becoming some default format and mis-checking a whole deck.
        let error = serde_json::from_str::<Format>("\"explorer\"").unwrap_err();
        assert!(error.to_string().contains("unknown format"), "{error}");
    }

    #[test]
    fn commander_formats_are_flagged() {
        assert!(Format::Commander.is_singleton_commander());
        assert!(Format::Brawl.is_singleton_commander());
        assert!(!Format::Modern.is_singleton_commander());
        assert!(!Format::Pauper.is_singleton_commander());
    }
}
