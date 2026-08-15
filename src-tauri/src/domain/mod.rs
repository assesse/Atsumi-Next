mod artifact;
mod auto_find;
mod download;
mod duplicate;
mod gallery;
mod job;
mod search;
mod settings;
mod window_placement;

pub use artifact::{
    ArtifactBundle, ArtifactConversionPolicy, ArtifactManifest, ArtifactManifestGallery,
    ArtifactManifestPage, ArtifactRelativePath, ArtifactSha256, ArtifactStorageFormat,
    DownloadArtifact, DownloadArtifactState, DownloadEntryId, PageArtifact, PageArtifactState,
    ARTIFACT_MANIFEST_SCHEMA_VERSION, HASH_PROFILE_VERSION,
};
pub use auto_find::{
    AutoFindCandidate, AutoFindCandidateRecord, AutoFindExclusionResult, AutoFindRun,
    AutoFindRunState, AutoFindSnapshot, FavoriteKey, FavoriteMutationResult, FavoriteNamespace,
    FavoriteRecord, SearchHistoryEntry,
};
pub use download::{DownloadEntry, DownloadListRequest, DownloadPage, DownloadReviewKind};
pub use duplicate::{
    DuplicateCandidate, DuplicateCandidateRecord, DuplicateDecisionAction,
    DuplicateDecisionApplyOutcome, DuplicateDecisionHistory, DuplicateDecisionRequest,
    DuplicateEvidence, DuplicateEvidenceKind, DuplicateGalleryRef, DuplicatePageHash,
    DuplicatePagePair, DuplicateRelation, DuplicateReview, DuplicateScanRun, DuplicateScanState,
    DuplicateSnapshot, ExternalRelationEvidence, HashProfile, SeriesGroup,
    DUPLICATE_HASH_ALGORITHM_VERSION, DUPLICATE_HASH_PROFILE_VERSION,
};
pub use gallery::{Gallery, GalleryId, GalleryMetadata, GalleryPageId, SourcePageNumber};
pub use job::{
    DownloadChangedEvent, DownloadJobDescriptor, DownloadJobProjection, FixtureDownloadJobStep,
    JobEvent, JobRef, JobState,
};
pub use search::{
    GalleryDetail, GalleryPage, GallerySummary, Language, SearchRequest, SearchSort,
    SearchSubmission,
};
pub use settings::{SettingsPatch, SettingsSnapshot};
pub use window_placement::{WindowPlacement, WindowPlacementSnapshot};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("invalid {field}: {message}")]
pub struct ValidationError {
    pub field: &'static str,
    pub message: &'static str,
}

impl ValidationError {
    pub const fn new(field: &'static str, message: &'static str) -> Self {
        Self { field, message }
    }
}
