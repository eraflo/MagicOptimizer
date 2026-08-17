//! Imports a decklist and checks it, from the command line.
//!
//! This is the phase 3 end-to-end check: take a list as some other site would export it,
//! resolve every name against a real catalog, and say what is wrong with the deck.
//!
//! ```text
//! cargo run --release -p mtg-deck --example deck -- --file mydeck.txt --format commander
//! ```

use std::path::PathBuf;

use clap::Parser;
use mtg_core::Format;
use mtg_data::Catalog;
use mtg_deck::{check, export, ExportStyle, Zone};

#[derive(Parser, Debug)]
#[command(about = "Import a decklist and check it against its format")]
struct Args {
    /// Path to the artifact produced by build-artifacts.
    #[arg(long, default_value = "artifacts/cards.rkyv")]
    catalog: PathBuf,

    /// Decklist to read. Reads standard input when absent.
    #[arg(long)]
    file: Option<PathBuf>,

    /// Scryfall format key, e.g. commander.
    #[arg(long, default_value = "commander")]
    format: String,

    /// Print the deck back out in this style: plain, arena or mtgo.
    #[arg(long)]
    export: Option<String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let format = Format::from_scryfall_key(&args.format.to_lowercase())
        .ok_or_else(|| format!("unknown format {:?}", args.format))?;

    let text = match &args.file {
        Some(path) => std::fs::read_to_string(path)?,
        None => std::io::read_to_string(std::io::stdin())?,
    };

    let catalog = Catalog::open(&args.catalog)?;
    let result = mtg_deck::import(&text, "Imported", format, &catalog);

    println!(
        "Imported {} main, {} sideboard, {} command",
        result.deck.count_in(Zone::Main),
        result.deck.count_in(Zone::Sideboard),
        result.deck.count_in(Zone::Command),
    );

    if !result.problems.is_empty() {
        println!("\nCould not import {} line(s):", result.problems.len());
        for problem in &result.problems {
            println!("  {problem}");
        }
    }

    let report = check(&result.deck, &catalog);
    println!("\n{} — {}", format.display_name(), verdict(&report));
    if report.approximate_rules {
        println!("  (this format's construction rules are inferred, not confirmed)");
    }
    if !report.commander_identity.is_empty() {
        println!("  commander identity: {}", report.commander_identity);
    }
    for violation in &report.violations {
        println!("  - {violation}");
    }

    if let Some(style) = &args.export {
        let style = match style.to_lowercase().as_str() {
            "plain" => ExportStyle::Plain,
            "arena" => ExportStyle::Arena,
            "mtgo" => ExportStyle::Mtgo,
            other => return Err(format!("unknown export style {other:?}").into()),
        };
        println!("\n{}", export(&result.deck, style));
    }

    Ok(())
}

fn verdict(report: &mtg_deck::LegalityReport) -> String {
    if report.is_legal() {
        "legal".to_owned()
    } else {
        format!("{} problem(s)", report.violations.len())
    }
}
