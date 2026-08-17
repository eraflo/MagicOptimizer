//! Errors from the collection store.

use crate::model::HoldingId;

/// Something went wrong reading or writing a collection.
#[derive(Debug, thiserror::Error)]
pub enum CollectionError {
    /// A failure from the underlying database.
    ///
    /// redb distinguishes a dozen error types by operation. They are flattened to a message
    /// here because nothing in this crate can act differently on them — every one of them means
    /// the write did not happen, and the caller's only recourse is to surface it.
    #[error("collection database error: {0}")]
    Database(String),

    #[error("could not encode or decode a holding: {0}")]
    Encoding(String),

    #[error("no holding with id {0}")]
    NotFound(HoldingId),

    /// Guarded rather than silently ignored: a zero-quantity add is a caller bug, and letting
    /// it through would leave an invisible empty row behind.
    #[error("cannot add zero copies; use remove to delete a holding")]
    ZeroQuantity,
}

pub type Result<T> = std::result::Result<T, CollectionError>;
