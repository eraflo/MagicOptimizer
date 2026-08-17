//! Errors from loading the combo database.

/// Something went wrong opening or reading a combo artifact.
///
/// The artifact is an optional download, so every one of these is expected to happen and none
/// of them panic.
#[derive(Debug, thiserror::Error)]
pub enum ComboError {
    #[error("could not read the combo database at {path}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("the combo database is corrupt or not a combo archive: {0}")]
    Corrupt(String),

    #[error("combo database format version {found} cannot be read by this build (expects {expected}); download the matching artifact")]
    VersionMismatch { expected: u32, found: u32 },

    #[error("could not serialize the combo database: {0}")]
    Serialize(String),
}

pub type Result<T> = std::result::Result<T, ComboError>;
