//! Fetching the combo snapshot from Commander Spellbook.
//!
//! # This is an unofficial endpoint
//!
//! Commander Spellbook is a community project with no published API contract. The shape below
//! is what it returned on 2026-08-17, and it may change without notice. Everything here is
//! written to fail loudly at build time rather than quietly produce a wrong artifact — and the
//! app treats the combo database as optional, so a broken build here costs a feature rather
//! than the whole application.

use std::collections::BTreeSet;
use std::time::Duration;

use anyhow::{Context, Result};
use mtg_combo::Combo;
use serde::Deserialize;

use crate::scryfall::USER_AGENT;

const BASE_URL: &str = "https://backend.commanderspellbook.com/variants/";

/// Most the endpoint returns per request, whatever you ask for.
const PAGE_SIZE: u32 = 100;

/// Between requests. Their servers are donated infrastructure for a free community project;
/// there is no reason to hammer them for a snapshot that is taken once per build.
///
/// 250ms was not enough — a full fetch was rate-limited after about a hundred pages.
const DELAY: Duration = Duration::from_millis(600);

/// How many times to retry a page that was refused.
const MAX_RETRIES: u32 = 6;

/// First backoff after a refusal, doubling from there.
const FIRST_BACKOFF: Duration = Duration::from_secs(5);

/// A hard stop, so a change in the pagination contract cannot spin forever.
const MAX_PAGES: u32 = 2_000;

#[derive(Debug, Deserialize)]
struct Page {
    results: Vec<Variant>,
    /// Absent or null on the last page. There is no `count`, so this is the only way to know
    /// when to stop.
    #[serde(default)]
    next: Option<String>,
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
    legalities: std::collections::HashMap<String, bool>,
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
    pub pages: u32,
    pub variants_seen: usize,
    pub kept: usize,
    /// Variants Spellbook itself does not consider valid.
    pub skipped_not_ok: usize,
    /// Variants using a card with no oracle id, which cannot be matched to a deck.
    pub skipped_without_oracle_id: usize,
    /// Status values we did not expect, so a contract change is visible rather than silent.
    pub unexpected_statuses: BTreeSet<String>,
}

/// Downloads every combo variant.
pub fn fetch_combos() -> Result<(Vec<Combo>, FetchReport)> {
    let mut report = FetchReport::default();
    let mut combos = Vec::new();
    let mut url = format!("{BASE_URL}?limit={PAGE_SIZE}");

    loop {
        if report.pages >= MAX_PAGES {
            anyhow::bail!(
                "stopped after {MAX_PAGES} pages — the pagination contract has probably changed"
            );
        }

        let body = get_with_retry(&url)?;

        let page: Page = serde_json::from_str(&body).with_context(|| format!("parsing {url}"))?;
        report.pages += 1;
        report.variants_seen += page.results.len();

        for variant in page.results {
            match convert(variant, &mut report) {
                Some(combo) => combos.push(combo),
                None => continue,
            }
        }

        match page.next {
            Some(next) if !next.is_empty() => url = next,
            _ => break,
        }
        std::thread::sleep(DELAY);
    }

    report.kept = combos.len();
    Ok((combos, report))
}

/// Fetches one page, backing off and retrying when the server pushes back.
///
/// A full fetch is a few hundred requests and reliably ran into `429 Too Many Requests`
/// partway through. Giving up there would mean never getting a complete snapshot, so refusals
/// are waited out rather than treated as failures. Only a refusal is retried: a malformed
/// response is a contract change and should fail the build loudly.
fn get_with_retry(url: &str) -> Result<String> {
    let mut backoff = FIRST_BACKOFF;

    for attempt in 0..=MAX_RETRIES {
        match ureq::get(url)
            .header("User-Agent", USER_AGENT)
            .header("Accept", "application/json")
            .call()
        {
            Ok(mut response) => {
                return response
                    .body_mut()
                    .read_to_string()
                    .with_context(|| format!("reading {url}"));
            }
            Err(error) if is_worth_retrying(&error) && attempt < MAX_RETRIES => {
                eprintln!("  {error}; waiting {}s before retrying", backoff.as_secs());
                std::thread::sleep(backoff);
                backoff *= 2;
            }
            Err(error) => {
                return Err(anyhow::Error::new(error)).with_context(|| format!("requesting {url}"));
            }
        }
    }

    anyhow::bail!("gave up on {url} after {MAX_RETRIES} retries")
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
        let oracle_id = used.card.oracle_id.as_ref()?;
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
        legal_in_commander: variant
            .legalities
            .get("commander")
            .copied()
            .unwrap_or(false),
        popularity: variant.popularity,
        bracket_tag: variant.bracket_tag.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Page {
        serde_json::from_str(json).expect("parse")
    }

    #[test]
    fn only_transient_failures_are_retried() {
        // Rate limiting and server errors pass; a contract change must fail loudly instead of
        // being retried six times and then failing anyway.
        assert!(is_worth_retrying(&ureq::Error::StatusCode(429)));
        assert!(is_worth_retrying(&ureq::Error::StatusCode(503)));
        assert!(!is_worth_retrying(&ureq::Error::StatusCode(404)));
        assert!(!is_worth_retrying(&ureq::Error::StatusCode(400)));
    }

    #[test]
    fn a_page_of_the_current_shape_parses() {
        // Trimmed from a real response on 2026-08-17.
        let page = parse(
            r#"{
                "count": null,
                "next": "https://backend.commanderspellbook.com/variants/?limit=100&offset=100",
                "previous": null,
                "results": [{
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

        assert_eq!(page.results.len(), 1);
        assert!(page.next.is_some());

        let mut report = FetchReport::default();
        let combo =
            convert(page.results.into_iter().next().expect("one"), &mut report).expect("converted");

        assert_eq!(combo.id, "513-5034--46");
        assert_eq!(combo.oracle_ids.len(), 2);
        assert_eq!(combo.card_names, ["Hullbreaker Horror", "Sol Ring"]);
        assert_eq!(combo.produces.len(), 2);
        assert!(combo.legal_in_commander);
        assert_eq!(combo.bracket_tag, "S");
    }

    #[test]
    fn the_last_page_has_no_next() {
        let page = parse(r#"{"results": [], "next": null}"#);
        assert!(page.next.is_none());
    }

    #[test]
    fn unknown_fields_do_not_break_the_parse() {
        // An unofficial endpoint will add fields; that must not fail a build.
        let page =
            parse(r#"{"results": [], "next": null, "somethingNew": {"nested": true}, "count": 5}"#);
        assert!(page.results.is_empty());
    }

    #[test]
    fn variants_spellbook_rejects_are_skipped_and_recorded() {
        // So a new status value shows up in the build output rather than silently changing
        // what ends up in the artifact.
        let page = parse(
            r#"{"results": [{"id": "x", "status": "NOT_WORKING", "uses": [], "produces": []}], "next": null}"#,
        );
        let mut report = FetchReport::default();
        assert!(convert(page.results.into_iter().next().expect("one"), &mut report).is_none());
        assert_eq!(report.skipped_not_ok, 1);
        assert!(report.unexpected_statuses.contains("NOT_WORKING"));
    }

    #[test]
    fn a_combo_with_a_card_lacking_an_oracle_id_is_dropped_whole() {
        // Keeping it with one piece missing would make it match decks it is not in.
        let page = parse(
            r#"{"results": [{"id": "x", "status": "OK", "produces": [],
                 "uses": [{"card": {"name": "A", "oracleId": "o-a"}}, {"card": {"name": "B"}}]}],
                "next": null}"#,
        );
        let mut report = FetchReport::default();
        assert!(convert(page.results.into_iter().next().expect("one"), &mut report).is_none());
    }

    #[test]
    fn missing_optional_fields_default_rather_than_failing() {
        let page = parse(
            r#"{"results": [{"id": "x", "status": "OK",
                 "uses": [{"card": {"name": "A", "oracleId": "o-a"}}]}],
                "next": null}"#,
        );
        let mut report = FetchReport::default();
        let combo =
            convert(page.results.into_iter().next().expect("one"), &mut report).expect("converted");
        assert!(combo.produces.is_empty());
        assert!(combo.identity.is_empty());
        assert!(!combo.legal_in_commander, "absent legality is not legal");
        assert_eq!(combo.popularity, None);
    }
}
