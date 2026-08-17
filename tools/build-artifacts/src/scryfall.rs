//! Talking to Scryfall's bulk data endpoints.
//!
//! Two rules matter here and both are enforced, not just documented:
//!
//! * **A descriptive User-Agent is mandatory.** Scryfall blocks generic agents — their own
//!   documentation returns 403 to a default agent. [`USER_AGENT`] is what we send.
//! * **Bulk files are JSONL, gzipped, and that is the only format offered** since 20 July 2026.
//!   The old `download_uri` field no longer exists; only `jsonl_download_uri` does.

use std::io::Read;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Sent on every request. Scryfall asks for something that identifies the client and offers a
/// way to get in touch; a generic agent gets blocked outright.
pub const USER_AGENT: &str = concat!(
    "MagicOptimizer/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/eraflo/MagicOptimizer)"
);

const BULK_INDEX_URL: &str = "https://api.scryfall.com/bulk-data";

/// One entry in the bulk data index.
#[derive(Debug, Clone, Deserialize)]
pub struct BulkEntry {
    /// e.g. `oracle_cards`, `unique_artwork`, `default_cards`.
    #[serde(rename = "type")]
    pub kind: String,
    /// When Scryfall last rebuilt this file. Stored in the artifact so the app can report the
    /// age of its data without a network call.
    pub updated_at: String,
    /// The gzipped JSONL file. The only download format Scryfall still offers.
    pub jsonl_download_uri: String,
    #[serde(default)]
    pub compressed_size: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct BulkIndex {
    data: Vec<BulkEntry>,
}

/// Fetches the bulk data index.
pub fn fetch_bulk_index() -> Result<Vec<BulkEntry>> {
    let body = ureq::get(BULK_INDEX_URL)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .call()
        .context("requesting the Scryfall bulk data index")?
        .body_mut()
        .read_to_string()
        .context("reading the Scryfall bulk data index")?;

    let index: BulkIndex =
        serde_json::from_str(&body).context("parsing the Scryfall bulk data index")?;
    Ok(index.data)
}

/// Finds one bulk entry by its `type`.
pub fn find_entry(entries: &[BulkEntry], kind: &str) -> Result<BulkEntry> {
    entries
        .iter()
        .find(|e| e.kind == kind)
        .cloned()
        .with_context(|| {
            let available: Vec<&str> = entries.iter().map(|e| e.kind.as_str()).collect();
            format!("no bulk file of type {kind:?}; Scryfall offers {available:?}")
        })
}

/// Downloads a bulk file to `dest`, unless it is already there.
///
/// Bulk files are hundreds of megabytes, so the cache is not an optimization but a courtesy:
/// re-running the build should not re-download from someone else's servers.
pub fn download_cached(url: &str, dest: &Path) -> Result<()> {
    if dest.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating cache directory {}", parent.display()))?;
    }

    let response = ureq::get(url)
        .header("User-Agent", USER_AGENT)
        .call()
        .with_context(|| format!("downloading {url}"))?;

    if response.status() != 200 {
        bail!("downloading {url}: unexpected status {}", response.status());
    }

    // Download to a temporary name and rename on success, so an interrupted run never leaves
    // a truncated file that a later run would happily treat as cached.
    let partial = dest.with_extension("partial");
    let mut reader = response.into_body().into_reader();
    let mut file = std::fs::File::create(&partial)
        .with_context(|| format!("creating {}", partial.display()))?;
    std::io::copy(&mut reader, &mut file)
        .with_context(|| format!("writing {}", partial.display()))?;
    drop(file);

    std::fs::rename(&partial, dest)
        .with_context(|| format!("moving {} into place", partial.display()))?;
    Ok(())
}

/// Opens a bulk file for line-by-line reading, decompressing on the fly when needed.
///
/// Never loads the whole file: the all-cards bulk does not fit comfortably in memory.
pub fn open_jsonl(path: &Path) -> Result<Box<dyn Read>> {
    let file = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let buffered = std::io::BufReader::with_capacity(1 << 20, file);

    let is_gzip = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));

    Ok(if is_gzip {
        Box::new(flate2::read::GzDecoder::new(buffered))
    } else {
        Box::new(buffered)
    })
}

/// Turns a bulk entry into a stable cache filename.
pub fn cache_file_name(entry: &BulkEntry) -> String {
    // updated_at is an RFC 3339 timestamp; strip anything a filesystem would object to.
    let stamp: String = entry
        .updated_at
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{}-{stamp}.jsonl.gz", entry.kind)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    fn entry(kind: &str, updated_at: &str) -> BulkEntry {
        BulkEntry {
            kind: kind.to_owned(),
            updated_at: updated_at.to_owned(),
            jsonl_download_uri: String::new(),
            compressed_size: None,
        }
    }

    #[test]
    fn user_agent_is_descriptive() {
        // A generic agent gets a 403 from Scryfall, so this is a functional requirement and
        // not a nicety.
        assert!(USER_AGENT.starts_with("MagicOptimizer/"));
        assert!(USER_AGENT.contains("github.com/eraflo/MagicOptimizer"));
    }

    #[test]
    fn cache_name_is_filesystem_safe() {
        let name = cache_file_name(&entry("oracle_cards", "2026-08-17T09:01:54.476+00:00"));
        assert_eq!(name, "oracle_cards-2026-08-17T09-01-54-476-00-00.jsonl.gz");
        assert!(!name.contains(':'));
        assert!(!name.contains('+'));
    }

    #[test]
    fn cache_name_changes_when_scryfall_rebuilds() {
        let a = cache_file_name(&entry("oracle_cards", "2026-08-17T09:01:54.476+00:00"));
        let b = cache_file_name(&entry("oracle_cards", "2026-08-18T09:01:54.476+00:00"));
        assert_ne!(a, b, "a new bulk build must not reuse a stale cache file");
    }

    #[test]
    fn missing_bulk_type_reports_what_is_available() {
        let entries = vec![entry("oracle_cards", ""), entry("rulings", "")];
        let err = find_entry(&entries, "unique_artwork").unwrap_err();
        let message = format!("{err}");
        assert!(message.contains("unique_artwork"), "{message}");
        assert!(message.contains("oracle_cards"), "{message}");
    }

    #[test]
    fn bulk_index_parses_the_current_shape() {
        // Trimmed from a real response on 2026-08-17. Note there is no `download_uri`: the
        // JSON-only fields were retired, and a parser that required them would break.
        let json = r#"{
            "object": "list",
            "data": [{
                "object": "bulk_data",
                "id": "27bf3214-1271-490b-bdfe-c0be6c23d02e",
                "type": "oracle_cards",
                "updated_at": "2026-08-17T09:01:54.476+00:00",
                "name": "Oracle Cards",
                "description": "...",
                "compressed_size": 12345,
                "jsonl_download_uri": "https://data.scryfall.io/oracle-cards/oracle-cards-20260817090154.jsonl.gz"
            }]
        }"#;

        let index: BulkIndex = serde_json::from_str(json).unwrap();
        let oracle = find_entry(&index.data, "oracle_cards").unwrap();
        assert!(oracle.jsonl_download_uri.ends_with(".jsonl.gz"));
        assert_eq!(oracle.compressed_size, Some(12345));
    }
}
