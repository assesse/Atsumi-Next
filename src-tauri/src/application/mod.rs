mod auto_find_supervisor;
mod classic_import;
mod download_pipeline;
mod download_supervisor;
mod duplicate_analyzer;
mod duplicate_supervisor;
mod error;
mod internal_duplicate_analyzer;
mod internal_duplicate_supervisor;
mod ports;
mod service;

pub use auto_find_supervisor::AutoFindSupervisor;
pub use classic_import::{ClassicImportService, ClassicSourceInspector, ClassicSourceInventory};
pub use download_pipeline::{
    ArtifactLayout, ArtifactStore, DownloadArtifactPlan, DownloadCheckpoint,
    DownloadGallerySnapshot, DownloadPageAttempt, DownloadPageAttemptOutcome,
    DownloadPageAttemptResult, DownloadPagePayload, DownloadPipelineError,
    DownloadPipelineErrorCode, DownloadPipelineRepository, DownloadPrepared, DownloadRootPicker,
    DownloadSourceImageFormat, DownloadSourcePage, DownloadSourcePort, ExistingPageVerification,
    QuarantineSaga, QuarantineSagaState, ReconcileIssue, ReconcileReport, StoredPage,
};
pub use download_supervisor::DownloadSupervisor;
pub use duplicate_supervisor::{DisabledDuplicateRelationProvider, DuplicateSupervisor};
pub use error::{ApplicationError, RepositoryError};
pub use internal_duplicate_supervisor::InternalDuplicateSupervisor;
pub use ports::{
    ArtifactRepository, AutomationRepository, ClassicArtifactCopy, ClassicImportRepository,
    ClassicImportTransitionOutcome, DownloadMutationOutcome, DownloadQueueAddOutcome,
    DownloadQueueRecord, DownloadRepository, DuplicateRelationProvider, DuplicateRepository,
    InternalDuplicateRepository, InternalPlanPrepareOutcome, SearchRepository, StateRepository,
};
pub use service::{ApplicationService, DownloadQueueLaunch};
