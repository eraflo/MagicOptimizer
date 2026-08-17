//! Fetching the combo snapshot from Commander Spellbook.
//!
//! # This is an unofficial endpoint
//!
//! Commander Spellbook is a community project with no published API contract. The shape below
//! is what it returned on 2026-08-17, and it may change without notice. Everything here is
//! written to fail loudly at build time rather than quietly produce a wrong artifact — and the
//! app treats the combo database as optional, so a broken build here costs a feature rather
//! than the whole application.
//!
//! # Why the bulk file rather than the paginated API
//!
//! The first version walked `/variants/?limit=100`, which is about three hundred requests. It
//! did not work: at 250 ms between requests it was rate-limited after roughly a hundred pages,
//! and at 600 ms with exponential backoff it still collapsed into a wall of `429`s and finally
//! a `503` around offset 30,800 — losing the entire run, because pagination accumulates in
//! memory and one failure at the end throws away everything before it.
//!
//! Spellbook publishes the whole thing as a single gzipped file, regenerated several times a
//! day. One request instead of three hundred, no rate limit to fight, and nothing to lose
//! partway through. It is also plainly the more considerate way to use donated infrastructure.

use std::collections::BTreeSet;
use std::io::Read;
use std::time::Duration;

use anyhow::{Context, Result};
use mtg_combo::Combo;
use serde::Deserialize;

use crate::scryfall::USER_AGENT;

const BULK_URL: &str = "https://json.commanderspellbook.com/variants.json.gz";

/// A ceiling on the decoded stream.
///
/// The file was 627 MB uncompressed on 2026-08-17. This is well above any plausible growth and
/// stops a decompression bomb, or a redirect somewhere unexpected, from being read without
/// limit. It applies after decompression, which is the only place a bomb would show.
const MAX_DECODED_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// How many times to retry a refused download.
const MAX_RETRIES: u32 = 4;

/// First backoff after a refusal, doubling from there.
const FIRST_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
struct Bulk {
    /// When Spellbook generated the snapshot. Better than the build's own clock: it says how
    /// old the *data* is, not when it happened to be fetched.
    #[serde(default)]
    timestamp: Option<String>,
    variants: Vec<Variant>,
}

#[derive(Debug, Deserialize)]
struct Variant {
    id: String,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    identity: Option<String>,
    #[serde(default)]
    popularity: Option<u32>,
    #[serde(rename = "bracketTag", default)]
    bracket_tag: Option<String>,
    #[serde(default)]
    uses: Vec<Uses>,
    #[serde(default)]
    produces: Vec<Produces>,
    #[serde(default)]
    legalities: Legalities,
}

/// Only the one format that is used, so thirty thousand maps of twenty string keys are not
/// built and thrown away. Serde ignores the rest.
#[derive(Debug, Default, Deserialize)]
struct Legalities {
    #[serde(default)]
    commander: bool,
}

#[derive(Debug, Deserialize)]
struct Uses {
    card: VariantCard,
}

#[derive(Debug, Deserialize)]
struct VariantCard {
    name: String,
    #[serde(rename = "oracleId", default)]
    oracle_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Produces {
    feature: Feature,
}

#[derive(Debug, Deserialize)]
struct Feature {
    name: String,
}

/// Totals and warnings from a fetch.
#[derive(Debug, Default)]
pub struct FetchReport {
    /// When Spellbook generated the snapshot, if it said.
    pub snapshot_taken_at: Option<String>,
    pub variants_seen: usize,
    pub kept: usize,
    /// Variants Spellbook itself does not consider valid.
    pub skipped_not_ok: usize,
    /// Variants using a card with no oracle id, which cannot be matched to a deck.
    pub skipped_without_oracle_id: usize,
    /// Status values we did not expect, so a contract change is visible rather than silent.
    pub unexpected_statuses: BTreeSet<String>,
}

/// Downloads and converts the whole combo snapshot.
pub fn fetch_combos() -> Result<(Vec<Combo>, FetchReport)> {
    let mut report = FetchReport::default();

    let reader = download_with_retry()?;
    // Parsed straight off the wire. The file is 627 MB uncompressed, and holding that as text
    // to parse afterwards would be gratuitous — streaming keeps only what survives conversion.
    let bulk: Bulk = serde_json::from_reader(std::io::BufReader::with_capacity(1 << 20, reader))
        .context("parsing the Commander Spellbook variant dump")?;

    report.snapshot_taken_at = bulk.timestamp;
    report.variants_seen = bulk.variants.len();

    let mut combos = Vec::new();
    for variant in bulk.variants {
        if let Some(combo) = convert(variant, &mut report) {
            combos.push(combo);
        }
    }

    report.kept = combos.len();
    Ok((combos, report))
}

/// Fetches the dump, backing off and retrying when the server pushes back.
///
/// Only a refusal is retried: a malformed response is a contract change and should fail the
/// build loudly rather than be attempted four more times and fail anyway.
fn download_with_retry() -> Result<impl Read> {
    let mut backoff = FIRST_BACKOFF;

    for attempt in 0..=MAX_RETRIES {
        match ureq::get(BULK_URL)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .call()
        {
            Ok(response) => {
                return Ok(
                    gunzip_if_needed(response.into_body().into_reader())?.take(MAX_DECODED_BYTES)
                );
            }
            Err(error) if is_worth_retrying(&error) && attempt < MAX_RETRIES => {
                eprintln!("  {error}; waiting {}s before retrying", backoff.as_secs());
                std::thread::sleep(backoff);
                backoff *= 2;
            }
            Err(error) => {
                return Err(anyhow::Error::new(error))
                    .with_context(|| format!("requesting {BULK_URL}"));
            }
        }
    }

    anyhow::bail!("gave up on {BULK_URL} after {MAX_RETRIES} retries")
}

/// Decompresses the stream, but only if it actually arrived compressed.
///
/// The file is named `.gz` *and* served with `Content-Encoding: gzip`, so whether the bytes on
/// the wire are still compressed by the time they reach here depends on the HTTP client's
/// configuration — `ureq` unwraps it by default, and an earlier version of this code assumed
/// otherwise and failed with "invalid gzip header". Sniffing the magic number is proof rather
/// than assumption, and it survives either side changing its mind.
fn gunzip_if_needed(mut reader: impl Read + 'static) -> Result<Box<dyn Read>> {
    let mut magic = [0u8; 2];
    let mut filled = 0;
    while filled < magic.len() {
        // A single `read` is allowed to return fewer bytes than asked for, and on a network
        // stream it usually does.
        match reader.read(&mut magic[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) => return Err(anyhow::Error::new(error).context("reading the dump")),
        }
    }

    let head = std::io::Cursor::new(magic[..filled].to_vec());
    let whole = head.chain(reader);

    Ok(if filled == 2 && magic == [0x1f, 0x8b] {
        Box::new(flate2::read::GzDecoder::new(whole))
    } else {
        Box::new(whole)
    })
}

/// True for the failures that pass on their own: rate limiting and server-side errors.
fn is_worth_retrying(error: &ureq::Error) -> bool {
    match error {
        ureq::Error::StatusCode(code) => *code == 429 || (500..600).contains(code),
        // A dropped connection mid-fetch is worth another go; a bad URL is not.
        ureq::Error::Io(_) | ureq::Error::Timeout(_) => true,
        _ => false,
    }
}

fn convert(variant: Variant, report: &mut FetchReport) -> Option<Combo> {
    // Spellbook marks variants that are not valid combos. Anything that is not "OK" is
    // recorded so an unexpected new value shows up in the build output.
    match variant.status.as_deref() {
        Some("OK") => {}
        Some(other) => {
            report.skipped_not_ok += 1;
            report.unexpected_statuses.insert(other.to_owned());
            return None;
        }
        None => {
            report.skipped_not_ok += 1;
            return None;
        }
    }

    let mut oracle_ids = Vec::with_capacity(variant.uses.len());
    let mut card_names = Vec::with_capacity(variant.uses.len());
    for used in &variant.uses {
        // Without an oracle id a combo piece cannot be matched against a deck, so the whole
        // combo is useless to us. Dropped rather than half-stored.
        let Some(oracle_id) = used.card.oracle_id.as_ref() else {
            report.skipped_without_oracle_id += 1;
            return None;
        };
        oracle_ids.push(oracle_id.clone());
        card_names.push(used.card.name.clone());
    }

    if oracle_ids.is_empty() {
        report.skipped_without_oracle_id += 1;
        return None;
    }

    Some(Combo {
        id: variant.id,
        oracle_ids,
        card_names,
        produces: variant
            .produces
            .into_iter()
            .map(|p| p.feature.name)
            .collect(),
        identity: variant.identity.unwrap_or_default(),
        legal_in_commander: variant.legalities.commander,
        popularity: variant.popularity,
        bracket_tag: variant.bracket_tag.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Bulk {
        serde_json::from_str(json).expect("parse")
    }

    #[test]
    fn only_transient_failures_are_retried() {
        // Rate limiting and server errors pass; a contract change must fail loudly instead of
        // being retried and then failing anyway.
        assert!(is_worth_retrying(&ureq::Error::StatusCode(429)));
        assert!(is_worth_retrying(&ureq::Error::StatusCode(503)));
        assert!(!is_worth_retrying(&ureq::Error::StatusCode(404)));
        assert!(!is_worth_retrying(&ureq::Error::StatusCode(400)));
    }

    #[test]
    fn the_dump_of_the_current_shape_parses() {
        // Trimmed from the real file fetched on 2026-08-17.
        let bulk = parse(
            r#"{
                "timestamp": "2026-08-17T15:29:35.923525+00:00",
                "version": "6.1.1",
                "variants": [{
                    "id": "513-5034--46",
                    "status": "OK",
                    "identity": "U",
                    "popularity": 1234,
                    "bracketTag": "S",
                    "uses": [
                        {"card": {"name": "Hullbreaker Horror", "oracleId": "d4a84e78-d9b9-4c67-8a4b-4329e65f0f15"}},
                        {"card": {"name": "Sol Ring", "oracleId": "6ad8011d-3471-4369-9d68-b264cc027487"}}
                    ],
                    "produces": [
                        {"feature": {"name": "Infinite colorless mana"}},
                        {"feature": {"name": "Infinite storm count"}}
                    ],
                    "legalities": {"commander": true, "modern": false}
                }]
            }"#,
        );

        assert_eq!(bulk.variants.len(), 1);
        assert_eq!(
            bulk.timestamp.as_deref(),
            Some("2026-08-17T15:29:35.923525+00:00")
        );

        let mut report = FetchReport::default();
        let combo = convert(bulk.variants.into_iter().next().expect("one"), &mut report)
            .expect("converted");

        assert_eq!(combo.id, "513-5034--46");
        assert_eq!(combo.oracle_ids.len(), 2);
        assert_eq!(combo.card_names, ["Hullbreaker Horror", "Sol Ring"]);
        assert_eq!(combo.produces.len(), 2);
        assert!(combo.legal_in_commander);
        assert_eq!(combo.bracket_tag, "S");
    }

    #[test]
    fn the_nested_fields_the_real_file_carries_are_tolerated() {
        // The dump nests far more than the paginated API did: features have ids and statuses,
        // cards carry five image URLs each, produces entries have quantities.
        let bulk = parse(
            r#"{"timestamp": "t", "variants": [{
                "id": "1054-1538-1735", "status": "OK", "identity": "BR", "popularity": 6,
                "bracketTag": "E", "spoiler": false, "variantCount": 3,
                "prices": {"tcgplayer": "12.34"}, "manaNeeded": "{2}{R}",
                "uses": [{"card": {"id": 1735, "name": "Spellweaver Helix",
                          "oracleId": "9aa8eef1-fc67-4d54-9783-4d0175a76741", "faces": 1,
                          "imageUriFrontPng": "https://cards.scryfall.io/png/front/a/4/x.png",
                          "layoutRotationFront": null},
                         "zoneLocations": ["B"], "mustBeCommander": false}],
                "produces": [{"feature": {"id": 1, "name": "Near-infinite damage",
                              "uncountable": true, "status": "S"}, "quantity": 1}],
                "legalities": {"commander": true, "pauperCommanderMain": false}
            }]}"#,
        );

        let mut report = FetchReport::default();
        let combo = convert(bulk.variants.into_iter().next().expect("one"), &mut report)
            .expect("converted");
        assert_eq!(combo.card_names, ["Spellweaver Helix"]);
        assert_eq!(combo.produces, ["Near-infinite damage"]);
        assert!(combo.legal_in_commander);
    }

    #[test]
    fn unknown_fields_do_not_break_the_parse() {
        // An unofficial source will add fields; that must not fail a build.
        let bulk = parse(r#"{"variants": [], "somethingNew": {"nested": true}, "count": 5}"#);
        assert!(bulk.variants.is_empty());
        assert!(bulk.timestamp.is_none());
    }

    #[test]
    fn variants_spellbook_rejects_are_skipped_and_recorded() {
        // So a new status value shows up in the build output rather than silently changing
        // what ends up in the artifact.
        let bulk = parse(
            r#"{"variants": [{"id": "x", "status": "NOT_WORKING", "uses": [], "produces": []}]}"#,
        );
        let mut report = FetchReport::default();
        assert!(convert(bulk.variants.into_iter().next().expect("one"), &mut report).is_none());
        assert_eq!(report.skipped_not_ok, 1);
        assert!(report.unexpected_statuses.contains("NOT_WORKING"));
    }

    #[test]
    fn a_combo_with_a_card_lacking_an_oracle_id_is_dropped_whole() {
        // Keeping it with one piece missing would make it match decks it is not in.
        let bulk = parse(
            r#"{"variants": [{"id": "x", "status": "OK", "produces": [],
                 "uses": [{"card": {"name": "A", "oracleId": "o-a"}}, {"card": {"name": "B"}}]}]}"#,
        );
        let mut report = FetchReport::default();
        assert!(convert(bulk.variants.into_iter().next().expect("one"), &mut report).is_none());
        assert_eq!(report.skipped_without_oracle_id, 1);
    }

    #[test]
    fn missing_optional_fields_default_rather_than_failing() {
        let bulk = parse(
            r#"{"variants": [{"id": "x", "status": "OK",
                 "uses": [{"card": {"name": "A", "oracleId": "o-a"}}]}]}"#,
        );
        let mut report = FetchReport::default();
        let combo = convert(bulk.variants.into_iter().next().expect("one"), &mut report)
            .expect("converted");
        assert!(combo.produces.is_empty());
        assert!(combo.identity.is_empty());
        assert!(!combo.legal_in_commander, "absent legality is not legal");
        assert_eq!(combo.popularity, None);
    }
}
