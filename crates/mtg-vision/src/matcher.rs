//! Matching a hash against the known artwork.
//!
//! Brute force over every hash, and deliberately so. Fifty thousand entries at 32 bytes each
//! is 1.6 MB — small enough to sit in cache — and a Hamming distance is a handful of XOR and
//! popcount instructions. An approximate nearest-neighbour index would be more code, more
//! memory and another failure mode, in exchange for microseconds nobody would notice.

use serde::{Deserialize, Serialize};

use crate::hash::{ArtHash, HASH_BITS};

/// Bits of difference still counted as the same artwork.
///
/// Same painting through a phone camera lands within roughly a dozen bits; different paintings
/// land near 128, which is what chance gives. The gap between those is wide, so the exact
/// threshold matters less than it looks — this sits well inside it.
pub const DEFAULT_MAX_DISTANCE: u32 = 28;

/// How much closer the best match must be than the runner-up.
///
/// Two different printings of the same artwork are genuinely indistinguishable here, and that
/// is the point: they *are* the same picture. But two unrelated cards landing equally close
/// means the frame was bad, and returning either would be a guess. This is the check that
/// turns "closest" into "confidently closest".
pub const DEFAULT_MARGIN: u32 = 8;

/// One artwork in the database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtEntry {
    pub hash: ArtHash,
    /// Scryfall printing id, which identifies the exact printing this art belongs to.
    pub printing_id: String,
    /// Oracle id, so a match lands straight in a deck or collection.
    pub oracle_id: String,
    pub name: String,
}

/// What a lookup found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Match {
    pub printing_id: String,
    pub oracle_id: String,
    pub name: String,
    /// Bits of difference. Lower is better; zero is identical.
    pub distance: u32,
    /// How much worse the next-best *different card* was. Larger means more certain.
    pub margin: u32,
}

/// The artwork database.
#[derive(Debug, Clone, Default)]
pub struct ArtDatabase {
    entries: Vec<ArtEntry>,
}

impl ArtDatabase {
    pub fn new(entries: Vec<ArtEntry>) -> ArtDatabase {
        ArtDatabase { entries }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[ArtEntry] {
        &self.entries
    }

    /// Finds the artwork a hash belongs to, or nothing if the answer is not clear.
    ///
    /// Returns `None` rather than a best guess when the closest entry is too far away, or when
    /// a *different card* is nearly as close. A wrong card added silently to a collection is
    /// worse than a card the scanner declines to name.
    pub fn best_match(&self, hash: &ArtHash) -> Option<Match> {
        self.best_match_with(hash, DEFAULT_MAX_DISTANCE, DEFAULT_MARGIN)
    }

    /// As [`ArtDatabase::best_match`], with the thresholds spelled out.
    pub fn best_match_with(&self, hash: &ArtHash, max_distance: u32, margin: u32) -> Option<Match> {
        let mut best: Option<(&ArtEntry, u32)> = None;
        let mut runner_up = HASH_BITS as u32 + 1;

        for entry in &self.entries {
            let distance = hash.distance(&entry.hash);
            match best {
                Some((current, best_distance)) if distance >= best_distance => {
                    // Only a *different card* counts as a rival. Two printings of the same
                    // artwork sit at the same distance by definition, and letting them
                    // compete would make every reprinted card unrecognisable.
                    if entry.oracle_id != current.oracle_id {
                        runner_up = runner_up.min(distance);
                    }
                }
                Some((current, _)) => {
                    if current.oracle_id != entry.oracle_id {
                        runner_up = runner_up.min(best.map_or(u32::MAX, |(_, d)| d));
                    }
                    best = Some((entry, distance));
                }
                None => best = Some((entry, distance)),
            }
        }

        let (entry, distance) = best?;
        if distance > max_distance {
            return None;
        }

        let gap = runner_up.saturating_sub(distance);
        if runner_up <= HASH_BITS as u32 && gap < margin {
            return None;
        }

        Some(Match {
            printing_id: entry.printing_id.clone(),
            oracle_id: entry.oracle_id.clone(),
            name: entry.name.clone(),
            distance,
            margin: gap,
        })
    }

    /// Every printing whose artwork matches, closest first.
    ///
    /// A reprint shares its painting with the original, so the hash cannot tell them apart —
    /// by design, since they are the same picture. This is what lets the UI offer the choice
    /// rather than picking a printing the user never saw.
    pub fn printings_of(&self, oracle_id: &str) -> Vec<&ArtEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.oracle_id == oracle_id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash_of(bits: &[usize]) -> ArtHash {
        let mut bytes = [0u8; crate::hash::HASH_BYTES];
        for bit in bits {
            bytes[bit / 8] |= 1 << (bit % 8);
        }
        ArtHash(bytes)
    }

    fn entry(name: &str, oracle: &str, printing: &str, bits: &[usize]) -> ArtEntry {
        ArtEntry {
            hash: hash_of(bits),
            printing_id: printing.to_owned(),
            oracle_id: oracle.to_owned(),
            name: name.to_owned(),
        }
    }

    /// Cards far apart from each other, as real artwork is.
    fn database() -> ArtDatabase {
        ArtDatabase::new(vec![
            entry(
                "Forest",
                "o-forest",
                "p-forest-1",
                &(0..40).collect::<Vec<_>>(),
            ),
            entry(
                "Island",
                "o-island",
                "p-island-1",
                &(80..130).collect::<Vec<_>>(),
            ),
            entry(
                "Mountain",
                "o-mountain",
                "p-mountain-1",
                &(160..210).collect::<Vec<_>>(),
            ),
        ])
    }

    #[test]
    fn an_exact_hash_matches_its_card() {
        let db = database();
        let found = db
            .best_match(&hash_of(&(0..40).collect::<Vec<_>>()))
            .expect("match");
        assert_eq!(found.name, "Forest");
        assert_eq!(found.distance, 0);
    }

    #[test]
    fn a_slightly_noisy_hash_still_matches() {
        // A camera never reproduces a hash exactly; a handful of flipped bits is normal.
        let db = database();
        let mut bits: Vec<usize> = (0..40).collect();
        bits.extend([250, 251, 252, 253, 254]);

        let found = db.best_match(&hash_of(&bits)).expect("match");
        assert_eq!(found.name, "Forest");
        assert_eq!(found.distance, 5);
    }

    #[test]
    fn a_hash_that_matches_nothing_returns_nothing() {
        // A blurry frame, a thumb over the card, a coaster. Naming a card here would be worse
        // than declining to.
        let db = database();
        assert!(db
            .best_match(&hash_of(&(200..256).collect::<Vec<_>>()))
            .is_none());
    }

    #[test]
    fn an_ambiguous_hash_is_refused_rather_than_guessed() {
        // Two unrelated cards equally close means the frame was bad.
        let db = ArtDatabase::new(vec![
            entry("A", "o-a", "p-a", &[0, 1, 2, 3]),
            entry("B", "o-b", "p-b", &[4, 5, 6, 7]),
        ]);
        // Four bits from each: equidistant.
        assert!(db.best_match(&hash_of(&[0, 1, 4, 5])).is_none());
    }

    #[test]
    fn two_printings_of_the_same_art_do_not_block_each_other() {
        // They are the same painting, so they sit at the same distance. Treating that as
        // ambiguity would make every reprinted card unrecognisable.
        let db = ArtDatabase::new(vec![
            entry("Sol Ring", "o-sol", "p-sol-a", &[0, 1, 2, 3]),
            entry("Sol Ring", "o-sol", "p-sol-b", &[0, 1, 2, 3]),
            entry(
                "Island",
                "o-island",
                "p-island",
                &(100..150).collect::<Vec<_>>(),
            ),
        ]);

        let found = db.best_match(&hash_of(&[0, 1, 2, 3])).expect("match");
        assert_eq!(found.oracle_id, "o-sol");
    }

    #[test]
    fn every_printing_of_a_card_can_be_listed() {
        // What the UI offers when a reprint cannot be told from the original — because it
        // genuinely cannot.
        let db = ArtDatabase::new(vec![
            entry("Sol Ring", "o-sol", "p-sol-a", &[0]),
            entry("Sol Ring", "o-sol", "p-sol-b", &[0]),
            entry("Island", "o-island", "p-island", &[100]),
        ]);

        let printings = db.printings_of("o-sol");
        assert_eq!(printings.len(), 2);
        assert!(db.printings_of("o-nothing").is_empty());
    }

    #[test]
    fn an_empty_database_matches_nothing_without_complaining() {
        // What the app has before the optional artwork hashes are downloaded.
        let db = ArtDatabase::default();
        assert!(db.is_empty());
        assert!(db.best_match(&hash_of(&[1, 2, 3])).is_none());
    }

    #[test]
    fn the_thresholds_can_be_tightened_or_loosened() {
        let db = database();
        let mut bits: Vec<usize> = (0..40).collect();
        bits.extend(240..250);

        // Ten bits out: inside the default, outside a strict threshold.
        assert!(db.best_match_with(&hash_of(&bits), 28, 8).is_some());
        assert!(db.best_match_with(&hash_of(&bits), 5, 8).is_none());
    }

    #[test]
    fn a_match_reports_how_certain_it_was() {
        // So the caller can require more confidence than the defaults if it wants.
        let db = database();
        let found = db
            .best_match(&hash_of(&(0..40).collect::<Vec<_>>()))
            .expect("match");
        assert!(found.margin > 0, "the runner-up should be measurably worse");
    }
}
