//! Builds the binary artifacts the application ships with.
//!
//! This is a **development tool**. It runs on a PC, downloads hundreds of megabytes, and is
//! never bundled into the app or built for Android — which is what makes its native TLS
//! dependency acceptable when the rest of the workspace forbids one. See
//! `docs/dev/data-pipeline.md`.

mod artwork;
mod oracle;
mod scryfall;
mod spellbook;
mod tags;

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

    /// Also fetch the combo snapshot from Commander Spellbook into combos.rkyv.
    ///
    /// Off by default: it is several hundred paginated requests against a community project's
    /// donated infrastructure, and the card catalog does not depend on it.
    #[arg(long)]
    combos: bool,

    /// Only fetch combos, skipping the card catalog.
    #[arg(long)]
    combos_only: bool,

    /// Also build arthashes.bin, the fingerprints the camera scanner matches against.
    ///
    /// Off by default because it is tens of thousands of image downloads and over an hour of
    /// wall clock. It is resumable — run it again and it picks up where it stopped.
    #[arg(long)]
    art: bool,

    /// Only build arthashes.bin, skipping the card catalog.
    #[arg(long)]
    art_only: bool,

    /// Stop after this many artwork downloads. For checking the plumbing, not for publishing.
    #[arg(long)]
    art_limit: Option<usize>,

    /// Skip the functional tags, leaving every card's roles empty.
    ///
    /// They are part of the catalog rather than a separate artifact, so they are fetched by
    /// default; this is the escape hatch for a quick rebuild or an offline one.
    #[arg(long)]
    no_tags: bool,

    /// Build everything: the card catalog, the combos and the artwork hashes.
    ///
    /// Budget a couple of hours, almost all of it the artwork downloads.
    #[arg(long)]
    all: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let started = Instant::now();

    std::fs::create_dir_all(&args.out)
        .with_context(|| format!("creating {}", args.out.display()))?;

    if args.combos_only {
        return build_combos(&args.out, started);
    }
    if args.art_only {
        artwork::build(&args.out, &args.cache, args.art_limit, started)?;
        return Ok(());
    }

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

    if !args.no_tags && args.from_file.is_none() {
        println!("\nFetching functional tags from Scryfall's tagger...");
        apply_tags(&mut cards)?;
    }

    let data = CatalogData {
        format_version: FORMAT_VERSION,
        source_updated_at,
        cards,
    };

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

    if args.combos || args.all {
        println!();
        build_combos(&args.out, Instant::now())?;
    }
    if args.art || args.all {
        println!();
        artwork::build(&args.out, &args.cache, args.art_limit, Instant::now())?;
    }
    Ok(())
}

/// Fetches the functional tags and writes them onto the cards.
///
/// A failure here is reported and swallowed: tags make the optimizer better, and a card catalog
/// without them is the catalog this project shipped for its first five phases. Losing a whole
/// build to a community endpoint being down would be the wrong trade.
fn apply_tags(cards: &mut [mtg_data::Card]) -> Result<()> {
    let (tags, report) = match tags::fetch_tags() {
        Ok(fetched) => fetched,
        Err(error) => {
            eprintln!("  WARNING: could not fetch tags: {error}");
            eprintln!("  The catalog is still valid; cards will simply carry no roles.");
            return Ok(());
        }
    };

    let mut applied = 0usize;
    for card in cards.iter_mut() {
        if let Some(bits) = tags.get(&card.oracle_id) {
            card.tags = *bits;
            applied += 1;
        }
    }

    println!(
        "\n  {} requests, {} cards tagged, {applied} of them in this catalog ({:.0}%)",
        report.requests,
        report.cards_tagged,
        applied as f64 / cards.len().max(1) as f64 * 100.0
    );

    // Loud, like the legality-key warning. A drifted tag name is a whole role disappearing from
    // every deck's analysis, and nothing downstream would ever notice on its own.
    if !report.empty_tags.is_empty() {
        let list: Vec<&str> = report.empty_tags.iter().copied().collect();
        eprintln!();
        eprintln!("  WARNING: these tags matched no cards at all:");
        eprintln!("    {}", list.join(", "));
        eprintln!("  Their names have probably changed. Check them against");
        eprintln!("  https://tagger.scryfall.com and update `mtg_core::Tag`.");
    }
    Ok(())
}

/// Fetches the combo snapshot and writes `combos.rkyv`.
fn build_combos(out: &std::path::Path, started: Instant) -> Result<()> {
    println!("Fetching combos from Commander Spellbook...");
    let (combos, report) = spellbook::fetch_combos()?;

    println!(
        "  {} variants seen, {} kept",
        report.variants_seen, report.kept
    );
    if report.skipped_not_ok > 0 {
        println!(
            "  {} variants skipped as not valid combos",
            report.skipped_not_ok
        );
    }
    if report.skipped_without_oracle_id > 0 {
        println!(
            "  {} variants skipped for having no oracle id",
            report.skipped_without_oracle_id
        );
    }
    // Loud, like the legality-key warning: an unexpected status means the endpoint's contract
    // moved, and the artifact may be quietly missing combos.
    if !report.unexpected_statuses.is_empty() {
        let list: Vec<&str> = report
            .unexpected_statuses
            .iter()
            .map(String::as_str)
            .collect();
        eprintln!();
        eprintln!("  WARNING: Commander Spellbook returned unexpected variant statuses:");
        eprintln!("    {}", list.join(", "));
        eprintln!("  Those variants were skipped. Check whether the endpoint's contract changed.");
    }

    let data = mtg_combo::ComboData {
        format_version: mtg_combo::FORMAT_VERSION,
        // When Spellbook generated the snapshot, not when we happened to download it — that is
        // what tells the user how old their combo data really is. Only falls back to the local
        // clock if the dump stopped saying.
        fetched_at: report.snapshot_taken_at.clone().unwrap_or_else(today),
        combos,
    };
    let bytes = mtg_combo::serialize(&data).context("serializing the combo database")?;
    let path = out.join("combos.rkyv");
    std::fs::write(&path, &bytes).with_context(|| format!("writing {}", path.display()))?;

    println!(
        "\nWrote {} ({:.1} MB, {} combos) in {:.1}s",
        path.display(),
        bytes.len() as f64 / 1e6,
        data.combos.len(),
        started.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Today's date, for stamping the snapshot.
///
/// Computed from the system clock rather than pulling in a date library: the artifact only
/// needs to say roughly how old it is, and that is not worth a dependency. This is the standard
/// civil-from-days conversion.
fn today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let z = (seconds / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;

    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);

    format!("{year:04}-{month:02}-{day:02}")
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
