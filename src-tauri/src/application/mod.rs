mod auto_find_supervisor;
mod download_pipeline;
mod download_supervisor;
mod duplicate_analyzer;
mod duplicate_supervisor;
mod error;
mod ports;
mod service;

pub use auto_find_supervisor::AutoFindSupervisor;
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
pub use ports::{
    ArtifactRepository, AutomationRepository, DownloadMutationOutcome, DownloadQueueAddOutcome,
    DownloadQueueRecord, DownloadRepository, DuplicateRelationProvider, DuplicateRepository,
    SearchRepository, StateRepository,
};
pub use service::{ApplicationService, DownloadQueueLaunch};
