//! Checking a deck against its format.
//!
//! The result is a list of named violations rather than a boolean. "Illegal" on its own is
//! useless to someone trying to fix a deck; "three too many cards" and "Sol Ring is banned in
//! Modern" are things you can act on.

use std::collections::{HashMap, HashSet};

use mtg_core::{ColorSet, Format, Legality};
use mtg_data::{ArchivedCard, Catalog};
use serde::{Deserialize, Serialize};

use crate::deck::{Deck, Zone};
use crate::rules::{DeckSize, FormatRules};

/// One thing wrong with a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Violation {
    /// The card is not in the loaded catalog. Usually a stale deck after a catalog change, or
    /// an import that resolved to nothing.
    UnknownCard {
        name: String,
    },
    DeckTooSmall {
        found: u32,
        required: u32,
    },
    DeckTooLarge {
        found: u32,
        allowed: u32,
    },
    SideboardTooLarge {
        found: u32,
        allowed: u32,
    },
    SideboardNotAllowed {
        found: u32,
    },
    TooManyCopies {
        name: String,
        found: u32,
        allowed: u32,
    },
    /// Legal nowhere in this format — usually a card from a set the format does not include.
    NotInFormat {
        name: String,
    },
    Banned {
        name: String,
    },
    Restricted {
        name: String,
        found: u32,
    },
    OutsideColorIdentity {
        name: String,
        card_identity: String,
        commander_identity: String,
    },
    CommandZoneSize {
        found: u32,
        minimum: u32,
        maximum: u32,
    },
    /// In the command zone but not something that can be a commander.
    NotAValidCommander {
        name: String,
    },
    /// This format has no command zone.
    CommandZoneNotAllowed {
        found: u32,
    },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Violation::UnknownCard { name } => {
                write!(f, "{name} is not in the loaded card data")
            }
            Violation::DeckTooSmall { found, required } => {
                write!(f, "the deck has {found} cards, {required} are required")
            }
            Violation::DeckTooLarge { found, allowed } => {
                write!(f, "the deck has {found} cards, only {allowed} are allowed")
            }
            Violation::SideboardTooLarge { found, allowed } => {
                write!(f, "the sideboard has {found} cards, only {allowed} are allowed")
            }
            Violation::SideboardNotAllowed { found } => {
                write!(f, "this format has no sideboard, but {found} cards are in one")
            }
            Violation::TooManyCopies { name, found, allowed } => {
                write!(f, "{found} copies of {name}, only {allowed} allowed")
            }
            Violation::NotInFormat { name } => write!(f, "{name} is not legal in this format"),
            Violation::Banned { name } => write!(f, "{name} is banned"),
            Violation::Restricted { name, found } => {
                write!(f, "{name} is restricted to one copy, the deck has {found}")
            }
            Violation::OutsideColorIdentity { name, card_identity, commander_identity } => write!(
                f,
                "{name} has colour identity {card_identity}, outside the commander's {commander_identity}"
            ),
            Violation::CommandZoneSize { found, minimum, maximum } => {
                if minimum == maximum {
                    write!(f, "the command zone has {found} cards, {minimum} is required")
                } else {
                    write!(
                        f,
                        "the command zone has {found} cards, between {minimum} and {maximum} are required"
                    )
                }
            }
            Violation::NotAValidCommander { name } => {
                write!(f, "{name} cannot be a commander")
            }
            Violation::CommandZoneNotAllowed { found } => {
                write!(f, "this format has no command zone, but {found} cards are in one")
            }
        }
    }
}

/// The outcome of checking a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalityReport {
    pub format: Format,
    /// True when the format's construction rules are inferred rather than confirmed. The UI
    /// should say so instead of presenting the verdict as certain.
    pub approximate_rules: bool,
    pub violations: Vec<Violation>,
    pub main_count: u32,
    pub sideboard_count: u32,
    pub command_count: u32,
    /// Combined colour identity of the command zone, as WUBRG letters.
    pub commander_identity: String,
}

impl LegalityReport {
    pub fn is_legal(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Checks a deck against the rules of its format.
pub fn check(deck: &Deck, catalog: &Catalog) -> LegalityReport {
    let rules = FormatRules::for_format(deck.format);
    let mut violations = Vec::new();

    // Resolve every card once. Entries whose card is missing are reported and then skipped, so
    // one stale entry does not cascade into a dozen misleading follow-on violations.
    //
    // `attempted` tracks lookups rather than successes: a card in both the main deck and the
    // sideboard is two entries, and keying off the success map alone would retry — and
    // re-report — a card that is not in the catalog once per entry.
    let mut cards: HashMap<&str, &ArchivedCard> = HashMap::new();
    let mut attempted: HashSet<&str> = HashSet::new();
    for entry in &deck.entries {
        if !attempted.insert(entry.oracle_id.as_str()) {
            continue;
        }
        match find_by_oracle_id(catalog, &entry.oracle_id) {
            Some(card) => {
                cards.insert(entry.oracle_id.as_str(), card);
            }
            None => violations.push(Violation::UnknownCard {
                name: entry.name.clone(),
            }),
        }
    }

    let main_count = deck.count_in(Zone::Main);
    let sideboard_count = deck.count_in(Zone::Sideboard);
    let command_count = deck.count_in(Zone::Command);

    let commander_identity = check_command_zone(deck, &rules, &cards, &mut violations);

    check_size(main_count, command_count, &rules, &mut violations);

    if rules.max_sideboard == 0 {
        if sideboard_count > 0 {
            violations.push(Violation::SideboardNotAllowed {
                found: sideboard_count,
            });
        }
    } else if sideboard_count > rules.max_sideboard {
        violations.push(Violation::SideboardTooLarge {
            found: sideboard_count,
            allowed: rules.max_sideboard,
        });
    }

    check_cards(deck, &rules, &cards, commander_identity, &mut violations);

    LegalityReport {
        format: deck.format,
        approximate_rules: rules.approximate,
        violations,
        main_count,
        sideboard_count,
        command_count,
        commander_identity: commander_identity
            .map(|c| c.to_string())
            .unwrap_or_default(),
    }
}

fn check_size(main_count: u32, command_count: u32, rules: &FormatRules, out: &mut Vec<Violation>) {
    let counted = match &rules.commander {
        Some(commander) if commander.counts_towards_deck_size => main_count + command_count,
        _ => main_count,
    };

    match rules.deck_size {
        DeckSize::Exactly(size) => {
            if counted < size {
                out.push(Violation::DeckTooSmall {
                    found: counted,
                    required: size,
                });
            } else if counted > size {
                out.push(Violation::DeckTooLarge {
                    found: counted,
                    allowed: size,
                });
            }
        }
        DeckSize::AtLeast(size) => {
            if counted < size {
                out.push(Violation::DeckTooSmall {
                    found: counted,
                    required: size,
                });
            }
        }
    }
}

/// Validates the command zone and returns the identity it allows, if the format enforces one.
fn check_command_zone(
    deck: &Deck,
    rules: &FormatRules,
    cards: &HashMap<&str, &ArchivedCard>,
    out: &mut Vec<Violation>,
) -> Option<ColorSet> {
    let command_count = deck.count_in(Zone::Command);

    let Some(commander_rules) = &rules.commander else {
        if command_count > 0 {
            out.push(Violation::CommandZoneNotAllowed {
                found: command_count,
            });
        }
        return None;
    };

    if !commander_rules.count.contains(&command_count) {
        out.push(Violation::CommandZoneSize {
            found: command_count,
            minimum: *commander_rules.count.start(),
            maximum: *commander_rules.count.end(),
        });
    }

    let mut identity = ColorSet::COLORLESS;
    for entry in deck.entries_in(Zone::Command) {
        let Some(card) = cards.get(entry.oracle_id.as_str()) else {
            continue;
        };
        identity = identity.union(card.color_identity());

        // Oathbreaker's signature spell is not a commander in the usual sense, so the check
        // only applies where the command zone holds actual commanders.
        if is_commander_format(rules.format) && !card.can_be_commander() {
            out.push(Violation::NotAValidCommander {
                name: entry.name.clone(),
            });
        }
    }

    commander_rules.enforce_color_identity.then_some(identity)
}

fn is_commander_format(format: Format) -> bool {
    !matches!(format, Format::Oathbreaker)
}

fn check_cards(
    deck: &Deck,
    rules: &FormatRules,
    cards: &HashMap<&str, &ArchivedCard>,
    commander_identity: Option<ColorSet>,
    out: &mut Vec<Violation>,
) {
    // One pass per distinct card rather than per entry, so a card in both the main deck and
    // the sideboard is only reported once.
    let mut seen: Vec<&str> = Vec::new();
    for entry in &deck.entries {
        if seen.contains(&entry.oracle_id.as_str()) {
            continue;
        }
        seen.push(&entry.oracle_id);

        let Some(card) = cards.get(entry.oracle_id.as_str()) else {
            continue;
        };

        match card.legality(rules.format) {
            Legality::Legal => {}
            Legality::NotLegal => out.push(Violation::NotInFormat {
                name: entry.name.clone(),
            }),
            Legality::Banned => out.push(Violation::Banned {
                name: entry.name.clone(),
            }),
            Legality::Restricted => {
                let copies = deck.copies_of(&entry.oracle_id);
                if copies > 1 {
                    out.push(Violation::Restricted {
                        name: entry.name.clone(),
                        found: copies,
                    });
                }
            }
        }

        let allowed = copy_limit(card, rules.max_copies);
        let copies = deck.copies_of(&entry.oracle_id);
        if copies > allowed {
            out.push(Violation::TooManyCopies {
                name: entry.name.clone(),
                found: copies,
                allowed,
            });
        }

        if let Some(allowed_identity) = commander_identity {
            let card_identity = card.color_identity();
            if !card_identity.is_subset_of(allowed_identity) {
                out.push(Violation::OutsideColorIdentity {
                    name: entry.name.clone(),
                    card_identity: card_identity.to_string(),
                    commander_identity: allowed_identity.to_string(),
                });
            }
        }
    }
}

/// How many copies of this particular card are allowed.
///
/// Two exceptions to the format's limit: basic lands are unlimited, and a handful of cards
/// grant their own allowance in their rules text — Relentless Rats and friends.
fn copy_limit(card: &ArchivedCard, format_limit: u32) -> u32 {
    if card.has_type("Basic") && card.has_type("Land") {
        return u32::MAX;
    }

    let text = card.oracle_text();
    if text.contains("can have any number of cards named") {
        return u32::MAX;
    }
    // The Nazgûl, which allows exactly nine rather than unlimited.
    if text.contains("can have up to nine cards named") {
        return 9;
    }

    format_limit
}

fn find_by_oracle_id<'a>(catalog: &'a Catalog, oracle_id: &str) -> Option<&'a ArchivedCard> {
    catalog
        .iter()
        .find(|(_, card)| card.oracle_id() == oracle_id)
        .map(|(_, card)| card)
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::deck::DeckEntry;
    use mtg_core::Rarity;
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card, CardFace, CatalogData, Layout, LEGALITY_SLOTS,
    };

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
                type_line: "Instant".to_owned(),
                oracle_text: String::new(),
                power: None,
                toughness: None,
                loyalty: None,
                keywords: Vec::new(),
                legalities: [legality_to_u8(Legality::Legal); LEGALITY_SLOTS],
                rarity: rarity_to_u8(Rarity::Common),
                edhrec_rank: None,
                game_changer: false,
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

        fn identity(mut self, symbols: &str) -> Builder {
            self.0.color_identity = ColorSet::from_symbols(symbols).bits();
            self
        }

        fn legality(mut self, format: Format, legality: Legality) -> Builder {
            self.0.legalities[format.index()] = legality_to_u8(legality);
            self
        }

        fn build(mut self) -> Card {
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
            self.0
        }
    }

    fn catalog(cards: Vec<Card>) -> Catalog {
        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards,
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    /// A catalog with the pieces the tests below reach for.
    fn sample_catalog() -> Catalog {
        catalog(vec![
            Builder::new("Island").types("Basic Land — Island").build(),
            Builder::new("Lightning Bolt").identity("R").build(),
            Builder::new("Counterspell").identity("U").build(),
            Builder::new("Krenko, Mob Boss")
                .types("Legendary Creature — Goblin Warrior")
                .identity("R")
                .build(),
            Builder::new("Sol Ring")
                .types("Artifact")
                .legality(Format::Modern, Legality::Banned)
                .legality(Format::Vintage, Legality::Restricted)
                .build(),
            Builder::new("Relentless Rats")
                .types("Creature — Rat")
                .identity("B")
                .text("A deck can have any number of cards named Relentless Rats.")
                .build(),
            Builder::new("Ancestral Recall")
                .identity("U")
                .legality(Format::Modern, Legality::NotLegal)
                .build(),
        ])
    }

    fn oracle(name: &str) -> String {
        format!("oracle-{name}")
    }

    /// A legal 60-card Modern deck: 56 Islands and the four Counterspells the limit allows.
    fn modern_deck() -> Deck {
        let mut deck = Deck::new("Test", Format::Modern);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 56));
        deck.add(DeckEntry::new(oracle("Counterspell"), "Counterspell", 4));
        deck
    }

    #[test]
    fn a_legal_deck_reports_no_violations() {
        let report = check(&modern_deck(), &sample_catalog());
        assert!(report.is_legal(), "{:?}", report.violations);
        assert_eq!(report.main_count, 60);
    }

    #[test]
    fn a_short_deck_is_reported_with_numbers() {
        let mut deck = Deck::new("Test", Format::Modern);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 55));

        let report = check(&deck, &sample_catalog());
        assert_eq!(
            report.violations,
            [Violation::DeckTooSmall {
                found: 55,
                required: 60
            }]
        );
    }

    #[test]
    fn sixty_card_formats_have_no_maximum() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new(oracle("Island"), "Island", 20));
        assert!(check(&deck, &sample_catalog()).is_legal());
    }

    #[test]
    fn basic_lands_ignore_the_copy_limit() {
        // 40 Islands would otherwise be 36 too many.
        let report = check(&modern_deck(), &sample_catalog());
        assert!(!report
            .violations
            .iter()
            .any(|v| matches!(v, Violation::TooManyCopies { .. })));
    }

    #[test]
    fn too_many_copies_of_a_normal_card() {
        let mut deck = Deck::new("Test", Format::Modern);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 55));
        deck.add(DeckEntry::new(
            oracle("Lightning Bolt"),
            "Lightning Bolt",
            5,
        ));

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::TooManyCopies {
            name: "Lightning Bolt".to_owned(),
            found: 5,
            allowed: 4,
        }));
    }

    #[test]
    fn cards_that_grant_their_own_allowance_are_exempt() {
        // Relentless Rats says so in its own rules text.
        let mut deck = Deck::new("Test", Format::Modern);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 20));
        deck.add(DeckEntry::new(
            oracle("Relentless Rats"),
            "Relentless Rats",
            40,
        ));

        let report = check(&deck, &sample_catalog());
        assert!(report.is_legal(), "{:?}", report.violations);
    }

    #[test]
    fn copies_span_the_main_deck_and_sideboard() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new(
            oracle("Lightning Bolt"),
            "Lightning Bolt",
            3,
        ));
        deck.add(
            DeckEntry::new(oracle("Lightning Bolt"), "Lightning Bolt", 3).in_zone(Zone::Sideboard),
        );

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::TooManyCopies {
            name: "Lightning Bolt".to_owned(),
            found: 6,
            allowed: 4,
        }));
        // And it is reported once, not once per zone.
        assert_eq!(
            report
                .violations
                .iter()
                .filter(|v| matches!(v, Violation::TooManyCopies { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn banned_and_absent_cards_are_told_apart() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new(oracle("Sol Ring"), "Sol Ring", 1));
        deck.add(DeckEntry::new(
            oracle("Ancestral Recall"),
            "Ancestral Recall",
            1,
        ));

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::Banned {
            name: "Sol Ring".to_owned()
        }));
        assert!(report.violations.contains(&Violation::NotInFormat {
            name: "Ancestral Recall".to_owned()
        }));
    }

    #[test]
    fn restricted_cards_allow_exactly_one() {
        let mut one = Deck::new("Test", Format::Vintage);
        one.add(DeckEntry::new(oracle("Island"), "Island", 59));
        one.add(DeckEntry::new(oracle("Sol Ring"), "Sol Ring", 1));
        assert!(check(&one, &sample_catalog()).is_legal());

        let mut two = Deck::new("Test", Format::Vintage);
        two.add(DeckEntry::new(oracle("Island"), "Island", 58));
        two.add(DeckEntry::new(oracle("Sol Ring"), "Sol Ring", 2));
        assert!(check(&two, &sample_catalog())
            .violations
            .contains(&Violation::Restricted {
                name: "Sol Ring".to_owned(),
                found: 2
            }));
    }

    #[test]
    fn sideboard_limit() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new(oracle("Island"), "Island", 16).in_zone(Zone::Sideboard));

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::SideboardTooLarge {
            found: 16,
            allowed: 15
        }));
    }

    // --- Commander --------------------------------------------------------------------

    /// A legal Commander deck: Krenko plus 99 Mountains, using Islands for convenience.
    fn commander_deck() -> Deck {
        let mut deck = Deck::new("Krenko", Format::Commander);
        deck.add(
            DeckEntry::new(oracle("Krenko, Mob Boss"), "Krenko, Mob Boss", 1)
                .in_zone(Zone::Command),
        );
        deck.add(DeckEntry::new(oracle("Island"), "Island", 99));
        deck
    }

    #[test]
    fn the_commander_counts_towards_the_hundred() {
        let report = check(&commander_deck(), &sample_catalog());
        assert!(report.is_legal(), "{:?}", report.violations);
        assert_eq!(report.main_count, 99);
        assert_eq!(report.command_count, 1);
    }

    #[test]
    fn a_missing_commander_is_reported() {
        let mut deck = Deck::new("Nobody", Format::Commander);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 100));

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::CommandZoneSize {
            found: 0,
            minimum: 1,
            maximum: 2
        }));
    }

    #[test]
    fn a_non_legendary_card_cannot_be_the_commander() {
        let mut deck = Deck::new("Bolt", Format::Commander);
        deck.add(
            DeckEntry::new(oracle("Lightning Bolt"), "Lightning Bolt", 1).in_zone(Zone::Command),
        );
        deck.add(DeckEntry::new(oracle("Island"), "Island", 99));

        let report = check(&deck, &sample_catalog());
        assert!(report.violations.contains(&Violation::NotAValidCommander {
            name: "Lightning Bolt".to_owned()
        }));
    }

    #[test]
    fn colour_identity_is_enforced_against_the_commander() {
        let mut deck = commander_deck();
        deck.remove(&oracle("Island"), Zone::Main, 1);
        deck.add(DeckEntry::new(oracle("Counterspell"), "Counterspell", 1));

        let report = check(&deck, &sample_catalog());
        assert!(report
            .violations
            .contains(&Violation::OutsideColorIdentity {
                name: "Counterspell".to_owned(),
                card_identity: "U".to_owned(),
                commander_identity: "R".to_owned(),
            }));
        assert_eq!(report.commander_identity, "R");
    }

    #[test]
    fn colourless_cards_fit_any_commander() {
        // Sol Ring in a mono-red deck: legal, because the colourless identity is a subset of
        // everything.
        let mut deck = commander_deck();
        deck.remove(&oracle("Island"), Zone::Main, 1);
        deck.add(DeckEntry::new(oracle("Sol Ring"), "Sol Ring", 1));

        let report = check(&deck, &sample_catalog());
        assert!(report.is_legal(), "{:?}", report.violations);
    }

    #[test]
    fn commander_is_singleton_but_basics_are_still_unlimited() {
        let report = check(&commander_deck(), &sample_catalog());
        assert!(report.is_legal(), "99 Islands must be fine");
    }

    #[test]
    fn a_command_zone_in_a_format_without_one_is_reported() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new(oracle("Krenko, Mob Boss"), "Krenko", 1).in_zone(Zone::Command));

        let report = check(&deck, &sample_catalog());
        assert!(report
            .violations
            .contains(&Violation::CommandZoneNotAllowed { found: 1 }));
    }

    #[test]
    fn an_unknown_card_is_reported_once_and_does_not_cascade() {
        let mut deck = modern_deck();
        deck.add(DeckEntry::new("oracle-nonexistent", "Ghost Card", 4));
        deck.add(DeckEntry::new("oracle-nonexistent", "Ghost Card", 2).in_zone(Zone::Sideboard));

        let report = check(&deck, &sample_catalog());
        let unknown: Vec<&Violation> = report
            .violations
            .iter()
            .filter(|v| matches!(v, Violation::UnknownCard { .. }))
            .collect();
        assert_eq!(unknown.len(), 1);
        // No copy-limit or legality noise generated from the card we could not look up.
        assert_eq!(
            report.violations.len(),
            1,
            "unexpected extra: {:?}",
            report.violations
        );
    }

    #[test]
    fn approximate_rules_are_flagged_in_the_report() {
        let mut deck = Deck::new("Test", Format::Tlr);
        deck.add(DeckEntry::new(oracle("Island"), "Island", 100));
        assert!(check(&deck, &sample_catalog()).approximate_rules);

        assert!(!check(&modern_deck(), &sample_catalog()).approximate_rules);
    }

    #[test]
    fn violations_read_as_sentences() {
        // These strings go straight into the UI, so they have to make sense on their own.
        assert_eq!(
            Violation::Banned {
                name: "Sol Ring".to_owned()
            }
            .to_string(),
            "Sol Ring is banned"
        );
        assert_eq!(
            Violation::DeckTooSmall {
                found: 55,
                required: 60
            }
            .to_string(),
            "the deck has 55 cards, 60 are required"
        );
        assert_eq!(
            Violation::CommandZoneSize {
                found: 0,
                minimum: 1,
                maximum: 1
            }
            .to_string(),
            "the command zone has 0 cards, 1 is required"
        );
    }
}
