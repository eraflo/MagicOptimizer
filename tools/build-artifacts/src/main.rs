//! Builds the binary artifacts the application ships with.
//!
//! This is a **development tool**. It runs on a PC, downloads hundreds of megabytes, and is
//! never bundled into the app or built for Android — which is what makes its native TLS
//! dependency acceptable when the rest of the workspace forbids one. See
//! `docs/dev/data-pipeline.md`.

mod oracle;
mod scryfall;

use std::io::BufRead;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use mtg_data::{CatalogData, FORMAT_VERSION};

use crate::oracle::{Conversion, ScryfallCard};

#[derive(Parser, Debug)]
#[command(
    name = "build-artifacts",
    about = "Turns public card sources into the binary artifacts MagicOptimizer ships"
)]
struct Args {
    /// Where to write the artifacts.
    #[arg(long, default_value = "artifacts")]
    out: PathBuf,

    /// Where to keep downloaded bulk files between runs.
    #[arg(long, default_value = ".cache")]
    cache: PathBuf,

    /// Build from a local `.jsonl` or `.jsonl.gz` instead of downloading.
    /// Useful offline, and for reproducing a build from a known input.
    #[arg(long)]
    from_file: Option<PathBuf>,

    /// Stop after this many cards. For quick iteration only — never publish the result.
    #[arg(long)]
    limit: Option<usize>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let started = Instant::now();

    let (input_path, source_updated_at) = match &args.from_file {
        Some(path) => {
            println!("Reading {}", path.display());
            (path.clone(), "unknown (built from a local file)".to_owned())
        }
        None => {
            println!("Fetching the Scryfall bulk index...");
            let entries = scryfall::fetch_bulk_index()?;
            let entry = scryfall::find_entry(&entries, "oracle_cards")?;

            let path = args.cache.join(scryfall::cache_file_name(&entry));
            if path.exists() {
                println!("Using cached {}", path.display());
            } else {
                let size = entry
                    .compressed_size
                    .map(|b| format!("{:.1} MB", b as f64 / 1e6))
                    .unwrap_or_else(|| "unknown size".to_owned());
                println!("Downloading oracle_cards ({size})...");
            }
            scryfall::download_cached(&entry.jsonl_download_uri, &path)?;
            (path, entry.updated_at)
        }
    };

    println!("Converting...");
    let reader = std::io::BufReader::with_capacity(1 << 20, scryfall::open_jsonl(&input_path)?);
    let mut conversion = Conversion::default();
    let mut cards = Vec::new();
    let mut failed_lines = 0usize;

    for (number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("reading line {}", number + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        // One malformed card must not lose the other 35,000. Count and move on.
        let raw: ScryfallCard = match serde_json::from_str(&line) {
            Ok(card) => card,
            Err(error) => {
                if failed_lines < 5 {
                    eprintln!(
                        "  warning: line {} could not be parsed: {error}",
                        number + 1
                    );
                }
                failed_lines += 1;
                continue;
            }
        };
        if let Some(card) = conversion.convert(raw) {
            cards.push(card);
        }
        if args.limit.is_some_and(|limit| cards.len() >= limit) {
            println!("  stopping early at --limit {}", cards.len());
            break;
        }
    }

    report(&conversion, failed_lines);

    let data = CatalogData {
        format_version: FORMAT_VERSION,
        source_updated_at,
        cards,
    };

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;
    let out_path = args.out.join("cards.rkyv");
    let bytes = mtg_data::serialize(&data).context("serializing the catalog")?;
    std::fs::write(&out_path, &bytes).with_context(|| format!("writing {}", out_path.display()))?;

    println!(
        "\nWrote {} ({:.1} MB, {} cards) in {:.1}s",
        out_path.display(),
        bytes.len() as f64 / 1e6,
        data.cards.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

fn report(conversion: &Conversion, failed_lines: usize) {
    println!(
        "  {} cards, {} tokens and emblems skipped",
        conversion.converted, conversion.skipped_non_cards
    );

    if failed_lines > 0 {
        eprintln!("  warning: {failed_lines} line(s) could not be parsed and were skipped");
    }

    if !conversion.unknown_layouts.is_empty() {
        let list: Vec<&str> = conversion
            .unknown_layouts
            .iter()
            .map(String::as_str)
            .collect();
        println!("  note: layouts stored as Other: {}", list.join(", "));
    }

    // This one is loud on purpose. An unmapped legality key means a whole format is missing
    // from the artifact, and nothing downstream would ever notice on its own.
    if !conversion.unknown_legality_keys.is_empty() {
        let list: Vec<&str> = conversion
            .unknown_legality_keys
            .iter()
            .map(String::as_str)
            .collect();
        eprintln!();
        eprintln!("  WARNING: Scryfall sent legality keys this build does not model:");
        eprintln!("    {}", list.join(", "));
        eprintln!("  Those formats are missing from the artifact. Add them to mtg_core::Format,");
        eprintln!("  update LEGALITY_SLOTS and bump FORMAT_VERSION.");
    }
}
