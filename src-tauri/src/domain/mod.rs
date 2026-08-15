mod artifact;
mod download;
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
pub use download::{DownloadEntry, DownloadListRequest, DownloadPage, DownloadReviewKind};
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
