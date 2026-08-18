//! Reducing a deck to the numbers scoring and simulation actually need.
//!
//! Built once per evaluation. The optimizer scores a deck thousands of times, and re-reading
//! the catalog for every card on every pass would dominate the run time — this walks the deck
//! once and hands back flat arrays.

use mtg_core::{Color, ColorSet};
use mtg_data::{ArchivedCard, Catalog};
use mtg_deck::{Deck, Zone};
use serde::{Deserialize, Serialize};

use crate::simulate::Card;

/// A coloured requirement the deck has to be able to meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipRequirement {
    pub color: Color,
    /// Symbols of this colour in one card's cost.
    pub pips: u32,
    /// The turn it wants to be cast on, which is its mana value.
    pub turn: u32,
    /// Copies in the deck, so a four-of weighs four times a singleton.
    pub copies: u32,
}

/// What a deck looks like once flattened.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckProfile {
    /// One entry per physical card, for the simulator to shuffle.
    pub cards: Vec<Card>,
    /// Sources of each colour, indexed in WUBRG order.
    pub color_sources: [u32; 5],
    /// Cards producing mana of any kind, including colourless lands.
    pub mana_producers: u32,
    pub lands: u32,
    pub creatures: u32,
    /// Copies carrying each functional role, indexed by [`mtg_core::Tag`] discriminant.
    ///
    /// Counted per copy, not per distinct card: four Lightning Bolts are four pieces of
    /// removal, which is the number that matters to a deck.
    pub roles: [u32; mtg_core::Tag::ALL.len()],
    /// Non-land copies carrying at least one role.
    ///
    /// The denominator for anything reading `roles`. It is **not** the non-land count: the
    /// tagger's coverage is 72% of the catalog, so a deck can hold cards whose role is simply
    /// unknown. Scoring those as "no role" would invent a weakness the deck may not have.
    pub with_roles: u32,
    /// Non-land copies the catalog knows but the tagger does not.
    pub without_roles: u32,
    pub pip_requirements: Vec<PipRequirement>,
    pub color_identity: ColorSet,
    /// Cards not found in the catalog. Reported so a score can be labelled unreliable rather
    /// than quietly computed from a partial deck.
    pub unresolved: u32,
}

/// Written out rather than derived: `Default` stops at 32-element arrays, and the role counts
/// are one per tag in the vocabulary.
impl Default for DeckProfile {
    fn default() -> DeckProfile {
        DeckProfile {
            cards: Vec::new(),
            color_sources: [0; 5],
            mana_producers: 0,
            lands: 0,
            creatures: 0,
            roles: [0; mtg_core::Tag::ALL.len()],
            with_roles: 0,
            without_roles: 0,
            pip_requirements: Vec::new(),
            color_identity: ColorSet::COLORLESS,
            unresolved: 0,
        }
    }
}

impl DeckProfile {
    pub fn deck_size(&self) -> u32 {
        self.cards.len() as u32
    }

    /// Copies carrying any of these roles, counting a card once however many it matches.
    ///
    /// Counting once matters: Lightning Bolt is both `removal` and `spot-removal`, and adding
    /// those would make one card look like two pieces of interaction.
    pub fn copies_with_any(&self, tags: &[mtg_core::Tag]) -> u32 {
        // Without per-card sets this cannot be exact, so it takes the largest single role
        // rather than the sum. For groups built from a parent tag and its children — which is
        // how the vocabulary is shaped — the parent's count is the right answer.
        tags.iter()
            .map(|tag| self.roles[*tag as usize])
            .max()
            .unwrap_or(0)
    }

    pub fn sources_of(&self, color: Color) -> u32 {
        self.color_sources[color_index(color)]
    }

    /// Colours the deck actually asks for, which is not the same as its identity: a commander
    /// widens identity without any card in the deck needing that colour.
    pub fn required_colors(&self) -> ColorSet {
        ColorSet::from_colors(self.pip_requirements.iter().map(|r| r.color))
    }
}

fn color_index(color: Color) -> usize {
    match color {
        Color::White => 0,
        Color::Blue => 1,
        Color::Black => 2,
        Color::Red => 3,
        Color::Green => 4,
    }
}

/// Oracle id to card, built once and reused.
///
/// The search scores a deck thousands of times. Walking all 35,000 catalog entries to build
/// this map on every one of those would dominate the run time by orders of magnitude, so it is
/// built once and handed in.
pub struct CardIndex<'a> {
    by_oracle: std::collections::HashMap<&'a str, &'a ArchivedCard>,
}

impl<'a> CardIndex<'a> {
    pub fn build(catalog: &'a Catalog) -> CardIndex<'a> {
        CardIndex {
            by_oracle: catalog
                .iter()
                .map(|(_, card)| (card.oracle_id(), card))
                .collect(),
        }
    }

    pub fn get(&self, oracle_id: &str) -> Option<&'a ArchivedCard> {
        self.by_oracle.get(oracle_id).copied()
    }

    /// Every oracle id in the catalog, in no particular order.
    pub fn oracle_ids(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.by_oracle.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.by_oracle.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_oracle.is_empty()
    }
}

/// Flattens a deck against a catalog. Convenience for one-off calls; the search builds the
/// index itself and uses [`profile_with_index`].
///
/// The sideboard is excluded throughout: it is not part of what you shuffle up.
pub fn profile(deck: &Deck, catalog: &Catalog) -> DeckProfile {
    profile_with_index(deck, &CardIndex::build(catalog))
}

/// Flattens a deck against a prebuilt index.
pub fn profile_with_index(deck: &Deck, index: &CardIndex<'_>) -> DeckProfile {
    let mut built = DeckProfile::default();

    for entry in deck.entries.iter().filter(|e| e.zone != Zone::Sideboard) {
        let Some(card) = index.get(&entry.oracle_id) else {
            built.unresolved += entry.quantity;
            continue;
        };

        let is_land = card.has_type("Land");
        let mana_value = card.mana_value().max(0.0).round() as u32;

        for _ in 0..entry.quantity {
            built.cards.push(Card {
                is_land,
                mana_value: if is_land { 0 } else { mana_value },
            });
        }

        if is_land {
            built.lands += entry.quantity;
        }
        if card.has_type("Creature") {
            built.creatures += entry.quantity;
        }

        // Lands are left out: they have roles of their own, but every criterion reading this
        // is about what the deck's *spells* do, and a land ramp package would otherwise count
        // twice — once here and once in the mana base.
        if !is_land {
            let roles = card.tags();
            if roles.is_empty() {
                built.without_roles += entry.quantity;
            } else {
                built.with_roles += entry.quantity;
                for tag in roles.iter() {
                    built.roles[tag as usize] += entry.quantity;
                }
            }
        }

        built.color_identity = built.color_identity.union(card.color_identity());

        if card.produces_mana() {
            built.mana_producers += entry.quantity;
            for color in card.produced_mana().iter() {
                built.color_sources[color_index(color)] += entry.quantity;
            }
        }

        // Requirements come from the cost, and only from spells: a land's own colour
        // identity asks nothing of the mana base.
        if !is_land {
            if let Ok(cost) = card.mana_cost() {
                for color in Color::ALL {
                    let pips = cost.pip_count(color);
                    if pips > 0 {
                        built.pip_requirements.push(PipRequirement {
                            color,
                            pips,
                            // A spell wants to be cast on the turn its cost comes online.
                            // Clamped low so a fifteen-drop does not claim a mana base is
                            // fine simply because turn fifteen sees most of the deck.
                            turn: mana_value.clamp(1, 6),
                            copies: entry.quantity,
                        });
                    }
                }
            }
        }
    }

    built
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtg_core::{Format, Legality, Rarity};
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card as CatalogCard, CardFace, CatalogData, Layout,
        LEGALITY_SLOTS,
    };
    use mtg_deck::DeckEntry;

    fn card(name: &str, type_line: &str, mana_cost: &str, produced: &str) -> CatalogCard {
        let cost = mtg_core::ManaCost::parse(mana_cost).unwrap_or_default();
        let mut built = CatalogCard {
            oracle_id: format!("o-{name}"),
            name: name.to_owned(),
            mana_cost: mana_cost.to_owned(),
            mana_value: cost.mana_value() as f32,
            colors: cost.colors().bits(),
            color_identity: cost.colors().union(ColorSet::from_symbols(produced)).bits(),
            produced_mana: ColorSet::from_symbols(produced).bits(),
            type_line: type_line.to_owned(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            keywords: Vec::new(),
            legalities: [legality_to_u8(Legality::Legal); LEGALITY_SLOTS],
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
        };
        built.faces.push(CardFace {
            name: built.name.clone(),
            mana_cost: built.mana_cost.clone(),
            type_line: built.type_line.clone(),
            oracle_text: String::new(),
            power: None,
            toughness: None,
            loyalty: None,
            colors: built.colors,
        });
        built
    }

    fn catalog() -> Catalog {
        let data = CatalogData {
            format_version: mtg_data::FORMAT_VERSION,
            source_updated_at: String::new(),
            cards: vec![
                card("Island", "Basic Land — Island", "", "U"),
                card("Mountain", "Basic Land — Mountain", "", "R"),
                card("Wastes", "Basic Land — Wastes", "", ""),
                card("Command Tower", "Land", "", "WUBRG"),
                card("Birds", "Creature — Bird", "{G}", "WUBRG"),
                card("Counterspell", "Instant", "{U}{U}", ""),
                card("Bolt", "Instant", "{R}", ""),
                card("Emrakul", "Legendary Creature — Eldrazi", "{15}", ""),
            ],
        };
        Catalog::from_bytes(mtg_data::serialize(&data).unwrap()).unwrap()
    }

    fn deck_of(entries: &[(&str, u32)]) -> Deck {
        let mut deck = Deck::new("Test", Format::Modern);
        for (name, quantity) in entries {
            deck.add(DeckEntry::new(format!("o-{name}"), *name, *quantity));
        }
        deck
    }

    #[test]
    fn one_card_entry_per_physical_copy() {
        let built = profile(&deck_of(&[("Island", 20), ("Bolt", 4)]), &catalog());
        assert_eq!(built.deck_size(), 24);
        assert_eq!(built.cards.iter().filter(|c| c.is_land).count(), 20);
    }

    #[test]
    fn colour_sources_come_from_produced_mana_not_from_the_type_line() {
        // The reason produced_mana was worth adding: Birds of Paradise is a creature, and a
        // five-colour source. Nothing in its type line says so.
        let built = profile(&deck_of(&[("Birds", 4)]), &catalog());
        for color in Color::ALL {
            assert_eq!(built.sources_of(color), 4, "{color:?}");
        }
        assert_eq!(built.lands, 0);
        assert_eq!(built.creatures, 4);
    }

    #[test]
    fn a_five_colour_land_counts_for_every_colour() {
        let built = profile(
            &deck_of(&[("Command Tower", 1), ("Island", 10)]),
            &catalog(),
        );
        assert_eq!(built.sources_of(Color::Blue), 11);
        assert_eq!(built.sources_of(Color::Red), 1);
    }

    #[test]
    fn a_colourless_land_still_counts_as_a_mana_producer() {
        // Wastes produces no colour, but it is very much a land drop.
        let built = profile(&deck_of(&[("Wastes", 5)]), &catalog());
        assert_eq!(built.lands, 5);
        assert_eq!(built.mana_producers, 5);
        for color in Color::ALL {
            assert_eq!(built.sources_of(color), 0);
        }
    }

    #[test]
    fn requirements_weigh_copies_and_count_pips() {
        let built = profile(&deck_of(&[("Counterspell", 4)]), &catalog());
        assert_eq!(built.pip_requirements.len(), 1);
        let requirement = built.pip_requirements[0];
        assert_eq!(requirement.color, Color::Blue);
        assert_eq!(
            requirement.pips, 2,
            "{{U}}{{U}} is two symbols, not one card"
        );
        assert_eq!(requirement.copies, 4);
        assert_eq!(requirement.turn, 2);
    }

    #[test]
    fn lands_ask_nothing_of_the_mana_base() {
        let built = profile(&deck_of(&[("Island", 20)]), &catalog());
        assert!(built.pip_requirements.is_empty());
    }

    #[test]
    fn a_huge_cost_does_not_claim_a_late_turn() {
        // Turn fifteen would see almost the whole deck and make any mana base look perfect.
        let built = profile(&deck_of(&[("Emrakul", 1)]), &catalog());
        assert!(built.pip_requirements.is_empty(), "no coloured pips at all");

        let mixed = profile(&deck_of(&[("Counterspell", 1), ("Emrakul", 1)]), &catalog());
        for requirement in &mixed.pip_requirements {
            assert!(requirement.turn <= 6, "{requirement:?}");
        }
    }

    #[test]
    fn required_colours_are_not_the_colour_identity() {
        // A deck can be inside a five-colour identity while asking for one colour.
        let built = profile(&deck_of(&[("Command Tower", 5), ("Bolt", 4)]), &catalog());
        assert_eq!(built.color_identity, ColorSet::WUBRG);
        assert_eq!(built.required_colors(), ColorSet::from_symbols("R"));
    }

    #[test]
    fn the_sideboard_is_left_out() {
        let mut deck = deck_of(&[("Island", 20)]);
        deck.add(DeckEntry::new("o-Bolt", "Bolt", 15).in_zone(Zone::Sideboard));
        assert_eq!(profile(&deck, &catalog()).deck_size(), 20);
    }

    #[test]
    fn the_command_zone_is_included() {
        let mut deck = deck_of(&[("Island", 99)]);
        deck.add(DeckEntry::new("o-Birds", "Birds", 1).in_zone(Zone::Command));
        let built = profile(&deck, &catalog());
        assert_eq!(built.deck_size(), 100);
        assert_eq!(built.creatures, 1);
    }

    #[test]
    fn unresolved_cards_are_counted_not_silently_dropped() {
        let mut deck = deck_of(&[("Island", 20)]);
        deck.add(DeckEntry::new("o-ghost", "Ghost", 4));
        let built = profile(&deck, &catalog());
        assert_eq!(built.unresolved, 4);
        assert_eq!(built.deck_size(), 20);
    }
}
