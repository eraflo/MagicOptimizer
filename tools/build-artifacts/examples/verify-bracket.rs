//! Checks the Commander bracket estimate against the real catalog and combo snapshot.
//!
//! The unit tests use a handful of fixture combos. This runs the same code over the 105,328
//! real ones, on decks built out of cards that actually exist, because a detector that works on
//! three fixtures and drowns in a hundred thousand entries would look identical in CI.
//!
//! ```bash
//! cargo run --release -p build-artifacts --example verify-bracket
//! ```

use anyhow::{Context, Result};
use mtg_combo::{assess, ComboDatabase, ComboIndex};
use mtg_core::Format;
use mtg_data::Catalog;
use mtg_deck::{Deck, DeckEntry};

fn main() -> Result<()> {
    let catalog = Catalog::open("artifacts/cards.rkyv").context("opening artifacts/cards.rkyv")?;
    let combos =
        ComboDatabase::open("artifacts/combos.rkyv").context("opening artifacts/combos.rkyv")?;
    println!(
        "Catalog: {} cards.  Combos: {} variants.\n",
        catalog.len(),
        combos.len()
    );

    // Look cards up by name so the deck is made of real oracle ids rather than invented ones.
    let by_name = |name: &str| -> Option<(String, String)> {
        catalog
            .iter()
            .find(|(_, card)| card.name().eq_ignore_ascii_case(name))
            .map(|(_, card)| (card.oracle_id().to_owned(), card.name().to_owned()))
    };

    let decks: Vec<(&str, Vec<&str>)> = vec![
        (
            "a two-card win plus tutors",
            vec![
                "Thassa's Oracle",
                "Demonic Consultation",
                "Demonic Tutor",
                "Vampiric Tutor",
                "Rhystic Study",
                "Mystical Tutor",
            ],
        ),
        (
            "Game Changers, no combo",
            vec![
                "Rhystic Study",
                "Smothering Tithe",
                "Cyclonic Rift",
                "Sol Ring",
            ],
        ),
        (
            "mass land denial and extra turns",
            vec!["Armageddon", "Time Warp", "Sol Ring", "Llanowar Elves"],
        ),
        (
            "a plain precon-ish list",
            vec!["Llanowar Elves", "Cultivate", "Forest", "Giant Growth"],
        ),
    ];

    let index = ComboIndex::build(&combos);

    for (label, names) in decks {
        let mut deck = Deck::new(label, Format::Commander);
        let mut missing = Vec::new();
        for name in &names {
            match by_name(name) {
                Some((oracle_id, real)) => deck.add(DeckEntry::new(oracle_id, real, 1)),
                None => missing.push(*name),
            }
        }

        let found = index.find_in(&deck);
        let verdict = assess(&deck, &catalog, Some(&combos));

        println!("── {label}");
        if !missing.is_empty() {
            println!("   not in the catalog: {}", missing.join(", "));
        }
        println!("   bracket {}", verdict.bracket);
        for reason in &verdict.reasons {
            println!("     · {reason}");
        }
        println!(
            "   game changers {}, two-card combos {}, longer {}, land denial {}, extra turns {}, tutors {}",
            verdict.game_changers.len(),
            verdict.two_card_combos.len(),
            verdict.longer_combos.len(),
            verdict.mass_land_denial.len(),
            verdict.extra_turns.len(),
            verdict.tutors.len()
        );
        if let Some(combo) = found.first() {
            println!(
                "   e.g. {} -> {}",
                combo.card_names.join(" + "),
                combo.produces.join(", ")
            );
        }
        for caveat in &verdict.caveats {
            println!("   caveat: {caveat}");
        }
        println!();
    }

    // The check that matters for the artifact being optional: the same decks with no combo data
    // must not silently read as clean.
    let mut deck = Deck::new("two-card win, no combo data", Format::Commander);
    for name in ["Thassa's Oracle", "Demonic Consultation"] {
        if let Some((oracle_id, real)) = by_name(name) {
            deck.add(DeckEntry::new(oracle_id, real, 1));
        }
    }
    let blind = assess(&deck, &catalog, None);
    println!("── the same deck with the combo artifact absent");
    println!("   bracket {}", blind.bracket);
    for caveat in &blind.caveats {
        println!("   caveat: {caveat}");
    }
    Ok(())
}
