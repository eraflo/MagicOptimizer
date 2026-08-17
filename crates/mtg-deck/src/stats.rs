//! Descriptive numbers about a deck.
//!
//! Description only — nothing here judges a deck. Deciding that a curve is *wrong* is the
//! optimizer's job in phase 4; this just counts what is there so the editor can draw it.

use mtg_core::{Color, ColorSet};
use mtg_data::{ArchivedCard, Catalog};
use serde::{Deserialize, Serialize};

use crate::deck::{Deck, Zone};

/// Mana values above this share the top bucket. Individual counts past it are noise on a
/// chart, and every deckbuilding site collapses them the same way.
const CURVE_CAP: u32 = 7;

/// One column of the mana curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurveBucket {
    pub mana_value: u32,
    pub count: u32,
    /// True for the top bucket, which holds everything at or above it.
    pub is_overflow: bool,
}

/// How many symbols of each colour the deck's costs ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPips {
    /// WUBRG letter.
    pub color: char,
    /// Total coloured symbols across every copy — the input Karsten's land formulas want,
    /// which is why it counts pips rather than cards.
    pub pips: u32,
    /// Cards asking for this colour at all.
    pub cards: u32,
}

/// Counted properties of a deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeckStats {
    /// Main deck plus command zone. The sideboard is excluded: it is not what you draw from.
    pub total_cards: u32,
    pub lands: u32,
    pub creatures: u32,
    /// Buckets 0 through [`CURVE_CAP`], always all present so a chart has no gaps.
    pub curve: Vec<CurveBucket>,
    /// Mean mana value of the non-land cards. Lands would drag it towards zero and say nothing.
    pub average_mana_value: f32,
    pub color_pips: Vec<ColorPips>,
    /// Combined colour identity, as WUBRG letters.
    pub color_identity: String,
    /// Cards the catalog could not resolve, so the numbers can be labelled incomplete rather
    /// than quietly wrong.
    pub unresolved_cards: u32,
}

/// Counts a deck.
pub fn stats(deck: &Deck, catalog: &Catalog) -> DeckStats {
    let mut curve = vec![0u32; (CURVE_CAP + 1) as usize];
    let mut total_cards = 0;
    let mut lands = 0;
    let mut creatures = 0;
    let mut unresolved_cards = 0;
    let mut non_land_cards = 0;
    let mut mana_value_total = 0f64;
    let mut identity = ColorSet::COLORLESS;
    let mut pips = [0u32; 5];
    let mut cards_per_color = [0u32; 5];

    for entry in deck.entries.iter().filter(|e| e.zone != Zone::Sideboard) {
        let Some(card) = find_by_oracle_id(catalog, &entry.oracle_id) else {
            unresolved_cards += entry.quantity;
            continue;
        };

        let quantity = entry.quantity;
        total_cards += quantity;
        identity = identity.union(card.color_identity());

        let is_land = card.has_type("Land");
        if is_land {
            lands += quantity;
        }
        if card.has_type("Creature") {
            creatures += quantity;
        }

        // Lands are excluded from the curve as well as from the average: a 40-land Commander
        // deck would otherwise show a huge zero-cost column that means nothing.
        if !is_land {
            let mana_value = card.mana_value().max(0.0).round() as u32;
            let bucket = mana_value.min(CURVE_CAP) as usize;
            curve[bucket] += quantity;
            non_land_cards += quantity;
            mana_value_total += f64::from(card.mana_value()) * f64::from(quantity);
        }

        if let Ok(cost) = card.mana_cost() {
            for (index, color) in Color::ALL.into_iter().enumerate() {
                let count = cost.pip_count(color);
                if count > 0 {
                    pips[index] += count * quantity;
                    cards_per_color[index] += quantity;
                }
            }
        }
    }

    DeckStats {
        total_cards,
        lands,
        creatures,
        curve: curve
            .into_iter()
            .enumerate()
            .map(|(mana_value, count)| CurveBucket {
                mana_value: mana_value as u32,
                count,
                is_overflow: mana_value as u32 == CURVE_CAP,
            })
            .collect(),
        average_mana_value: if non_land_cards == 0 {
            0.0
        } else {
            (mana_value_total / f64::from(non_land_cards)) as f32
        },
        color_pips: Color::ALL
            .into_iter()
            .enumerate()
            .map(|(index, color)| ColorPips {
                color: color.symbol(),
                pips: pips[index],
                cards: cards_per_color[index],
            })
            .collect(),
        color_identity: identity.to_string(),
        unresolved_cards,
    }
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
    use mtg_core::{Format, Legality, Rarity};
    use mtg_data::{
        legality_to_u8, rarity_to_u8, Card, CardFace, CatalogData, Layout, LEGALITY_SLOTS,
    };

    fn card(name: &str, type_line: &str, mana_cost: &str, mana_value: f32) -> Card {
        let mut built = Card {
            oracle_id: format!("o-{name}"),
            name: name.to_owned(),
            mana_cost: mana_cost.to_owned(),
            mana_value,
            colors: 0,
            color_identity: mtg_core::ManaCost::parse(mana_cost)
                .map(|c| c.colors().bits())
                .unwrap_or(0),
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
                card("Island", "Basic Land — Island", "", 0.0),
                card("Delver", "Creature — Human Wizard", "{U}", 1.0),
                card("Counterspell", "Instant", "{U}{U}", 2.0),
                card("Cryptic Command", "Instant", "{1}{U}{U}{U}", 4.0),
                card("Emrakul", "Legendary Creature — Eldrazi", "{15}", 15.0),
                card("Lightning Bolt", "Instant", "{R}", 1.0),
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
    fn counts_lands_and_creatures() {
        let stats = stats(
            &deck_of(&[("Island", 20), ("Delver", 4), ("Counterspell", 4)]),
            &catalog(),
        );
        assert_eq!(stats.total_cards, 28);
        assert_eq!(stats.lands, 20);
        assert_eq!(stats.creatures, 4);
    }

    #[test]
    fn lands_stay_out_of_the_curve() {
        // Otherwise a land-heavy deck shows a giant zero-cost column that says nothing.
        let stats = stats(&deck_of(&[("Island", 20), ("Delver", 4)]), &catalog());
        assert_eq!(stats.curve[0].count, 0);
        assert_eq!(stats.curve[1].count, 4);
    }

    #[test]
    fn the_top_bucket_collects_everything_above_it() {
        let stats = stats(&deck_of(&[("Emrakul", 2)]), &catalog());
        let top = stats.curve.last().copied().unwrap();
        assert_eq!(top.mana_value, CURVE_CAP);
        assert_eq!(top.count, 2, "a 15-drop lands in the 7+ bucket");
        assert!(top.is_overflow);
    }

    #[test]
    fn the_curve_has_no_gaps() {
        // A chart should not have to guess which buckets exist.
        let stats = stats(&deck_of(&[("Delver", 1)]), &catalog());
        assert_eq!(stats.curve.len() as u32, CURVE_CAP + 1);
        for (index, bucket) in stats.curve.iter().enumerate() {
            assert_eq!(bucket.mana_value, index as u32);
        }
    }

    #[test]
    fn the_average_excludes_lands() {
        // Two one-drops and two two-drops average 1.5, however many lands are alongside.
        let stats = stats(
            &deck_of(&[("Island", 30), ("Delver", 2), ("Counterspell", 2)]),
            &catalog(),
        );
        assert!(
            (stats.average_mana_value - 1.5).abs() < 0.001,
            "{:?}",
            stats.average_mana_value
        );
    }

    #[test]
    fn an_all_land_deck_does_not_divide_by_zero() {
        let stats = stats(&deck_of(&[("Island", 60)]), &catalog());
        assert_eq!(stats.average_mana_value, 0.0);
    }

    #[test]
    fn pips_are_counted_per_symbol_not_per_card() {
        // The distinction Karsten's land formulas depend on: four Counterspells ask for eight
        // blue symbols, not four.
        let stats = stats(&deck_of(&[("Counterspell", 4)]), &catalog());
        let blue = stats.color_pips.iter().find(|p| p.color == 'U').unwrap();
        assert_eq!(blue.pips, 8);
        assert_eq!(blue.cards, 4);
    }

    #[test]
    fn every_colour_appears_even_at_zero() {
        let stats = stats(&deck_of(&[("Lightning Bolt", 4)]), &catalog());
        assert_eq!(stats.color_pips.len(), 5);
        let white = stats.color_pips.iter().find(|p| p.color == 'W').unwrap();
        assert_eq!(white.pips, 0);
    }

    #[test]
    fn the_sideboard_is_not_counted() {
        let mut deck = deck_of(&[("Island", 20)]);
        deck.add(DeckEntry::new("o-Counterspell", "Counterspell", 15).in_zone(Zone::Sideboard));
        assert_eq!(stats(&deck, &catalog()).total_cards, 20);
    }

    #[test]
    fn the_command_zone_is_counted() {
        let mut deck = deck_of(&[("Island", 99)]);
        deck.add(DeckEntry::new("o-Emrakul", "Emrakul", 1).in_zone(Zone::Command));
        assert_eq!(stats(&deck, &catalog()).total_cards, 100);
    }

    #[test]
    fn unresolved_cards_are_counted_rather_than_ignored() {
        // So the UI can label the numbers incomplete instead of showing wrong ones.
        let mut deck = deck_of(&[("Island", 20)]);
        deck.add(DeckEntry::new("o-ghost", "Ghost", 4));

        let stats = stats(&deck, &catalog());
        assert_eq!(stats.unresolved_cards, 4);
        assert_eq!(stats.total_cards, 20);
    }
}
