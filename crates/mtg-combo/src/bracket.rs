//! Estimating which Commander bracket a deck belongs to.
//!
//! Wizards' bracket system runs 1 (Exhibition) to 5 (cEDH). The published criteria are about
//! expected game length, with three concrete markers: cards from the official Game Changers
//! list, two-card infinite combos, and mass land denial or chained extra turns.
//!
//! # What this cannot tell you
//!
//! **Brackets 1 and 5 are about intent, not contents.** Bracket 1 is a deck built around a
//! theme where winning is not the point; bracket 5 is a deck built to win a tournament. Two
//! decks with identical cards can sit in 2 and 1, or in 4 and 5, depending only on how they
//! are played. Nothing here can see that, so the estimate is deliberately bounded to 2–4 and
//! says so rather than pretending to a precision it does not have.

use std::collections::HashSet;

use mtg_data::{ArchivedCard, Catalog};
use mtg_deck::{Deck, Zone};
use serde::{Deserialize, Serialize};

use crate::combo::ComboDatabase;
use crate::detect::{ComboIndex, ComboMatch};

/// Most Game Changers a bracket 3 deck may hold. Brackets 1 and 2 allow none.
const BRACKET_3_GAME_CHANGER_LIMIT: usize = 3;

/// A card that pushed the estimate up, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Marker {
    pub name: String,
    /// What about it matters, in a few words.
    pub note: String,
}

/// What bracket a deck looks like, and what led there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BracketAssessment {
    /// Between 2 and 4. See the module docs for why 1 and 5 are not reachable.
    pub bracket: u8,
    /// Sentences explaining the number, in the order they were applied.
    pub reasons: Vec<String>,
    pub game_changers: Vec<Marker>,
    /// Two-card infinite combos, which are what the bracket rules single out.
    pub two_card_combos: Vec<ComboMatch>,
    /// Longer combos, reported for information without moving the bracket.
    pub longer_combos: Vec<ComboMatch>,
    pub mass_land_denial: Vec<Marker>,
    pub extra_turns: Vec<Marker>,
    pub tutors: Vec<Marker>,
    /// Things the estimate could not check, stated so the number is not read as certainty.
    pub caveats: Vec<String>,
}

impl BracketAssessment {
    pub fn label(&self) -> &'static str {
        match self.bracket {
            1 => "Exhibition",
            2 => "Core",
            3 => "Upgraded",
            4 => "Optimized",
            _ => "cEDH",
        }
    }
}

/// Estimates a deck's bracket.
///
/// `combos` may be absent — the artifact is an optional download — in which case combo-based
/// criteria are skipped and said to have been skipped, rather than silently treated as clean.
pub fn assess(deck: &Deck, catalog: &Catalog, combos: Option<&ComboDatabase>) -> BracketAssessment {
    let cards = deck_cards(deck, catalog);

    let game_changers = collect(&cards, |card| {
        card.is_game_changer()
            .then(|| "on the official Game Changers list".to_owned())
    });
    let mass_land_denial = collect(&cards, |card| detect_mass_land_denial(card.oracle_text()));
    let extra_turns = collect(&cards, |card| detect_extra_turns(card.oracle_text()));
    let tutors = collect(&cards, |card| detect_tutor(card.oracle_text()));

    let (two_card_combos, longer_combos) = match combos {
        Some(database) => {
            let index = ComboIndex::build(database);
            let found = index.find_in(deck);
            found
                .into_iter()
                .filter(|combo| combo.is_infinite || combo.wins_the_game)
                .partition(|combo| combo.card_count <= 2)
        }
        None => (Vec::new(), Vec::new()),
    };

    let mut bracket = 2u8;
    let mut reasons = Vec::new();

    if game_changers.len() > BRACKET_3_GAME_CHANGER_LIMIT {
        bracket = bracket.max(4);
        reasons.push(format!(
            "{} Game Changers — bracket 3 allows at most {BRACKET_3_GAME_CHANGER_LIMIT}",
            game_changers.len()
        ));
    } else if !game_changers.is_empty() {
        bracket = bracket.max(3);
        reasons.push(format!(
            "{} Game Changer(s) — brackets 1 and 2 allow none",
            game_changers.len()
        ));
    }

    if !two_card_combos.is_empty() {
        bracket = bracket.max(3);
        reasons.push(format!(
            "{} two-card infinite combo(s) — not allowed below bracket 3",
            two_card_combos.len()
        ));
    }

    if !mass_land_denial.is_empty() {
        bracket = bracket.max(4);
        reasons.push(format!(
            "{} mass land denial effect(s) — not expected below bracket 4",
            mass_land_denial.len()
        ));
    }

    // One extra turn is fine at any bracket; it is chaining them that is not. Two or more
    // separate cards is the closest a card list gets to showing intent to chain.
    if extra_turns.len() > 1 {
        bracket = bracket.max(4);
        reasons.push(format!(
            "{} extra-turn cards — a single one is fine, several suggest chaining",
            extra_turns.len()
        ));
    }

    if reasons.is_empty() {
        reasons.push("nothing found that a bracket 2 deck should not have".to_owned());
    }

    let mut caveats = vec![
        "Brackets 1 and 5 depend on how a deck is played, not on what is in it, so this \
         estimate only ranges from 2 to 4."
            .to_owned(),
    ];
    if combos.is_none() {
        caveats.push(
            "The combo database is not loaded, so two-card combos were not checked — the \
             real bracket may be higher."
                .to_owned(),
        );
    }
    if tutors.len() >= 5 {
        caveats.push(format!(
            "{} tutors. The rules say tutors should be sparse below bracket 3 without saying \
             how many is too many, so this is reported rather than counted.",
            tutors.len()
        ));
    }

    BracketAssessment {
        bracket,
        reasons,
        game_changers,
        two_card_combos,
        longer_combos,
        mass_land_denial,
        extra_turns,
        tutors,
        caveats,
    }
}

/// The distinct cards a deck plays, sideboard excluded.
fn deck_cards<'a>(deck: &Deck, catalog: &'a Catalog) -> Vec<(String, &'a ArchivedCard)> {
    let wanted: HashSet<&str> = deck
        .entries
        .iter()
        .filter(|entry| entry.zone != Zone::Sideboard)
        .map(|entry| entry.oracle_id.as_str())
        .collect();

    let mut found: Vec<(String, &ArchivedCard)> = catalog
        .iter()
        .filter(|(_, card)| wanted.contains(card.oracle_id()))
        .map(|(_, card)| (card.name().to_owned(), card))
        .collect();
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn collect(
    cards: &[(String, &ArchivedCard)],
    test: impl Fn(&ArchivedCard) -> Option<String>,
) -> Vec<Marker> {
    cards
        .iter()
        .filter_map(|(name, card)| {
            test(card).map(|note| Marker {
                name: name.clone(),
                note,
            })
        })
        .collect()
}

/// Mass land denial, read off rules text.
///
/// A heuristic, and a deliberately narrow one: it looks for effects that destroy or sacrifice
/// *all* lands. Stax pieces that merely tax or slow lands — Winter Orb, Rising Waters — are
/// not mass land denial under the bracket rules and are left alone.
fn detect_mass_land_denial(text: &str) -> Option<String> {
    let lowered = text.to_lowercase();
    const PATTERNS: [&str; 5] = [
        "destroy all lands",
        "each player sacrifices all lands",
        "destroy target player's lands",
        "sacrifice all lands",
        "exile all lands",
    ];
    PATTERNS
        .iter()
        .find(|pattern| lowered.contains(*pattern))
        .map(|pattern| format!("mass land denial: \"{pattern}\""))
}

/// Extra-turn effects, read off rules text.
fn detect_extra_turns(text: &str) -> Option<String> {
    let lowered = text.to_lowercase();
    (lowered.contains("take an extra turn") || lowered.contains("takes an extra turn"))
        .then(|| "takes an extra turn".to_owned())
}

/// Tutors, read off rules text.
///
/// Reported rather than counted against the bracket: the published rules say tutors should be
/// "sparse" below bracket 3 without putting a number on it, and inventing one would be
/// presenting a guess as a rule.
fn detect_tutor(text: &str) -> Option<String> {
    let lowered = text.to_lowercase();
    (lowered.contains("search your library for a card")
        || lowered.contains("search your library for up to"))
    .then(|| "searches your library".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combo::tests::{combo, database};
    use mtg_core::{Format, Legality, Rarity};
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card, CardFace, CatalogData, Layout, LEGALITY_SLOTS,
    };
    use mtg_deck::DeckEntry;

    fn card(name: &str, text: &str, game_changer: bool) -> Card {
        let mut built = Card {
            oracle_id: format!("o-{name}"),
            name: name.to_owned(),
            mana_cost: String::new(),
            mana_value: 0.0,
            colors: 0,
            color_identity: 0,
            produced_mana: 0,
            tags: 0,
            type_line: "Instant".to_owned(),
            oracle_text: text.to_owned(),
            power: None,
            toughness: None,
            loyalty: None,
            keywords: Vec::new(),
            legalities: [legality_to_u8(Legality::Legal); LEGALITY_SLOTS],
            rarity: rarity_to_u8(Rarity::Common),
            edhrec_rank: None,
            game_changer,
            reserved: false,
            layout: Layout::Normal,
            faces: Vec::new(),
            set_code: "tst".to_owned(),
            collector_number: "1".to_owned(),
            released_at: "2026-01-01".to_owned(),
            image_id: String::new(),
        };
        built.faces.push(CardFace {
            name: built.name.clone(),
            mana_cost: String::new(),
            type_line: built.type_line.clone(),
            oracle_text: built.oracle_text.clone(),
            power: None,
            toughness: None,
            loyalty: None,
            colors: 0,
        });
        built
    }

    fn catalog() -> Catalog {
        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards: vec![
                card("Island", "", false),
                card("Mana Vault", "Mana Vault doesn't untap.", true),
                card(
                    "Rhystic Study",
                    "Draw a card unless that player pays {1}.",
                    true,
                ),
                card("Smothering Tithe", "Create a Treasure token.", true),
                card("Jeska's Will", "Add {R} for each card.", true),
                card("Armageddon", "Destroy all lands.", false),
                card("Time Warp", "Take an extra turn after this one.", false),
                card(
                    "Temporal Manipulation",
                    "Take an extra turn after this one.",
                    false,
                ),
                card("Demonic Tutor", "Search your library for a card.", false),
                card("Thassa's Oracle", "You win the game.", false),
                card("Demonic Consultation", "Exile cards.", false),
            ],
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    fn deck_of(cards: &[&str]) -> Deck {
        let mut deck = Deck::new("Test", Format::Commander);
        for name in cards {
            deck.add(DeckEntry::new(format!("o-{name}"), *name, 1));
        }
        deck
    }

    fn combos() -> ComboDatabase {
        database(vec![combo(
            "a",
            &["Thassa's Oracle", "Demonic Consultation"],
            &["Win the game"],
        )])
    }

    #[test]
    fn a_plain_deck_sits_at_bracket_two() {
        let result = assess(&deck_of(&["Island"]), &catalog(), Some(&combos()));
        assert_eq!(result.bracket, 2);
        assert_eq!(result.label(), "Core");
        assert!(
            result.reasons[0].contains("nothing found"),
            "{:?}",
            result.reasons
        );
    }

    #[test]
    fn one_game_changer_lifts_it_to_three() {
        let result = assess(
            &deck_of(&["Island", "Mana Vault"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(result.bracket, 3);
        assert_eq!(result.game_changers.len(), 1);
        assert_eq!(result.game_changers[0].name, "Mana Vault");
    }

    #[test]
    fn a_fourth_game_changer_lifts_it_to_four() {
        // Three is the documented ceiling for bracket 3.
        let three = assess(
            &deck_of(&["Mana Vault", "Rhystic Study", "Smothering Tithe"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(three.bracket, 3);

        let four = assess(
            &deck_of(&[
                "Mana Vault",
                "Rhystic Study",
                "Smothering Tithe",
                "Jeska's Will",
            ]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(four.bracket, 4);
    }

    #[test]
    fn a_two_card_infinite_combo_lifts_it_to_three() {
        let result = assess(
            &deck_of(&["Island", "Thassa's Oracle", "Demonic Consultation"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(result.bracket, 3);
        assert_eq!(result.two_card_combos.len(), 1);
        assert!(
            result.reasons.iter().any(|r| r.contains("two-card")),
            "{:?}",
            result.reasons
        );
    }

    #[test]
    fn mass_land_denial_lifts_it_to_four() {
        let result = assess(
            &deck_of(&["Island", "Armageddon"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(result.bracket, 4);
        assert_eq!(result.mass_land_denial.len(), 1);
    }

    #[test]
    fn one_extra_turn_card_is_fine_but_two_are_not() {
        // The published rules single out chaining, not the effect itself.
        let one = assess(
            &deck_of(&["Island", "Time Warp"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(one.bracket, 2);
        assert_eq!(one.extra_turns.len(), 1, "still reported");

        let two = assess(
            &deck_of(&["Time Warp", "Temporal Manipulation"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(two.bracket, 4);
    }

    #[test]
    fn tutors_are_reported_without_moving_the_bracket() {
        // "Sparse" is not a number, and inventing one would present a guess as a rule.
        let result = assess(
            &deck_of(&["Island", "Demonic Tutor"]),
            &catalog(),
            Some(&combos()),
        );
        assert_eq!(result.bracket, 2);
        assert_eq!(result.tutors.len(), 1);
    }

    #[test]
    fn the_estimate_never_claims_bracket_one_or_five() {
        // Those depend on how the deck is played. Every assessment says so.
        for deck in [
            deck_of(&["Island"]),
            deck_of(&[
                "Mana Vault",
                "Rhystic Study",
                "Smothering Tithe",
                "Jeska's Will",
                "Armageddon",
            ]),
        ] {
            let result = assess(&deck, &catalog(), Some(&combos()));
            assert!((2..=4).contains(&result.bracket), "{}", result.bracket);
            assert!(
                result.caveats.iter().any(|c| c.contains("1 and 5")),
                "{:?}",
                result.caveats
            );
        }
    }

    #[test]
    fn a_missing_combo_database_is_admitted_not_assumed_clean() {
        // Otherwise a deck with a two-card combo would be reported as bracket 2 with no hint
        // that the check never ran.
        let result = assess(
            &deck_of(&["Thassa's Oracle", "Demonic Consultation"]),
            &catalog(),
            None,
        );
        assert_eq!(result.bracket, 2);
        assert!(
            result.caveats.iter().any(|c| c.contains("not loaded")),
            "{:?}",
            result.caveats
        );
    }

    #[test]
    fn mass_land_denial_does_not_catch_stax_pieces() {
        // Winter Orb slows lands; it is not mass land denial under the bracket rules.
        assert!(detect_mass_land_denial("Lands don't untap during untap steps.").is_none());
        assert!(detect_mass_land_denial("Destroy all lands.").is_some());
        assert!(detect_mass_land_denial("Destroy target land.").is_none());
    }

    #[test]
    fn the_reasons_read_as_sentences() {
        // They go straight into the UI.
        let result = assess(
            &deck_of(&[
                "Mana Vault",
                "Rhystic Study",
                "Smothering Tithe",
                "Jeska's Will",
            ]),
            &catalog(),
            Some(&combos()),
        );
        assert!(
            result.reasons[0].starts_with("4 Game Changers"),
            "{:?}",
            result.reasons
        );
    }
}
