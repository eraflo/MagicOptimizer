//! Errors from loading and reading the catalog.

/// Something went wrong opening or reading a catalog artifact.
///
/// Artifacts arrive over the network, so every failure here is expected to happen sooner or
/// later and none of them panic.
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("could not read catalog at {path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    /// The bytes are not a valid archive. Usually a truncated download.
    #[error("catalog is corrupt or not a catalog archive: {0}")]
    Corrupt(String),

    /// The artifact was written by a different version of the format.
    #[error("catalog format version {found} cannot be read by this build (expects {expected}); download the matching artifact")]
    VersionMismatch { expected: u32, found: u32 },

    #[error("could not serialize catalog: {0}")]
    Serialize(String),
}

pub type Result<T> = std::result::Result<T, CatalogError>;
