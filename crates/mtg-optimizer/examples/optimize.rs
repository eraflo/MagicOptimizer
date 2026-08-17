//! Scores a decklist and looks for improvements, from the command line.
//!
//! The phase 4 end-to-end check: read a real list, score it against real card data, and see
//! what the search proposes.
//!
//! ```text
//! cargo run --release -p mtg-optimizer --example optimize -- --file mydeck.txt --format commander
//! ```

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use mtg_core::Format;
use mtg_data::Catalog;
use mtg_optimizer::{score, search, Archetype, CardIndex, CardPool, SearchSettings};

#[derive(Parser, Debug)]
#[command(about = "Score a decklist and suggest improvements")]
struct Args {
    #[arg(long, default_value = "artifacts/cards.rkyv")]
    catalog: PathBuf,

    /// Decklist to read. Reads standard input when absent.
    #[arg(long)]
    file: Option<PathBuf>,

    #[arg(long, default_value = "commander")]
    format: String,

    /// aggro, midrange or control.
    #[arg(long, default_value = "midrange")]
    archetype: String,

    /// Skip the search and only report the score.
    #[arg(long)]
    score_only: bool,

    #[arg(long, default_value_t = 1_200)]
    iterations: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let format = Format::from_scryfall_key(&args.format.to_lowercase())
        .ok_or_else(|| format!("unknown format {:?}", args.format))?;
    let archetype = match args.archetype.to_lowercase().as_str() {
        "aggro" => Archetype::Aggro,
        "midrange" => Archetype::Midrange,
        "control" => Archetype::Control,
        other => return Err(format!("unknown archetype {other:?}").into()),
    };

    let text = match &args.file {
        Some(path) => std::fs::read_to_string(path)?,
        None => std::io::read_to_string(std::io::stdin())?,
    };

    let catalog = Catalog::open(&args.catalog)?;
    let imported = mtg_deck::import(&text, "Imported", format, &catalog);
    for problem in &imported.problems {
        eprintln!("  {problem}");
    }

    let index = CardIndex::build(&catalog);
    let profile = mtg_optimizer::profile_with_index(&imported.deck, &index);

    let mut settings = SearchSettings::for_deck_size(profile.deck_size());
    settings.score.archetype = archetype;
    settings.iterations = args.iterations;
    settings.pool = CardPool::Everything;

    let started = Instant::now();
    let current = score(&profile, settings.score);
    println!(
        "{} — {:.1}/100{}",
        imported.deck.name,
        current.total,
        if current.reliable {
            String::new()
        } else {
            format!(
                "  (unreliable: {} cards not in the card data)",
                current.unresolved_cards
            )
        }
    );
    for criterion in &current.criteria {
        println!(
            "  {:<14} {:>5.1}%  {}{}",
            criterion.name,
            criterion.score * 100.0,
            criterion.detail,
            if criterion.derived {
                ""
            } else {
                "  [convention]"
            }
        );
    }

    let simulation = &current.simulation;
    println!(
        "\n  over {} simulated games: {:.0}% keepable openers, {:.2} mulligans, {:.0}% make every land drop to turn 4",
        simulation.games,
        simulation.keepable_opening_hands * 100.0,
        simulation.average_mulligans,
        simulation.land_drops_made.get(3).copied().unwrap_or(0.0) * 100.0
    );
    println!(
        "  scored in {:.0} ms",
        started.elapsed().as_secs_f64() * 1000.0
    );

    if args.score_only {
        return Ok(());
    }

    println!("\nSearching...");
    let started = Instant::now();
    let result = search(&imported.deck, &index, &settings);
    println!(
        "  {} candidates, {:.1} s\n",
        result.candidates_considered,
        started.elapsed().as_secs_f64()
    );

    if result.suggestions.is_empty() {
        println!("No improvement found.");
        return Ok(());
    }

    println!(
        "{:.1} -> {:.1} with {} change(s):\n",
        result.before.total,
        result.after.total,
        result.suggestions.len()
    );
    for suggestion in &result.suggestions {
        println!(
            "  -1 {:<32} +1 {:<32} {:+.2}",
            suggestion.remove_name,
            suggestion.add_name,
            suggestion.gain()
        );
        for reason in &suggestion.reasons {
            println!("       {reason}");
        }
    }

    Ok(())
}
