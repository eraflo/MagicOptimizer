//! Checks the functional tags in the real catalog.
//!
//! Tags are the first thing to give the optimizer any notion of what a card is *for*, so a
//! wrong one is not a cosmetic problem — it would push deck advice in a direction nothing else
//! can see. This asks two questions of the finished `cards.rkyv`:
//!
//! 1. **Coverage.** How much of the catalog carries any role at all? The tagger is crowdsourced,
//!    so this is a measurement rather than a guarantee.
//! 2. **Correctness on cards we can check by hand.** A short list of cards whose role nobody
//!    would argue about, and one — Grizzly Bears — that must come back with *nothing*, because
//!    an empty set has to keep meaning "no role" rather than "the data is missing".
//!
//! ```bash
//! cargo run --release -p build-artifacts --example verify-tags
//! ```

use anyhow::{Context, Result};
use mtg_core::Tag;
use mtg_data::Catalog;

/// Cards whose role is not a matter of opinion, and what must be true of each.
const EXPECTED: &[(&str, &[Tag], &[Tag])] = &[
    ("Lightning Bolt", &[Tag::Removal], &[Tag::Ramp, Tag::Draw]),
    ("Counterspell", &[Tag::Counterspell], &[Tag::Removal]),
    ("Sol Ring", &[Tag::Ramp], &[Tag::Removal]),
    ("Rhystic Study", &[Tag::CardAdvantage], &[Tag::Removal]),
    ("Wrath of God", &[Tag::BoardWipe], &[Tag::Ramp]),
    ("Demonic Tutor", &[Tag::Tutor], &[Tag::Ramp]),
    ("Ponder", &[Tag::Cantrip], &[Tag::Removal]),
    ("Cultivate", &[Tag::Ramp], &[Tag::Removal]),
    ("Time Warp", &[Tag::ExtraTurn], &[Tag::Removal]),
    ("Llanowar Elves", &[Tag::ManaDork], &[Tag::Removal]),
];

fn main() -> Result<()> {
    let catalog = Catalog::open("artifacts/cards.rkyv").context("opening artifacts/cards.rkyv")?;
    println!("Catalog: {} cards\n", catalog.len());

    // --- Coverage -----------------------------------------------------------------------
    let mut tagged = 0usize;
    let mut nonland = 0usize;
    let mut nonland_tagged = 0usize;
    let mut per_tag = [0usize; Tag::ALL.len()];

    for (_, card) in catalog.iter() {
        let tags = card.tags();
        let is_land = card.has_type("Land");
        if !is_land {
            nonland += 1;
        }
        if !tags.is_empty() {
            tagged += 1;
            if !is_land {
                nonland_tagged += 1;
            }
        }
        for tag in tags.iter() {
            per_tag[tag as usize] += 1;
        }
    }

    println!(
        "Coverage: {tagged}/{} cards ({:.0}%), {nonland_tagged}/{nonland} non-land ({:.0}%)\n",
        catalog.len(),
        tagged as f64 / catalog.len().max(1) as f64 * 100.0,
        nonland_tagged as f64 / nonland.max(1) as f64 * 100.0
    );

    let mut counts: Vec<(Tag, usize)> = Tag::ALL
        .into_iter()
        .map(|tag| (tag, per_tag[tag as usize]))
        .collect();
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    for (tag, count) in &counts {
        println!("  {:<18} {count:>6}", tag.scryfall_tag());
    }

    // A tag that landed on nothing means its name drifted and a whole role is missing.
    let empty: Vec<&str> = counts
        .iter()
        .filter(|(_, count)| *count == 0)
        .map(|(tag, _)| tag.scryfall_tag())
        .collect();
    if !empty.is_empty() {
        println!(
            "\n  EMPTY, so their names have drifted: {}",
            empty.join(", ")
        );
    }

    // --- Cards we can check by hand ----------------------------------------------------
    println!("\n── spot checks");
    let mut wrong = 0usize;
    for (name, must_have, must_not_have) in EXPECTED {
        let Some((_, card)) = catalog
            .iter()
            .find(|(_, card)| card.name().eq_ignore_ascii_case(name))
        else {
            println!("  {name:<18} not in the catalog");
            wrong += 1;
            continue;
        };

        let tags = card.tags();
        let missing: Vec<&str> = must_have
            .iter()
            .filter(|tag| !tags.contains(**tag))
            .map(|tag| tag.scryfall_tag())
            .collect();
        let unexpected: Vec<&str> = must_not_have
            .iter()
            .filter(|tag| tags.contains(**tag))
            .map(|tag| tag.scryfall_tag())
            .collect();

        let roles: Vec<&str> = tags.iter().map(|tag| tag.scryfall_tag()).collect();
        if missing.is_empty() && unexpected.is_empty() {
            println!("  {name:<18} ok    [{}]", roles.join(", "));
        } else {
            wrong += 1;
            println!(
                "  {name:<18} WRONG missing {:?}, unexpected {:?}  [{}]",
                missing,
                unexpected,
                roles.join(", ")
            );
        }
    }

    // The other half of the contract: an empty set must keep meaning "no role".
    match catalog
        .iter()
        .find(|(_, card)| card.name().eq_ignore_ascii_case("Grizzly Bears"))
    {
        Some((_, bears)) if bears.tags().is_empty() => {
            println!(
                "  {:<18} ok    [no role, as a vanilla 2/2 should have]",
                "Grizzly Bears"
            )
        }
        Some((_, bears)) => {
            wrong += 1;
            let roles: Vec<&str> = bears.tags().iter().map(|tag| tag.scryfall_tag()).collect();
            println!(
                "  {:<18} WRONG a vanilla 2/2 has [{}]",
                "Grizzly Bears",
                roles.join(", ")
            );
        }
        None => println!("  {:<18} not in the catalog", "Grizzly Bears"),
    }

    println!("\n{wrong} spot check(s) failed");
    Ok(())
}
