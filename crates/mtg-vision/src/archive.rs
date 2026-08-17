//! Reading and writing `arthashes.bin`.
//!
//! # The format
//!
//! ```text
//! "MTGART\0"        7 bytes   magic
//! version           u8        format version
//! count             u32 LE    number of entries
//! then `count` records, each:
//!   hash            32 bytes
//!   printing_id     u8 length, then that many bytes of UTF-8
//!   oracle_id       u8 length, then that many bytes of UTF-8
//!   name            u16 length, then that many bytes of UTF-8
//! ```
//!
//! Deliberately not `rkyv`, unlike the card catalog. The catalog is mmapped and read
//! zero-copy because it is large and randomly accessed; this file is a flat list that is walked
//! once at startup and then only ever compared byte-by-byte. A hand-written format keeps
//! `mtg-vision` free of the archiving machinery, which matters for a crate that also has to be
//! small on Android.
//!
//! Everything here treats the file as **untrusted**: it is downloaded from a GitHub release,
//! and a truncated download must produce an error rather than a panic.

use std::io::{Read, Write};

use crate::hash::{ArtHash, HASH_BYTES};
use crate::matcher::{ArtDatabase, ArtEntry};

const MAGIC: &[u8; 7] = b"MTGART\0";

/// Bumped whenever the layout or the hashing changes.
///
/// The hash is part of the contract: a file written by a different hasher would match nothing,
/// silently. Changing [`crate::hash::hash_gray`] means bumping this.
pub const ARCHIVE_VERSION: u8 = 1;

/// A ceiling on the entry count, so a corrupt header cannot ask for a huge allocation.
///
/// Scryfall has on the order of 60,000 distinct artworks; a million is far above any plausible
/// growth and far below anything that would hurt.
const MAX_ENTRIES: u32 = 1_000_000;

#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("not an artwork archive (bad magic)")]
    NotAnArchive,
    #[error("archive version {found} is not supported (this build reads {supported})")]
    UnsupportedVersion { found: u8, supported: u8 },
    #[error("archive claims {0} entries, which is not plausible")]
    ImplausibleCount(u32),
    #[error("archive ends in the middle of entry {0} — the download is probably incomplete")]
    Truncated(u32),
    #[error("entry {index} has invalid UTF-8 in its {field}")]
    InvalidText { index: u32, field: &'static str },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Writes a database out.
pub fn write<W: Write>(writer: &mut W, entries: &[ArtEntry]) -> Result<(), ArchiveError> {
    writer.write_all(MAGIC)?;
    writer.write_all(&[ARCHIVE_VERSION])?;
    writer.write_all(&(entries.len() as u32).to_le_bytes())?;

    for entry in entries {
        writer.write_all(entry.hash.as_bytes())?;
        write_short(writer, &entry.printing_id)?;
        write_short(writer, &entry.oracle_id)?;
        write_long(writer, &entry.name)?;
    }
    Ok(())
}

/// Reads a database back.
pub fn read<R: Read>(reader: &mut R) -> Result<ArtDatabase, ArchiveError> {
    let mut header = [0u8; 12];
    reader
        .read_exact(&mut header)
        .map_err(|_| ArchiveError::NotAnArchive)?;

    if &header[..7] != MAGIC {
        return Err(ArchiveError::NotAnArchive);
    }
    let version = header[7];
    if version != ARCHIVE_VERSION {
        return Err(ArchiveError::UnsupportedVersion {
            found: version,
            supported: ARCHIVE_VERSION,
        });
    }

    let count = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
    if count > MAX_ENTRIES {
        return Err(ArchiveError::ImplausibleCount(count));
    }

    // Not `with_capacity(count)`: the count is external data, and reserving on it would let a
    // corrupt header allocate megabytes before the first byte of payload is even read.
    let mut entries = Vec::new();
    for index in 0..count {
        let mut hash = [0u8; HASH_BYTES];
        reader
            .read_exact(&mut hash)
            .map_err(|_| ArchiveError::Truncated(index))?;

        let printing_id = read_short(reader, index, "printing id")?;
        let oracle_id = read_short(reader, index, "oracle id")?;
        let name = read_long(reader, index, "name")?;

        entries.push(ArtEntry {
            hash: ArtHash(hash),
            printing_id,
            oracle_id,
            name,
        });
    }

    Ok(ArtDatabase::new(entries))
}

fn write_short<W: Write>(writer: &mut W, text: &str) -> Result<(), ArchiveError> {
    // Ids are 36-character UUIDs; anything longer is not one, and truncating keeps the format
    // honest about its own length prefix.
    let bytes = text.as_bytes();
    let length = bytes.len().min(u8::MAX as usize);
    writer.write_all(&[length as u8])?;
    writer.write_all(&bytes[..length])?;
    Ok(())
}

fn write_long<W: Write>(writer: &mut W, text: &str) -> Result<(), ArchiveError> {
    let bytes = text.as_bytes();
    let length = bytes.len().min(u16::MAX as usize);
    writer.write_all(&(length as u16).to_le_bytes())?;
    writer.write_all(&bytes[..length])?;
    Ok(())
}

fn read_short<R: Read>(
    reader: &mut R,
    index: u32,
    field: &'static str,
) -> Result<String, ArchiveError> {
    let mut length = [0u8; 1];
    reader
        .read_exact(&mut length)
        .map_err(|_| ArchiveError::Truncated(index))?;
    let mut bytes = vec![0u8; length[0] as usize];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| ArchiveError::Truncated(index))?;
    String::from_utf8(bytes).map_err(|_| ArchiveError::InvalidText { index, field })
}

fn read_long<R: Read>(
    reader: &mut R,
    index: u32,
    field: &'static str,
) -> Result<String, ArchiveError> {
    let mut length = [0u8; 2];
    reader
        .read_exact(&mut length)
        .map_err(|_| ArchiveError::Truncated(index))?;
    let mut bytes = vec![0u8; u16::from_le_bytes(length) as usize];
    reader
        .read_exact(&mut bytes)
        .map_err(|_| ArchiveError::Truncated(index))?;
    String::from_utf8(bytes).map_err(|_| ArchiveError::InvalidText { index, field })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, seed: u8) -> ArtEntry {
        ArtEntry {
            hash: ArtHash([seed; HASH_BYTES]),
            printing_id: format!("{seed:08}-0000-0000-0000-000000000000"),
            oracle_id: format!("{seed:08}-1111-1111-1111-111111111111"),
            name: name.to_owned(),
        }
    }

    fn round_trip(entries: &[ArtEntry]) -> Result<ArtDatabase, ArchiveError> {
        let mut buffer = Vec::new();
        write(&mut buffer, entries).expect("write");
        read(&mut buffer.as_slice())
    }

    #[test]
    fn what_goes_in_comes_back_out() {
        let entries = vec![entry("Sol Ring", 1), entry("Lightning Bolt", 2)];
        let database = round_trip(&entries).expect("read");
        assert_eq!(database.entries(), entries.as_slice());
    }

    #[test]
    fn an_empty_archive_is_valid() {
        let database = round_trip(&[]).expect("read");
        assert!(database.is_empty());
    }

    #[test]
    fn names_with_accents_and_split_cards_survive() {
        // Æther, Lim-Dûl, "Fire // Ice" — the names that break naive length handling.
        let entries = vec![
            entry("Æther Vial", 3),
            entry("Lim-Dûl's Vault", 4),
            entry("Fire // Ice", 5),
        ];
        let database = round_trip(&entries).expect("read");
        assert_eq!(database.entries(), entries.as_slice());
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused() {
        let mut junk = b"<!DOCTYPE html><html>404".as_slice();
        assert!(matches!(read(&mut junk), Err(ArchiveError::NotAnArchive)));
    }

    #[test]
    fn an_empty_file_is_refused_rather_than_read_as_empty() {
        // A download that produced nothing must not look like a valid empty database.
        assert!(matches!(
            read(&mut [].as_slice()),
            Err(ArchiveError::NotAnArchive)
        ));
    }

    #[test]
    fn an_archive_from_a_different_version_is_refused() {
        // The hash is part of the contract. Reading an old file would match nothing, silently.
        let mut buffer = Vec::new();
        write(&mut buffer, &[entry("Sol Ring", 1)]).expect("write");
        buffer[7] = ARCHIVE_VERSION.wrapping_add(1);

        assert!(matches!(
            read(&mut buffer.as_slice()),
            Err(ArchiveError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn a_truncated_download_is_an_error_not_a_panic() {
        // The failure this format is most likely to actually meet.
        let entries = vec![entry("Sol Ring", 1), entry("Lightning Bolt", 2)];
        let mut buffer = Vec::new();
        write(&mut buffer, &entries).expect("write");

        for cut in [13, 20, 60, buffer.len() - 1] {
            assert!(
                read(&mut &buffer[..cut]).is_err(),
                "a file cut at {cut} bytes was accepted"
            );
        }
    }

    #[test]
    fn an_absurd_entry_count_is_refused_before_allocating() {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(MAGIC);
        buffer.push(ARCHIVE_VERSION);
        buffer.extend_from_slice(&u32::MAX.to_le_bytes());

        assert!(matches!(
            read(&mut buffer.as_slice()),
            Err(ArchiveError::ImplausibleCount(_))
        ));
    }

    #[test]
    fn invalid_text_is_reported_rather_than_replaced() {
        let mut buffer = Vec::new();
        write(&mut buffer, &[entry("Sol Ring", 1)]).expect("write");
        // The printing id starts right after the 12-byte header and the 32-byte hash: one
        // length byte, then its bytes.
        buffer[45] = 0xff;

        assert!(matches!(
            read(&mut buffer.as_slice()),
            Err(ArchiveError::InvalidText { .. })
        ));
    }

    #[test]
    fn a_realistic_archive_round_trips() {
        // Enough entries to shake out anything that only works for one record.
        let entries: Vec<ArtEntry> = (0..500)
            .map(|index| entry(&format!("Card {index}"), (index % 251) as u8))
            .collect();
        let database = round_trip(&entries).expect("read");
        assert_eq!(database.len(), 500);
        assert_eq!(database.entries(), entries.as_slice());
    }
}
