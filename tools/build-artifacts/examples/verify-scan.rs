//! End-to-end check of the scanner against real card images.
//!
//! Every test in `mtg-vision` uses synthetic cards, which proves the maths but not that any of
//! it recognises a Magic card. This takes real reference hashes out of the artwork build, pulls
//! the real images down again, distorts them the way a camera would — perspective, blur,
//! sensor noise, a dark table around them — and asks the scanner to name them.
//!
//! A development check, not a test: it needs the network and a partially built
//! `.cache/arthashes.jsonl`, neither of which belongs in `cargo test`.
//!
//! ```bash
//! cargo run --release -p build-artifacts --example verify-scan
//! ```

use std::collections::HashSet;
use std::io::BufRead;

use anyhow::{Context, Result};
use mtg_vision::{
    archive, homography_from_quads, rgba_to_gray, ArtDatabase, ArtEntry, ArtHash, GrayImage, Quad,
    Scanner,
};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Done {
    printing_id: String,
    oracle_id: String,
    name: String,
    hash: String,
}

#[derive(Debug, Deserialize)]
struct ApiCard {
    image_uris: Option<ApiImages>,
    #[serde(default)]
    card_faces: Vec<ApiFace>,
}

#[derive(Debug, Deserialize)]
struct ApiFace {
    image_uris: Option<ApiImages>,
}

#[derive(Debug, Deserialize)]
struct ApiImages {
    normal: Option<String>,
}

/// How the card is placed in the frame. Each one is a plausible way to hold a card.
struct Pose {
    label: &'static str,
    quad: Quad,
    blur: bool,
    noise: u8,
    /// Base brightness of the table the card sits on.
    table: u8,
}

/// A sweep across table brightness, which turned out to be the variable that matters.
fn poses() -> Vec<Pose> {
    [18u8, 50, 80, 110, 140, 170, 210]
        .into_iter()
        .flat_map(|table| {
            [
                Pose {
                    label: "square on",
                    quad: Quad::new([
                        (160.0, 150.0),
                        (480.0, 150.0),
                        (480.0, 597.0),
                        (160.0, 597.0),
                    ]),
                    blur: false,
                    noise: 8,
                    table,
                },
                Pose {
                    label: "tilted, blurred",
                    quad: Quad::new([
                        (150.0, 175.0),
                        (475.0, 145.0),
                        (500.0, 580.0),
                        (175.0, 615.0),
                    ]),
                    blur: true,
                    noise: 18,
                    table,
                },
            ]
        })
        .collect()
}

fn main() -> Result<()> {
    // The resume file is what a partial build leaves behind and is what makes this runnable
    // before the hours-long download finishes. Once `arthashes.bin` exists it is preferred:
    // that is the file the app actually reads, so verifying against it exercises the shipping
    // path — including the archive reader — rather than a lookalike built here.
    let records = load_records(".cache/arthashes.jsonl")?;
    let archive_path = std::path::Path::new("artifacts/arthashes.bin");

    let database = if archive_path.exists() {
        let file = std::fs::File::open(archive_path)?;
        let started = std::time::Instant::now();
        let database = archive::read(&mut std::io::BufReader::with_capacity(1 << 16, file))
            .context("reading artifacts/arthashes.bin")?;
        println!(
            "Database: {} artworks from arthashes.bin, opened in {:.0} ms",
            database.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        database
    } else if !records.is_empty() {
        let database = ArtDatabase::new(
            records
                .iter()
                .filter_map(|record| {
                    Some(ArtEntry {
                        hash: ArtHash::from_hex(&record.hash)?,
                        printing_id: record.printing_id.clone(),
                        oracle_id: record.oracle_id.clone(),
                        name: record.name.clone(),
                    })
                })
                .collect(),
        );
        println!(
            "Database: {} artworks from a partial build (no arthashes.bin yet)",
            database.len()
        );
        database
    } else {
        anyhow::bail!("no hashes anywhere — run `--art-only` first, even briefly");
    };

    // Spread across the file rather than the first few, which are all basic lands from one set.
    let step = (records.len() / 12).max(1);
    let sample: Vec<&Done> = records.iter().step_by(step).take(10).collect();
    println!(
        "Sample:   {} cards x {} poses
",
        sample.len(),
        poses().len()
    );

    let mut scanner = Scanner::new(database);
    // (background brightness, pose) -> [named, wrong, seen but unmatched, not detected]
    let mut tally: std::collections::BTreeMap<(u8, &str), [usize; 4]> = Default::default();

    for record in sample {
        let Some(url) = image_url(&record.printing_id)? else {
            continue;
        };
        let card = fetch_gray(&url)?;
        print!(".");
        let _ = std::io::Write::flush(&mut std::io::stdout());

        for pose in poses() {
            let frame = photograph(&card, &pose);
            let counts = tally.entry((pose.table, pose.label)).or_default();

            match scanner.recognise_still(&frame.pixels, frame.width, frame.height) {
                Some(found) if found.oracle_id == record.oracle_id => counts[0] += 1,
                Some(_) => counts[1] += 1,
                // Distinguishing these two matters: "seen but unmatched" means the outline
                // snapped to the wrong edge, which is a framing problem, while "not detected"
                // means the card never separated from the background at all.
                None if scanner.last_quad().is_some() => counts[2] += 1,
                None => counts[3] += 1,
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(120));
    }

    println!(
        "

 table  pose               named  wrong  seen-no-match  not-detected"
    );
    let mut total = [0usize; 4];
    for ((table, label), counts) in &tally {
        println!(
            "  {table:>4}  {label:<18} {:>5}  {:>5}  {:>13}  {:>12}",
            counts[0], counts[1], counts[2], counts[3]
        );
        for (slot, count) in counts.iter().enumerate() {
            total[slot] += count;
        }
    }

    let attempts: usize = total.iter().sum();
    println!(
        "
{}/{attempts} named, {} wrong, {} declined",
        total[0],
        total[1],
        total[2] + total[3]
    );
    println!();
    println!("A declined frame is a non-event: the scanner refuses rather than guessing, and");
    println!("the voter waits for a better frame. A non-zero `wrong` column is the one that");
    println!("matters — naming the wrong card is what the thresholds exist to prevent.");
    Ok(())
}

fn load_records(path: &str) -> Result<Vec<Done>> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {path}"))?;
    let mut records = Vec::new();
    let mut seen = HashSet::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line?;
        if let Ok(record) = serde_json::from_str::<Done>(&line) {
            // One entry per card: several printings of the same artwork would only test the
            // same hash repeatedly.
            if seen.insert(record.oracle_id.clone()) {
                records.push(record);
            }
        }
    }
    Ok(records)
}

/// Asks Scryfall where a printing's image lives.
fn image_url(printing_id: &str) -> Result<Option<String>> {
    // The `#0`/`#1` suffix marks a face of a double-faced card; the API wants the bare id.
    let (id, face) = match printing_id.split_once('#') {
        Some((id, index)) => (id, index.parse::<usize>().unwrap_or(0)),
        None => (printing_id, 0),
    };

    let body = build_artifacts_get(&format!("https://api.scryfall.com/cards/{id}"))?;
    let card: ApiCard = serde_json::from_str(&body)?;
    Ok(match card.image_uris.and_then(|uris| uris.normal) {
        Some(url) => Some(url),
        None => card
            .card_faces
            .into_iter()
            .nth(face)
            .and_then(|face| face.image_uris)
            .and_then(|uris| uris.normal),
    })
}

fn build_artifacts_get(url: &str) -> Result<String> {
    const AGENT: &str = concat!(
        "MagicOptimizer/",
        env!("CARGO_PKG_VERSION"),
        " (+https://github.com/eraflo/MagicOptimizer)"
    );
    Ok(ureq::get(url)
        .header("User-Agent", AGENT)
        .header("Accept", "application/json")
        .call()
        .with_context(|| format!("requesting {url}"))?
        .body_mut()
        .read_to_string()?)
}

fn fetch_gray(url: &str) -> Result<GrayImage> {
    const AGENT: &str = concat!(
        "MagicOptimizer/",
        env!("CARGO_PKG_VERSION"),
        " (+https://github.com/eraflo/MagicOptimizer)"
    );
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(
        &mut ureq::get(url)
            .header("User-Agent", AGENT)
            .call()
            .with_context(|| format!("downloading {url}"))?
            .into_body()
            .into_reader(),
        &mut bytes,
    )?;

    let decoded = image::load_from_memory(&bytes)?.to_rgba8();
    let (width, height) = decoded.dimensions();
    let mut pixels = Vec::new();
    rgba_to_gray(decoded.as_raw(), &mut pixels);
    Ok(GrayImage {
        pixels,
        width,
        height,
    })
}

/// Turns a clean card scan into something a camera might have produced.
fn photograph(card: &GrayImage, pose: &Pose) -> GrayImage {
    let width = 640;
    let height = 900;
    let mut frame = GrayImage::new(width, height);
    // A dark table, unevenly lit — a flat background would be an easier problem than reality.
    for y in 0..height {
        for x in 0..width {
            let shade = pose
                .table
                .saturating_add((x / 40) as u8)
                .saturating_add((y / 90) as u8);
            frame.pixels[(y * width + x) as usize] = shade;
        }
    }

    let source = Quad::new([
        (0.0, 0.0),
        (card.width as f32 - 1.0, 0.0),
        (card.width as f32 - 1.0, card.height as f32 - 1.0),
        (0.0, card.height as f32 - 1.0),
    ]);
    let Some(homography) = homography_from_quads(&pose.quad, &source) else {
        return frame;
    };

    let view = card.view();
    for y in 0..height {
        for x in 0..width {
            let (sx, sy) = homography.apply((x as f32 + 0.5, y as f32 + 0.5));
            if sx >= 0.0 && sy >= 0.0 && sx < card.width as f32 && sy < card.height as f32 {
                frame.pixels[(y * width + x) as usize] = view.at(sx as u32, sy as u32);
            }
        }
    }

    if pose.blur {
        frame = blur(&frame);
    }
    if pose.noise > 0 {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let spread = u64::from(pose.noise);
        for pixel in frame.pixels.iter_mut() {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            let jitter = ((state >> 40) % (spread * 2 + 1)) as i32 - spread as i32;
            *pixel = (i32::from(*pixel) + jitter).clamp(0, 255) as u8;
        }
    }
    frame
}

/// A 3x3 box blur — soft focus, which is what a hand-held phone produces.
fn blur(image: &GrayImage) -> GrayImage {
    let mut out = GrayImage::new(image.width, image.height);
    let view = image.view();
    for y in 0..image.height {
        for x in 0..image.width {
            let mut total = 0u32;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let sx = (x as i32 + dx).clamp(0, image.width as i32 - 1) as u32;
                    let sy = (y as i32 + dy).clamp(0, image.height as i32 - 1) as u32;
                    total += u32::from(view.at(sx, sy));
                }
            }
            out.pixels[(y * image.width + x) as usize] = (total / 9) as u8;
        }
    }
    out
}
