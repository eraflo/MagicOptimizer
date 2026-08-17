//! Building `arthashes.bin` — the fingerprints the camera scanner matches against.
//!
//! # What gets hashed, and why it matters
//!
//! Scryfall offers an `art_crop` image, which looks like the obvious thing to hash. It is not.
//! At scan time the app has a photo of a whole card, straightens it, and cuts the artwork out
//! using fixed fractions of the card. If the reference hash came from Scryfall's crop and the
//! query hash from ours, the two would be framed differently — and a perceptual hash compares
//! *layout*, so a few percent of shift moves it far more than camera noise ever does.
//!
//! So the reference comes from the `normal` image, which is a whole card at 488×680, and goes
//! through exactly the same [`crop_artwork`] and [`hash_gray`] the scanner uses. That the
//! rectifier's output is also 488×680 is not a coincidence.
//!
//! # Nothing is redistributed
//!
//! The images are downloaded, hashed and thrown away. What ships is a 32-byte fingerprint per
//! artwork, from which no image can be reconstructed. That is a deliberate design property —
//! see the Legal section of `CLAUDE.md`.

use std::collections::HashSet;
use std::io::{BufRead, BufWriter, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mtg_vision::{archive, crop_artwork, hash_gray, rgba_to_gray, ArtEntry, ArtHash, GrayView};
use serde::Deserialize;

use crate::scryfall;

/// Pause between image requests.
///
/// Scryfall asks for 50–100 ms between calls. The images come from their CDN rather than the
/// API, but this is a tool that makes tens of thousands of requests against a service that owes
/// us nothing, so it takes the slower end.
const DELAY: Duration = Duration::from_millis(100);

/// How often progress is printed. This run takes over an hour; silence would be alarming.
const PROGRESS_EVERY: usize = 250;

/// One entry of the `unique_artwork` bulk file, cut down to what is needed.
#[derive(Debug, Deserialize)]
struct ArtworkCard {
    id: String,
    /// Missing on the handful of entries that are not real cards, which are skipped.
    oracle_id: Option<String>,
    name: String,
    layout: String,
    image_uris: Option<ImageUris>,
    #[serde(default)]
    card_faces: Vec<Face>,
}

#[derive(Debug, Deserialize)]
struct Face {
    name: Option<String>,
    image_uris: Option<ImageUris>,
}

#[derive(Debug, Deserialize)]
struct ImageUris {
    /// A whole card at 488×680 — the same shape the rectifier produces.
    normal: Option<String>,
}

/// Layouts with no card to photograph.
fn is_not_a_card(layout: &str) -> bool {
    matches!(
        layout,
        "token" | "double_faced_token" | "emblem" | "art_series" | "vanguard" | "scheme" | "planar"
    )
}

/// One image to fetch: a card, or one face of a double-faced card.
struct Target {
    printing_id: String,
    oracle_id: String,
    name: String,
    url: String,
}

fn targets_of(card: ArtworkCard) -> Vec<Target> {
    if is_not_a_card(&card.layout) {
        return Vec::new();
    }
    let Some(oracle_id) = card.oracle_id else {
        return Vec::new();
    };

    // A single-faced card carries its images at the top level; a transforming one carries them
    // per face. Both faces are hashed against the same oracle id, so scanning the back of a
    // werewolf finds the card rather than nothing.
    if let Some(url) = card.image_uris.and_then(|uris| uris.normal) {
        return vec![Target {
            printing_id: card.id,
            oracle_id,
            name: card.name,
            url,
        }];
    }

    card.card_faces
        .into_iter()
        .enumerate()
        .filter_map(|(index, face)| {
            let url = face.image_uris?.normal?;
            Some(Target {
                // Suffixed so the two faces of one printing do not collide in the resume file.
                printing_id: format!("{}#{index}", card.id),
                oracle_id: oracle_id.clone(),
                name: face.name.unwrap_or_else(|| card.name.clone()),
                url,
            })
        })
        .collect()
}

/// A line of the resume file: what has already been hashed.
#[derive(Debug, Deserialize, serde::Serialize)]
struct Done {
    printing_id: String,
    oracle_id: String,
    name: String,
    hash: String,
}

/// Downloads every artwork, hashes it, and writes `arthashes.bin`.
///
/// Resumable, and it has to be: this makes tens of thousands of requests over more than an
/// hour, and a run that lost everything to one dropped connection would be unusable. Each hash
/// is appended to a resume file as it is computed, so a second run picks up where the first
/// stopped.
pub fn build(out: &Path, cache: &Path, limit: Option<usize>, started: Instant) -> Result<()> {
    std::fs::create_dir_all(cache).with_context(|| format!("creating {}", cache.display()))?;
    let resume_path = cache.join("arthashes.jsonl");

    let mut done = load_resume(&resume_path)?;
    let resumed = done.len();
    if resumed > 0 {
        println!("  resuming: {resumed} artworks already hashed");
    }

    println!("Fetching the Scryfall bulk index...");
    let entries = scryfall::fetch_bulk_index()?;
    let entry = scryfall::find_entry(&entries, "unique_artwork")?;
    let bulk_path = cache.join(scryfall::cache_file_name(&entry));
    scryfall::download_cached(&entry.jsonl_download_uri, &bulk_path)?;

    let reader = std::io::BufReader::with_capacity(1 << 20, scryfall::open_jsonl(&bulk_path)?);
    let mut targets = Vec::new();
    for line in reader.lines() {
        let line = line.context("reading the unique-artwork bulk")?;
        if line.trim().is_empty() {
            continue;
        }
        // One malformed entry must not lose the rest, exactly as in the catalog build.
        let Ok(card) = serde_json::from_str::<ArtworkCard>(&line) else {
            continue;
        };
        targets.extend(targets_of(card));
    }

    let already: HashSet<&str> = done
        .iter()
        .map(|entry| entry.printing_id.as_str())
        .collect();
    let total_artworks = targets.len();
    let mut todo: Vec<Target> = targets
        .into_iter()
        .filter(|target| !already.contains(target.printing_id.as_str()))
        .collect();

    println!(
        "  {total_artworks} artworks in total, {} still to download \
         (about {:.0} MB, roughly {:.0} minutes at {} ms apart)",
        todo.len(),
        todo.len() as f64 * 0.1,
        todo.len() as f64 * DELAY.as_secs_f64() / 60.0,
        DELAY.as_millis()
    );

    if let Some(limit) = limit {
        todo.truncate(limit);
    }

    let mut resume_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&resume_path)
        .with_context(|| format!("opening {}", resume_path.display()))?;

    let mut downloaded = 0usize;
    let mut failed = 0usize;
    let total = todo.len();

    for (index, target) in todo.into_iter().enumerate() {
        std::thread::sleep(DELAY);

        match hash_one(&target.url) {
            Ok(hash) => {
                let record = Done {
                    printing_id: target.printing_id,
                    oracle_id: target.oracle_id,
                    name: target.name,
                    hash: hash.to_hex(),
                };
                // Written before anything else so an interrupted run loses at most one image.
                writeln!(resume_file, "{}", serde_json::to_string(&record)?)?;
                done.push(record);
                downloaded += 1;
            }
            Err(error) => {
                // A single missing image is not worth losing an hour of work over. They are
                // counted and reported, and a later run retries them.
                if failed < 10 {
                    eprintln!("  warning: {} — {error}", target.name);
                }
                failed += 1;
            }
        }

        if (index + 1) % PROGRESS_EVERY == 0 {
            let elapsed = started.elapsed().as_secs_f64();
            let rate = (index + 1) as f64 / elapsed.max(0.001);
            println!(
                "  {}/{} ({:.0}%), about {:.0} minutes left",
                index + 1,
                total,
                (index + 1) as f64 / total as f64 * 100.0,
                (total - index - 1) as f64 / rate / 60.0
            );
        }
    }

    if failed > 0 {
        eprintln!("  {failed} image(s) could not be fetched; run again to retry them");
    }

    let entries: Vec<ArtEntry> = done
        .iter()
        .filter_map(|record| {
            Some(ArtEntry {
                hash: ArtHash::from_hex(&record.hash)?,
                printing_id: record.printing_id.clone(),
                oracle_id: record.oracle_id.clone(),
                name: record.name.clone(),
            })
        })
        .collect();

    let path = out.join("arthashes.bin");
    let file =
        std::fs::File::create(&path).with_context(|| format!("writing {}", path.display()))?;
    let mut writer = BufWriter::new(file);
    archive::write(&mut writer, &entries).context("serializing the artwork archive")?;
    writer.flush()?;

    println!(
        "\nWrote {} ({:.1} MB, {} artworks: {downloaded} new, {resumed} from a previous run) \
         in {:.1}s",
        path.display(),
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) as f64 / 1e6,
        entries.len(),
        started.elapsed().as_secs_f64()
    );

    Ok(())
}

/// Reads the resume file, ignoring lines a previous run left half-written.
fn load_resume(path: &Path) -> Result<Vec<Done>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut records = Vec::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.context("reading the resume file")?;
        if line.trim().is_empty() {
            continue;
        }
        // A run killed mid-write leaves a partial last line. Dropping it silently is right:
        // that image simply gets fetched again.
        if let Ok(record) = serde_json::from_str::<Done>(&line) {
            records.push(record);
        }
    }
    Ok(records)
}

/// Downloads one card image and hashes its artwork the way the scanner does.
fn hash_one(url: &str) -> Result<ArtHash> {
    let bytes = scryfall::download_bytes(url)?;

    let decoded = image::load_from_memory(&bytes)
        .with_context(|| format!("decoding the image from {url}"))?
        .to_rgba8();
    let (width, height) = decoded.dimensions();

    // Greyscale through `rgba_to_gray` rather than the image crate's own conversion. The two
    // use different luma weights, and a reference hash computed with different weights from the
    // query hash would be subtly, unfixably wrong.
    let mut gray = Vec::new();
    rgba_to_gray(decoded.as_raw(), &mut gray);

    let view = GrayView::new(&gray, width, height)
        .with_context(|| format!("the image from {url} is not the size it claims"))?;
    let art = crop_artwork(&view);
    Ok(hash_gray(&art.view()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Vec<Target> {
        targets_of(serde_json::from_str(json).expect("parse"))
    }

    #[test]
    fn a_normal_card_yields_one_target() {
        let targets = parse(
            r#"{"id":"p1","oracle_id":"o1","name":"Sol Ring","layout":"normal",
                "image_uris":{"normal":"https://cards.scryfall.io/normal/x.jpg"}}"#,
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].oracle_id, "o1");
        assert_eq!(targets[0].name, "Sol Ring");
    }

    #[test]
    fn both_faces_of_a_transforming_card_are_hashed() {
        // Scanning the back of a werewolf has to find the card, not nothing.
        let targets = parse(
            r#"{"id":"p2","oracle_id":"o2","name":"Delver // Insect","layout":"transform",
                "card_faces":[
                  {"name":"Delver of Secrets","image_uris":{"normal":"https://a/front.jpg"}},
                  {"name":"Insectile Aberration","image_uris":{"normal":"https://a/back.jpg"}}
                ]}"#,
        );
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|target| target.oracle_id == "o2"));
        assert_eq!(targets[0].name, "Delver of Secrets");
        assert_eq!(targets[1].name, "Insectile Aberration");
    }

    #[test]
    fn the_two_faces_get_distinct_ids() {
        // They share a printing id in Scryfall's data; colliding here would make the resume
        // file skip the back face forever.
        let targets = parse(
            r#"{"id":"p2","oracle_id":"o2","name":"A // B","layout":"transform",
                "card_faces":[
                  {"name":"A","image_uris":{"normal":"https://a/1.jpg"}},
                  {"name":"B","image_uris":{"normal":"https://a/2.jpg"}}
                ]}"#,
        );
        assert_ne!(targets[0].printing_id, targets[1].printing_id);
    }

    #[test]
    fn a_split_card_yields_one_target() {
        // Split and adventure cards have faces but a single physical image.
        let targets = parse(
            r#"{"id":"p3","oracle_id":"o3","name":"Fire // Ice","layout":"split",
                "image_uris":{"normal":"https://a/fireice.jpg"},
                "card_faces":[{"name":"Fire"},{"name":"Ice"}]}"#,
        );
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "Fire // Ice");
    }

    #[test]
    fn tokens_and_emblems_are_skipped() {
        // There is no such physical card to point a camera at.
        for layout in ["token", "double_faced_token", "emblem", "art_series"] {
            let json = format!(
                r#"{{"id":"p","oracle_id":"o","name":"T","layout":"{layout}",
                    "image_uris":{{"normal":"https://a/t.jpg"}}}}"#
            );
            assert!(parse(&json).is_empty(), "{layout} was not skipped");
        }
    }

    #[test]
    fn an_entry_without_an_oracle_id_is_skipped() {
        // Collections are keyed on oracle id; an entry without one could never be used.
        let targets = parse(
            r#"{"id":"p4","name":"Mystery","layout":"normal",
                "image_uris":{"normal":"https://a/m.jpg"}}"#,
        );
        assert!(targets.is_empty());
    }

    #[test]
    fn an_entry_with_no_image_is_skipped_rather_than_failing() {
        // Some placeholder printings genuinely have none.
        let targets = parse(r#"{"id":"p5","oracle_id":"o5","name":"No Art","layout":"normal"}"#);
        assert!(targets.is_empty());
    }

    #[test]
    fn unknown_fields_do_not_break_parsing() {
        // Scryfall adds fields regularly; the build must not fall over when it does.
        let targets = parse(
            r#"{"id":"p6","oracle_id":"o6","name":"Sol Ring","layout":"normal",
                "some_new_field":{"nested":true},"prices":{"usd":"1.00"},
                "image_uris":{"normal":"https://a/s.jpg","png":"https://a/s.png"}}"#,
        );
        assert_eq!(targets.len(), 1);
    }
}
