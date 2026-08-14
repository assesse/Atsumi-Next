use thiserror::Error;

use crate::domain::{DownloadEntryId, GalleryId, JobState, ValidationError};

#[derive(Debug, Error)]
pub enum RepositoryError {
    #[error("database is busy: {0}")]
    Busy(String),
    #[error("database is corrupt: {0}")]
    Corrupt(String),
    #[error("database operation failed: {0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error(transparent)]
    Validation(#[from] ValidationError),
    #[error("{resource} revision conflict: expected {expected}, actual {actual}")]
    RevisionConflict {
        resource: &'static str,
        expected: u64,
        actual: u64,
    },
    #[error("search query {0:?} is not available")]
    QueryNotFound(String),
    #[error("gallery {0} was not found")]
    GalleryNotFound(GalleryId),
    #[error("request ID {request_id:?} was already used for a different gallery batch")]
    IdempotencyConflict { request_id: String },
    #[error("download entry {0} was not found")]
    DownloadEntryNotFound(DownloadEntryId),
    #[error("download entry {entry_id} cannot {operation} from {state}")]
    InvalidDownloadState {
        entry_id: DownloadEntryId,
        state: JobState,
        operation: &'static str,
    },
    #[error(transparent)]
    Repository(#[from] RepositoryError),
}
