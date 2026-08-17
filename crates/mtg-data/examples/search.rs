//! Searches a catalog artifact from the command line.
//!
//! This is the phase 1 end-to-end check: build an artifact with `build-artifacts`, then read it
//! back and query it. It also prints timings, so the cost of opening a memory-mapped catalog
//! and of a full linear scan are both visible rather than assumed.
//!
//! ```text
//! cargo run --release -p mtg-data --example search -- --text "draw a card" --type Instant
//! ```

use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use mtg_core::{ColorSet, Format};
use mtg_data::{Catalog, Query};

#[derive(Parser, Debug)]
#[command(about = "Query a MagicOptimizer card catalog artifact")]
struct Args {
    /// Path to the artifact produced by build-artifacts.
    #[arg(long, default_value = "artifacts/cards.rkyv")]
    catalog: PathBuf,

    /// Exact card name. Prints full details for that one card.
    #[arg(long)]
    name: Option<String>,

    /// Substring of the name or rules text.
    #[arg(long)]
    text: Option<String>,

    /// Required word on the type line. Repeatable.
    #[arg(long = "type")]
    card_types: Vec<String>,

    /// Restrict to a commander's color identity, e.g. WU.
    #[arg(long)]
    identity: Option<String>,

    /// Restrict to cards playable in a format, e.g. commander.
    #[arg(long)]
    format: Option<String>,

    /// Only cards on the official Commander Game Changers list.
    #[arg(long)]
    game_changers: bool,

    #[arg(long, default_value_t = 15)]
    limit: usize,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let opening = Instant::now();
    let catalog = Catalog::open(&args.catalog)?;
    let open_time = opening.elapsed();

    println!(
        "Opened {} — {} cards, Scryfall data from {} (took {:.1} ms)\n",
        args.catalog.display(),
        catalog.len(),
        catalog.source_updated_at(),
        open_time.as_secs_f64() * 1000.0
    );

    if let Some(name) = &args.name {
        return match catalog.find_by_name(name) {
            Some((id, card)) => {
                print_details(id, card);
                Ok(())
            }
            None => Err(format!("no card named {name:?}").into()),
        };
    }

    let mut query = Query::new().limit(args.limit);
    if let Some(text) = &args.text {
        query = query.text(text);
    }
    for kind in &args.card_types {
        query = query.card_type(kind);
    }
    if let Some(identity) = &args.identity {
        query = query.identity_within(ColorSet::from_symbols(identity));
    }
    if let Some(format) = &args.format {
        let format = Format::from_scryfall_key(&format.to_lowercase())
            .ok_or_else(|| format!("unknown format {format:?}"))?;
        query = query.legal_in(format);
    }
    if args.game_changers {
        query = query.game_changer(true);
    }

    let searching = Instant::now();
    let total = query.count(&catalog);
    let results = query.execute(&catalog);
    let search_time = searching.elapsed();

    for (id, card) in &results {
        println!(
            "  {id:>8}  {:<44} {:<10} {}",
            truncate(card.name(), 44),
            card.mana_cost_display(),
            truncate(card.type_line(), 40)
        );
    }

    println!(
        "\n{} of {} matches, two full scans in {:.1} ms",
        results.len(),
        total,
        search_time.as_secs_f64() * 1000.0
    );
    Ok(())
}

fn print_details(id: mtg_core::CardId, card: &mtg_data::ArchivedCard) {
    println!("{}  {}", card.name(), id);
    println!("  type       {}", card.type_line());
    println!(
        "  cost       {} (mana value {})",
        card.mana_cost_display(),
        card.mana_value()
    );
    println!(
        "  colors     {} / identity {}",
        card.colors(),
        card.color_identity()
    );
    println!(
        "  layout     {:?} ({} face(s))",
        card.layout,
        card.faces().len()
    );
    if card.is_game_changer() {
        println!("  flags      Commander Game Changer");
    }
    if let Some(rank) = card.edhrec_rank() {
        println!("  edhrec     #{rank}");
    }

    let legal: Vec<&str> = Format::ALL
        .iter()
        .filter(|f| card.is_legal_in(**f))
        .map(|f| f.display_name())
        .collect();
    println!("  legal in   {}", legal.join(", "));

    if card.is_multi_faced() {
        for face in card.faces() {
            println!("\n  -- {} {}", face.name(), face.mana_cost_display());
            println!("     {}", face.type_line());
            for line in face.oracle_text().lines() {
                println!("     {line}");
            }
        }
    } else {
        println!();
        for line in card.oracle_text().lines() {
            println!("  {line}");
        }
    }
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_owned();
    }
    text.chars().take(width - 1).collect::<String>() + "…"
}
