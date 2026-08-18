//! Searching and filtering the catalog.
//!
//! Deliberately a linear scan. The catalog is ~35,000 cards, and walking it with these
//! predicates costs a few milliseconds — well under what a UI needs. Inverted indexes and
//! bitsets are easy to add later, but adding them before a measurement says they are needed
//! would be cost with no benefit. See `docs/dev/architecture.md`.

use mtg_core::{CardId, ColorSet, Format};

use crate::card::ArchivedCard;
use crate::catalog::Catalog;

/// How a card's colors must relate to the colors given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorMatch {
    /// Exactly these colors, no more and no fewer.
    Exactly(ColorSet),
    /// At least these colors.
    Including(ColorSet),
    /// Nothing outside these colors. This is the Commander identity rule.
    Within(ColorSet),
}

/// A catalog query.
///
/// Filters combine with AND. An empty query matches every card.
///
/// ```
/// # use mtg_data::Query;
/// # use mtg_core::{ColorSet, Format};
/// let query = Query::new()
///     .text("draw a card")
///     .card_type("Instant")
///     .identity_within(ColorSet::from_symbols("WU"))
///     .legal_in(Format::Commander)
///     .limit(20);
/// ```
#[derive(Debug, Clone, Default)]
pub struct Query {
    text: Option<String>,
    name: Option<String>,
    colors: Option<ColorMatch>,
    card_types: Vec<String>,
    format: Option<Format>,
    min_mana_value: Option<f32>,
    max_mana_value: Option<f32>,
    game_changer: Option<bool>,
    can_be_commander: Option<bool>,
    limit: Option<usize>,
}

impl Query {
    pub fn new() -> Query {
        Query::default()
    }

    /// Matches a substring of the card's name or oracle text, ignoring ASCII case.
    pub fn text(mut self, text: &str) -> Query {
        self.text = Some(text.to_lowercase());
        self
    }

    /// Matches a substring of the card's name, ignoring ASCII case.
    pub fn name(mut self, name: &str) -> Query {
        self.name = Some(name.to_lowercase());
        self
    }

    /// Exactly these colors.
    pub fn colors_exactly(mut self, colors: ColorSet) -> Query {
        self.colors = Some(ColorMatch::Exactly(colors));
        self
    }

    /// At least these colors, possibly more.
    pub fn colors_including(mut self, colors: ColorSet) -> Query {
        self.colors = Some(ColorMatch::Including(colors));
        self
    }

    /// Color identity within these colors: the Commander deck-building rule.
    pub fn identity_within(mut self, identity: ColorSet) -> Query {
        self.colors = Some(ColorMatch::Within(identity));
        self
    }

    /// Requires a word on the type line, e.g. `"Creature"` or `"Legendary"`.
    /// Repeating this requires all of them.
    pub fn card_type(mut self, kind: &str) -> Query {
        self.card_types.push(kind.to_owned());
        self
    }

    /// Requires the card to be playable in a format, which includes restricted cards.
    pub fn legal_in(mut self, format: Format) -> Query {
        self.format = Some(format);
        self
    }

    pub fn mana_value_at_least(mut self, value: f32) -> Query {
        self.min_mana_value = Some(value);
        self
    }

    pub fn mana_value_at_most(mut self, value: f32) -> Query {
        self.max_mana_value = Some(value);
        self
    }

    /// Filters on Scryfall's official Commander Game Changers flag.
    pub fn game_changer(mut self, flag: bool) -> Query {
        self.game_changer = Some(flag);
        self
    }

    pub fn can_be_commander(mut self, flag: bool) -> Query {
        self.can_be_commander = Some(flag);
        self
    }

    /// Stops after this many matches.
    pub fn limit(mut self, limit: usize) -> Query {
        self.limit = Some(limit);
        self
    }

    /// True when a single card satisfies every filter.
    pub fn matches(&self, card: &ArchivedCard) -> bool {
        if let Some(needle) = &self.name {
            if !contains_ignore_ascii_case(card.name(), needle) {
                return false;
            }
        }

        if let Some(needle) = &self.text {
            let in_name = contains_ignore_ascii_case(card.name(), needle);
            let in_text = contains_ignore_ascii_case(card.oracle_text(), needle);
            // Faces carry their own text; a search for "trample" must find a card whose back
            // face has it.
            let in_faces = card.faces().iter().any(|face| {
                contains_ignore_ascii_case(face.oracle_text(), needle)
                    || contains_ignore_ascii_case(face.name(), needle)
            });
            if !in_name && !in_text && !in_faces {
                return false;
            }
        }

        if let Some(rule) = self.colors {
            let matched = match rule {
                ColorMatch::Exactly(wanted) => card.colors() == wanted,
                ColorMatch::Including(wanted) => wanted.is_subset_of(card.colors()),
                ColorMatch::Within(allowed) => card.color_identity().is_subset_of(allowed),
            };
            if !matched {
                return false;
            }
        }

        if !self
            .card_types
            .iter()
            .all(|kind| card.has_type(kind.as_str()))
        {
            return false;
        }

        if let Some(format) = self.format {
            if !card.is_legal_in(format) {
                return false;
            }
        }

        let mana_value = card.mana_value();
        if let Some(min) = self.min_mana_value {
            if mana_value < min {
                return false;
            }
        }
        if let Some(max) = self.max_mana_value {
            if mana_value > max {
                return false;
            }
        }

        if let Some(flag) = self.game_changer {
            if card.is_game_changer() != flag {
                return false;
            }
        }

        if let Some(flag) = self.can_be_commander {
            if card.can_be_commander() != flag {
                return false;
            }
        }

        true
    }

    /// Runs the query, in catalog order.
    pub fn execute<'a>(&self, catalog: &'a Catalog) -> Vec<(CardId, &'a ArchivedCard)> {
        let limit = self.limit.unwrap_or(usize::MAX);
        catalog
            .iter()
            .filter(|(_, card)| self.matches(card))
            .take(limit)
            .collect()
    }

    /// Counts matches without collecting them. Ignores any limit.
    pub fn count(&self, catalog: &Catalog) -> usize {
        catalog
            .iter()
            .filter(|(_, card)| self.matches(card))
            .count()
    }
}

/// Case-insensitive substring search, with `needle` already lowercased.
///
/// Avoids allocating a lowercased copy of every card's text on every query, which is the
/// obvious implementation and by far the most expensive part of a scan.
fn contains_ignore_ascii_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let haystack = haystack.as_bytes();
    let needle = needle_lower.as_bytes();
    if needle.len() > haystack.len() {
        return false;
    }
    // Comparing raw bytes is safe for a lowercase ASCII needle: UTF-8 continuation bytes are
    // all >= 0x80 and so can never equal one.
    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle)
            .all(|(h, n)| h.to_ascii_lowercase() == *n)
    })
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::card::{legality_to_u8, rarity_to_u8, Card, CardFace, Layout, LEGALITY_SLOTS};
    use crate::catalog::{serialize, CatalogData, FORMAT_VERSION};
    use mtg_core::{Legality, Rarity};

    struct Builder(Card);

    impl Builder {
        fn new(name: &str) -> Builder {
            Builder(Card {
                oracle_id: format!("oracle-{name}"),
                name: name.to_owned(),
                mana_cost: String::new(),
                mana_value: 0.0,
                colors: 0,
                color_identity: 0,
                produced_mana: 0,
                type_line: "Instant".to_owned(),
                oracle_text: String::new(),
                power: None,
                toughness: None,
                loyalty: None,
                keywords: Vec::new(),
                legalities: [legality_to_u8(Legality::NotLegal); LEGALITY_SLOTS],
                rarity: rarity_to_u8(Rarity::Common),
                edhrec_rank: None,
                game_changer: false,
                tags: 0,
                reserved: false,
                layout: Layout::Normal,
                faces: Vec::new(),
                set_code: "tst".to_owned(),
                collector_number: "1".to_owned(),
                released_at: "2026-01-01".to_owned(),
                image_id: String::new(),
            })
        }

        fn types(mut self, type_line: &str) -> Builder {
            self.0.type_line = type_line.to_owned();
            self
        }

        fn text(mut self, text: &str) -> Builder {
            self.0.oracle_text = text.to_owned();
            self
        }

        fn colors(mut self, symbols: &str) -> Builder {
            let set = ColorSet::from_symbols(symbols);
            self.0.colors = set.bits();
            self.0.color_identity = set.bits();
            self
        }

        fn mana_value(mut self, value: f32) -> Builder {
            self.0.mana_value = value;
            self
        }

        fn legal(mut self, format: Format) -> Builder {
            self.0.legalities[format.index()] = legality_to_u8(Legality::Legal);
            self
        }

        fn game_changer(mut self) -> Builder {
            self.0.game_changer = true;
            self
        }

        fn face(mut self, name: &str, text: &str) -> Builder {
            self.0.faces.push(CardFace {
                name: name.to_owned(),
                mana_cost: String::new(),
                type_line: "Creature".to_owned(),
                oracle_text: text.to_owned(),
                power: None,
                toughness: None,
                loyalty: None,
                colors: 0,
            });
            self
        }

        fn build(mut self) -> Card {
            if self.0.faces.is_empty() {
                self.0.faces.push(CardFace {
                    name: self.0.name.clone(),
                    mana_cost: self.0.mana_cost.clone(),
                    type_line: self.0.type_line.clone(),
                    oracle_text: self.0.oracle_text.clone(),
                    power: None,
                    toughness: None,
                    loyalty: None,
                    colors: self.0.colors,
                });
            }
            self.0
        }
    }

    fn sample() -> Catalog {
        let cards = vec![
            Builder::new("Counterspell")
                .types("Instant")
                .text("Counter target spell.")
                .colors("U")
                .mana_value(2.0)
                .legal(Format::Modern)
                .legal(Format::Commander)
                .build(),
            Builder::new("Llanowar Elves")
                .types("Creature — Elf Druid")
                .text("{T}: Add {G}.")
                .colors("G")
                .mana_value(1.0)
                .legal(Format::Commander)
                .build(),
            Builder::new("Atraxa, Praetors' Voice")
                .types("Legendary Creature — Phyrexian Angel Horror")
                .text("Flying, vigilance, deathtouch, lifelink.")
                .colors("WUBG")
                .mana_value(4.0)
                .legal(Format::Commander)
                .build(),
            Builder::new("Mana Vault")
                .types("Artifact")
                .text("{T}: Add {C}{C}{C}.")
                .mana_value(1.0)
                .legal(Format::Commander)
                .game_changer()
                .build(),
            Builder::new("Delver of Secrets // Insectile Aberration")
                .types("Creature — Human Wizard")
                .colors("U")
                .mana_value(1.0)
                .legal(Format::Modern)
                .face(
                    "Delver of Secrets",
                    "At the beginning of your upkeep, look at the top card.",
                )
                .face("Insectile Aberration", "Flying")
                .build(),
        ];

        let data = CatalogData {
            format_version: FORMAT_VERSION,
            source_updated_at: String::new(),
            cards,
        };
        Catalog::from_bytes(serialize(&data).unwrap()).unwrap()
    }

    fn names(results: Vec<(CardId, &ArchivedCard)>) -> Vec<String> {
        results
            .into_iter()
            .map(|(_, c)| c.name().to_owned())
            .collect()
    }

    #[test]
    fn empty_query_matches_everything() {
        let catalog = sample();
        assert_eq!(Query::new().count(&catalog), catalog.len());
    }

    #[test]
    fn text_search_is_case_insensitive() {
        let catalog = sample();
        for needle in ["counter target", "COUNTER TARGET", "Counter Target"] {
            assert_eq!(
                names(Query::new().text(needle).execute(&catalog)),
                ["Counterspell"]
            );
        }
    }

    #[test]
    fn text_search_reaches_the_back_face() {
        // The reason faces carry their own text: "Flying" only appears on the back of Delver.
        let catalog = sample();
        let found = names(Query::new().text("flying").execute(&catalog));
        assert!(found.contains(&"Delver of Secrets // Insectile Aberration".to_owned()));
    }

    #[test]
    fn type_filter_is_whole_word() {
        let catalog = sample();
        let creatures = names(Query::new().card_type("Creature").execute(&catalog));
        assert_eq!(creatures.len(), 3);

        let legends = names(
            Query::new()
                .card_type("Legendary")
                .card_type("Creature")
                .execute(&catalog),
        );
        assert_eq!(legends, ["Atraxa, Praetors' Voice"]);
    }

    #[test]
    fn commander_identity_filter() {
        let catalog = sample();
        // A mono-blue commander may play blue and colorless cards, nothing else.
        let playable = names(
            Query::new()
                .identity_within(ColorSet::from_symbols("U"))
                .execute(&catalog),
        );
        assert!(playable.contains(&"Counterspell".to_owned()));
        assert!(
            playable.contains(&"Mana Vault".to_owned()),
            "colorless is always legal"
        );
        assert!(!playable.contains(&"Llanowar Elves".to_owned()));
        assert!(!playable.contains(&"Atraxa, Praetors' Voice".to_owned()));
    }

    #[test]
    fn exact_and_including_color_filters_differ() {
        let catalog = sample();

        let exactly_blue = names(
            Query::new()
                .colors_exactly(ColorSet::from_symbols("U"))
                .execute(&catalog),
        );
        assert_eq!(exactly_blue.len(), 2);

        let including_blue = names(
            Query::new()
                .colors_including(ColorSet::from_symbols("U"))
                .execute(&catalog),
        );
        // Atraxa is blue among other colors, so it is included here but not above.
        assert!(including_blue.contains(&"Atraxa, Praetors' Voice".to_owned()));
    }

    #[test]
    fn format_filter() {
        let catalog = sample();
        assert_eq!(Query::new().legal_in(Format::Commander).count(&catalog), 4);
        assert_eq!(Query::new().legal_in(Format::Modern).count(&catalog), 2);
        assert_eq!(Query::new().legal_in(Format::Standard).count(&catalog), 0);
    }

    #[test]
    fn mana_value_range() {
        let catalog = sample();
        assert_eq!(
            Query::new().mana_value_at_most(1.0).count(&catalog),
            3,
            "three one-drops"
        );
        assert_eq!(Query::new().mana_value_at_least(4.0).count(&catalog), 1);
    }

    #[test]
    fn game_changer_filter_uses_the_scryfall_flag() {
        let catalog = sample();
        assert_eq!(
            names(Query::new().game_changer(true).execute(&catalog)),
            ["Mana Vault"]
        );
    }

    #[test]
    fn commander_filter() {
        let catalog = sample();
        assert_eq!(
            names(Query::new().can_be_commander(true).execute(&catalog)),
            ["Atraxa, Praetors' Voice"]
        );
    }

    #[test]
    fn filters_combine_with_and() {
        let catalog = sample();
        let results = names(
            Query::new()
                .card_type("Creature")
                .legal_in(Format::Commander)
                .identity_within(ColorSet::from_symbols("G"))
                .execute(&catalog),
        );
        assert_eq!(results, ["Llanowar Elves"]);
    }

    #[test]
    fn limit_stops_early() {
        let catalog = sample();
        assert_eq!(Query::new().limit(2).execute(&catalog).len(), 2);
        // count ignores the limit, which is what makes "showing 2 of 5" possible.
        assert_eq!(Query::new().limit(2).count(&catalog), 5);
    }

    #[test]
    fn case_insensitive_contains_handles_non_ascii_text() {
        // Type lines use an em dash, and French cards carry accents. Byte windows must not
        // produce false positives or panic on a char boundary.
        assert!(contains_ignore_ascii_case("Créature — Elfe", "elfe"));
        assert!(!contains_ignore_ascii_case("Créature — Elfe", "creature"));
        assert!(contains_ignore_ascii_case("Créature — Elfe", "ature"));
    }

    #[test]
    fn needle_longer_than_haystack_is_not_a_match() {
        assert!(!contains_ignore_ascii_case("ab", "abcdef"));
        assert!(contains_ignore_ascii_case("anything", ""));
    }
}
