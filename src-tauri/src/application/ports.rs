use crate::domain::{
    ArtifactBundle, AutoFindCandidateRecord, AutoFindExclusionResult, AutoFindRun,
    AutoFindRunState, AutoFindSnapshot, DownloadEntry, DownloadEntryId, DownloadJobDescriptor,
    DownloadJobProjection, DownloadListRequest, DownloadPage, FavoriteKey, FavoriteMutationResult,
    FavoriteRecord, FixtureDownloadJobStep, GalleryDetail, GalleryId, GalleryPage, JobRef,
    JobState, SearchHistoryEntry, SearchRequest, SearchSubmission, SettingsSnapshot,
    WindowPlacementSnapshot,
};

use super::RepositoryError;

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadQueueRecord {
    pub entries: Vec<DownloadEntry>,
    pub jobs: Vec<DownloadJobDescriptor>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadQueueAddOutcome {
    Added(DownloadQueueRecord),
    IdempotencyConflict,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadMutationOutcome<T> {
    Applied(T),
    EntryNotFound(DownloadEntryId),
    InvalidState {
        entry_id: DownloadEntryId,
        state: JobState,
    },
}

pub trait StateRepository: Send + Sync {
    fn settings_get(&self) -> Result<SettingsSnapshot, RepositoryError>;

    fn settings_compare_and_set(
        &self,
        next: &SettingsSnapshot,
        expected_revision: u64,
    ) -> Result<bool, RepositoryError>;

    fn window_placement_get(&self) -> Result<WindowPlacementSnapshot, RepositoryError>;

    fn window_placement_compare_and_set(
        &self,
        next: &WindowPlacementSnapshot,
        expected_revision: u64,
    ) -> Result<bool, RepositoryError>;
}

pub trait ArtifactRepository: Send + Sync {
    fn artifact_bundle_replace(&self, bundle: &ArtifactBundle) -> Result<(), RepositoryError>;

    fn artifact_bundle_get(
        &self,
        entry_id: &DownloadEntryId,
    ) -> Result<Option<ArtifactBundle>, RepositoryError>;
}

pub trait SearchRepository: Send + Sync {
    fn search_submit(&self, request: &SearchRequest) -> Result<SearchSubmission, RepositoryError>;

    fn search_page_get(
        &self,
        query_id: &str,
        page: u32,
    ) -> Result<Option<GalleryPage>, RepositoryError>;

    fn gallery_detail_get(
        &self,
        gallery_id: GalleryId,
    ) -> Result<Option<GalleryDetail>, RepositoryError>;
}

pub trait AutomationRepository: Send + Sync {
    fn favorites_list(&self) -> Result<Vec<FavoriteRecord>, RepositoryError>;

    fn favorite_set(
        &self,
        key: &FavoriteKey,
        enabled: bool,
    ) -> Result<FavoriteMutationResult, RepositoryError>;

    fn search_history_record(
        &self,
        request: &SearchRequest,
    ) -> Result<SearchHistoryEntry, RepositoryError>;

    fn search_history_list(&self, limit: u32) -> Result<Vec<SearchHistoryEntry>, RepositoryError>;

    fn auto_find_recover_interrupted(&self) -> Result<usize, RepositoryError>;

    fn auto_find_start(&self, total_favorites: u32) -> Result<AutoFindRun, RepositoryError>;

    fn auto_find_candidate_add(
        &self,
        candidate: &AutoFindCandidateRecord,
    ) -> Result<Option<AutoFindRun>, RepositoryError>;

    fn auto_find_progress(
        &self,
        run_id: &str,
        completed_favorites: u32,
    ) -> Result<Option<AutoFindRun>, RepositoryError>;

    fn auto_find_finish(
        &self,
        run_id: &str,
        state: AutoFindRunState,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<Option<AutoFindRun>, RepositoryError>;

    fn auto_find_is_running(&self, run_id: &str) -> Result<bool, RepositoryError>;

    fn auto_find_snapshot(&self) -> Result<AutoFindSnapshot, RepositoryError>;

    fn auto_find_exclude(
        &self,
        gallery_ids: &[GalleryId],
        reason: &str,
    ) -> Result<AutoFindExclusionResult, RepositoryError>;
}

pub trait DownloadRepository: Send + Sync {
    fn download_recover_interrupted(&self) -> Result<usize, RepositoryError>;

    fn download_queue_add(
        &self,
        request_id: &str,
        galleries: &[GalleryId],
    ) -> Result<DownloadQueueAddOutcome, RepositoryError>;

    fn download_entries_list(
        &self,
        request: &DownloadListRequest,
    ) -> Result<DownloadPage, RepositoryError>;

    fn download_active_count(&self) -> Result<u64, RepositoryError>;

    fn download_retry(
        &self,
        entry_ids: &[DownloadEntryId],
    ) -> Result<DownloadMutationOutcome<Vec<JobRef>>, RepositoryError>;

    fn download_cancel(
        &self,
        entry_ids: &[DownloadEntryId],
    ) -> Result<DownloadMutationOutcome<Vec<DownloadEntry>>, RepositoryError>;

    fn fixture_download_job_advance(
        &self,
        job_id: &str,
        worker_attempt: u64,
        step: FixtureDownloadJobStep,
    ) -> Result<DownloadJobProjection, RepositoryError>;
}
