mod artifact;
mod artifact_path;
mod auto_find;
mod classic_import;
mod download;
mod duplicate;
mod gallery;
mod internal_duplicate;
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
pub use artifact_path::{
    plan_artifact_relative_directory, validate_folder_name_template, DEFAULT_FOLDER_NAME_TEMPLATE,
    MAX_FOLDER_COMPONENT_UTF16, MAX_MANAGED_ABSOLUTE_PATH_UTF16,
};
pub use auto_find::{
    AutoFindCandidate, AutoFindCandidateRecord, AutoFindExclusionResult, AutoFindRun,
    AutoFindRunState, AutoFindSnapshot, FavoriteKey, FavoriteMutationResult, FavoriteNamespace,
    FavoriteRecord, SearchHistoryEntry,
};
pub use classic_import::{
    ClassicConflictCode, ClassicConflictSeverity, ClassicImportApplyRequest,
    ClassicImportApplyResult, ClassicImportConflict, ClassicImportCounts,
    ClassicImportDryRunRequest, ClassicImportGalleryPlan, ClassicImportPagePlan, ClassicImportPlan,
    ClassicImportReport, ClassicImportRollbackRequest, ClassicImportState,
    ClassicLegacyHashSummary, ClassicPairPlan, ClassicSeriesPlan, ClassicSourceRootKind,
    StoredClassicImport, CLASSIC_IMPORT_SCHEMA_VERSION,
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
pub use internal_duplicate::{
    InternalDuplicateGroup, InternalDuplicateReview, InternalDuplicateSnapshot,
    InternalGroupRecord, InternalMatchKind, InternalPageEvidence, InternalRemovalApplyRequest,
    InternalRemovalPlan, InternalRemovalPlanRequest, InternalRemovalResult,
    InternalRemovalSelection, InternalRemovalUndoRequest, InternalScanRun, InternalScanState,
    PageQuarantineRecord, PageQuarantineSaga, PageQuarantineState,
};
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
