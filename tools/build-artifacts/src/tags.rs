//! Fetching functional tags from Scryfall's tagger.
//!
//! # Why one search per tag
//!
//! There is no bulk export of tags: they are not in the bulk files, and the public API offers
//! no way to ask a card what it is tagged with. What it does offer is the reverse — `otag:x`
//! returns every card carrying tag `x` — so the build walks the vocabulary rather than the
//! catalog. Around 52,000 card rows across 35 tags, which is roughly 300 paginated requests.
//!
//! # This is an unofficial, community-maintained taxonomy
//!
//! The tagger is crowdsourced. Coverage is uneven — measured, twelve of the broadest tags cover
//! 61% of non-land cards — and a card carrying no tag is genuinely ambiguous between "does
//! nothing" and "nobody has tagged it". Grizzly Bears really is in the first group. Nothing
//! downstream may read an empty set as "this card does nothing".
//!
//! Tag names also drift, exactly like the legality keys. A tag that returns no cards is
//! reported **loudly** rather than skipped, because the alternative is a whole role quietly
//! vanishing from every deck's analysis with nothing anywhere saying why.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use anyhow::{Context, Result};
use mtg_core::Tag;
use serde::Deserialize;

use crate::scryfall::USER_AGENT;

/// Between requests. Scryfall asks for 50–100 ms; this takes the slower end, as the rest of the
/// tool does.
const DELAY: Duration = Duration::from_millis(100);

/// A hard stop per tag, so a change in the pagination contract cannot spin forever.
///
/// The largest tag is `removal` at 6,428 cards, which is 37 pages. A hundred is far above that
/// and far below anything that would matter.
const MAX_PAGES_PER_TAG: u32 = 100;

/// Only what is needed. Serde ignores the rest of each card, which is most of it.
#[derive(Debug, Deserialize)]
struct Page {
    #[serde(default)]
    data: Vec<TaggedCard>,
    #[serde(default)]
    next_page: Option<String>,
    #[serde(default)]
    total_cards: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TaggedCard {
    /// Absent on the handful of entries that are not real cards.
    #[serde(default)]
    oracle_id: Option<String>,
}

#[derive(Debug, Default)]
pub struct TagReport {
    pub requests: u32,
    /// Cards carrying at least one tag.
    pub cards_tagged: usize,
    /// How many cards each tag matched, for the build log.
    pub per_tag: Vec<(Tag, usize)>,
    /// Tags that returned nothing. A drifted name, and a whole role gone.
    pub empty_tags: BTreeSet<&'static str>,
}

/// Fetches every tag in the vocabulary, returning oracle id → [`mtg_core::TagSet`] bits.
pub fn fetch_tags() -> Result<(HashMap<String, u64>, TagReport)> {
    let mut tags: HashMap<String, u64> = HashMap::new();
    let mut report = TagReport::default();

    for tag in Tag::ALL {
        let mut url = format!(
            "https://api.scryfall.com/cards/search?unique=cards&q={}",
            urlencode(&format!("otag:{} -is:token", tag.scryfall_tag()))
        );
        let mut matched = 0usize;
        let mut pages = 0u32;

        loop {
            if pages >= MAX_PAGES_PER_TAG {
                anyhow::bail!(
                    "tag {} did not stop after {MAX_PAGES_PER_TAG} pages — the pagination \
                     contract has probably changed",
                    tag.scryfall_tag()
                );
            }

            std::thread::sleep(DELAY);
            report.requests += 1;

            // A tag with no cards answers 404, which is a drifted name rather than a failure:
            // it is recorded and the build carries on, then says so loudly at the end.
            let Some(body) = get_or_none(&url)? else {
                break;
            };
            let page: Page = serde_json::from_str(&body).with_context(|| {
                format!("parsing the card list for otag:{}", tag.scryfall_tag())
            })?;
            pages += 1;

            if pages == 1 && page.total_cards == Some(0) {
                break;
            }

            for card in page.data {
                if let Some(oracle_id) = card.oracle_id {
                    *tags.entry(oracle_id).or_insert(0) |= tag.bit();
                    matched += 1;
                }
            }

            match page.next_page {
                Some(next) if !next.is_empty() => url = next,
                _ => break,
            }
        }

        if matched == 0 {
            report.empty_tags.insert(tag.scryfall_tag());
        }
        report.per_tag.push((tag, matched));
        println!("  {:<18} {matched:>6}", tag.scryfall_tag());
    }

    report.cards_tagged = tags.len();
    Ok((tags, report))
}

/// Percent-encodes a query. Small enough not to be worth a dependency.
fn urlencode(value: &str) -> String {
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Fetches a page, treating "no cards match" as an answer rather than an error.
fn get_or_none(url: &str) -> Result<Option<String>> {
    match ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
    {
        Ok(mut response) => Ok(Some(
            response
                .body_mut()
                .read_to_string()
                .with_context(|| format!("reading {url}"))?,
        )),
        // Scryfall answers 404 for a search that matched nothing, which is exactly what a
        // drifted tag name looks like.
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(error) => Err(anyhow::Error::new(error)).with_context(|| format!("requesting {url}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_of_the_current_shape_parses() {
        // Trimmed from a real response on 2026-08-18. Every card carries about forty more
        // fields than this; serde drops them, which is why the parse is cheap.
        let page: Page = serde_json::from_str(
            r#"{
                "object": "list",
                "total_cards": 6428,
                "has_more": true,
                "next_page": "https://api.scryfall.com/cards/search?page=2&q=otag%3Aremoval",
                "data": [
                    {"object": "card", "name": "Lightning Bolt",
                     "oracle_id": "4457ed35-7c10-48c8-9776-456485fdf070",
                     "mana_cost": "{R}", "cmc": 1.0},
                    {"object": "card", "name": "Swords to Plowshares",
                     "oracle_id": "6d5ee7a5-937a-4c2b-af9c-1f1a5d97e2a6"}
                ]
            }"#,
        )
        .expect("parse");

        assert_eq!(page.data.len(), 2);
        assert_eq!(page.total_cards, Some(6428));
        assert!(page.next_page.is_some());
    }

    #[test]
    fn the_last_page_has_no_next() {
        let page: Page = serde_json::from_str(r#"{"data": [], "total_cards": 3}"#).expect("parse");
        assert!(page.next_page.is_none());
    }

    #[test]
    fn a_card_without_an_oracle_id_does_not_break_the_page() {
        // Nothing can be keyed on it, so it is skipped rather than failing the whole tag.
        let page: Page =
            serde_json::from_str(r#"{"data": [{"object": "card", "name": "?"}]}"#).expect("parse");
        assert_eq!(page.data.len(), 1);
        assert!(page.data[0].oracle_id.is_none());
    }

    #[test]
    fn queries_are_encoded_so_the_colon_and_space_survive() {
        // `otag:removal -is:token` has to reach Scryfall intact; a raw space would truncate it
        // and silently return the wrong card list.
        assert_eq!(
            urlencode("otag:removal -is:token"),
            "otag%3Aremoval%20-is%3Atoken"
        );
    }

    #[test]
    fn every_tag_in_the_vocabulary_has_a_distinct_bit() {
        // The fetch ORs bits together per card; two tags sharing one would make a card look
        // like it had a role it does not.
        let mut bits = 0u64;
        for tag in Tag::ALL {
            assert_eq!(bits & tag.bit(), 0, "{tag:?} collides");
            bits |= tag.bit();
        }
        assert_eq!(bits.count_ones() as usize, Tag::ALL.len());
    }
}
