use std::{collections::BTreeSet, sync::Arc};

use crate::domain::{
    DownloadEntry, DownloadEntryId, DownloadJobProjection, DownloadListRequest, DownloadPage,
    FixtureDownloadJobDescriptor, FixtureDownloadJobStep, GalleryDetail, GalleryId, GalleryPage,
    JobRef, SearchRequest, SearchSubmission, SettingsPatch, SettingsSnapshot, ValidationError,
    WindowPlacement, WindowPlacementSnapshot,
};

use super::{
    ApplicationError, DownloadMutationOutcome, DownloadQueueAddOutcome, DownloadRepository,
    SearchRepository, StateRepository,
};

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadQueueLaunch {
    pub entries: Vec<DownloadEntry>,
    pub fixture_jobs: Vec<FixtureDownloadJobDescriptor>,
}

pub struct ApplicationService {
    repository: Arc<dyn StateRepository>,
    search_repository: Option<Arc<dyn SearchRepository>>,
    download_repository: Option<Arc<dyn DownloadRepository>>,
}

impl Clone for ApplicationService {
    fn clone(&self) -> Self {
        Self {
            repository: Arc::clone(&self.repository),
            search_repository: self.search_repository.as_ref().map(Arc::clone),
            download_repository: self.download_repository.as_ref().map(Arc::clone),
        }
    }
}

impl ApplicationService {
    pub fn new(repository: Arc<dyn StateRepository>) -> Self {
        Self {
            repository,
            search_repository: None,
            download_repository: None,
        }
    }

    pub fn with_search_repository(mut self, search_repository: Arc<dyn SearchRepository>) -> Self {
        self.search_repository = Some(search_repository);
        self
    }

    pub fn with_download_repository(
        mut self,
        download_repository: Arc<dyn DownloadRepository>,
    ) -> Self {
        self.download_repository = Some(download_repository);
        self
    }

    pub fn settings_get(&self) -> Result<SettingsSnapshot, ApplicationError> {
        self.repository.settings_get().map_err(Into::into)
    }

    pub fn settings_update(
        &self,
        patch: SettingsPatch,
        expected_revision: u64,
    ) -> Result<SettingsSnapshot, ApplicationError> {
        let current = self.repository.settings_get()?;
        ensure_revision("settings", expected_revision, current.revision)?;
        let next = current.apply_patch(patch)?;

        if self
            .repository
            .settings_compare_and_set(&next, expected_revision)?
        {
            return Ok(next);
        }

        let actual = self.repository.settings_get()?.revision;
        Err(revision_conflict("settings", expected_revision, actual))
    }

    pub fn window_placement_get(&self) -> Result<WindowPlacementSnapshot, ApplicationError> {
        self.repository.window_placement_get().map_err(Into::into)
    }

    pub fn window_placement_update(
        &self,
        placement: WindowPlacement,
        expected_revision: u64,
    ) -> Result<WindowPlacementSnapshot, ApplicationError> {
        let current = self.repository.window_placement_get()?;
        ensure_revision("windowPlacement", expected_revision, current.revision)?;
        let next = current.updated(placement)?;

        if self
            .repository
            .window_placement_compare_and_set(&next, expected_revision)?
        {
            return Ok(next);
        }

        let actual = self.repository.window_placement_get()?.revision;
        Err(revision_conflict(
            "windowPlacement",
            expected_revision,
            actual,
        ))
    }

    pub fn fixture_download_job_advance(
        &self,
        job_id: &str,
        worker_attempt: u64,
        step: FixtureDownloadJobStep,
    ) -> Result<DownloadJobProjection, ApplicationError> {
        self.download_repository()?
            .fixture_download_job_advance(job_id, worker_attempt, step)
            .map_err(Into::into)
    }

    pub fn download_recover_interrupted(&self) -> Result<usize, ApplicationError> {
        self.download_repository()?
            .download_recover_interrupted()
            .map_err(Into::into)
    }

    pub fn download_queue_add(
        &self,
        galleries: Vec<i64>,
        request_id: String,
    ) -> Result<DownloadQueueLaunch, ApplicationError> {
        validate_request_id(&request_id)?;
        if galleries.is_empty() {
            return Err(ValidationError::new("galleries", "must not be empty").into());
        }
        if galleries.len() > 200 {
            return Err(ValidationError::new("galleries", "must contain at most 200 IDs").into());
        }

        if galleries.iter().any(|gallery_id| *gallery_id <= 0) {
            return Err(
                ValidationError::new("galleries", "gallery IDs must be positive integers").into(),
            );
        }
        let mut galleries = galleries
            .into_iter()
            .map(GalleryId::new)
            .collect::<Result<Vec<_>, _>>()?;
        galleries.sort_unstable();
        galleries.dedup();

        match self
            .download_repository()?
            .download_queue_add(request_id.trim(), &galleries)?
        {
            DownloadQueueAddOutcome::Added(record) => Ok(DownloadQueueLaunch {
                entries: record.entries,
                fixture_jobs: record.fixture_jobs,
            }),
            DownloadQueueAddOutcome::IdempotencyConflict => {
                Err(ApplicationError::IdempotencyConflict {
                    request_id: request_id.trim().to_owned(),
                })
            }
        }
    }

    pub fn download_entries_list(
        &self,
        request: DownloadListRequest,
    ) -> Result<DownloadPage, ApplicationError> {
        let request = request.normalized()?;
        self.download_repository()?
            .download_entries_list(&request)
            .map_err(Into::into)
    }

    pub fn download_active_count(&self) -> Result<u64, ApplicationError> {
        self.download_repository()?
            .download_active_count()
            .map_err(Into::into)
    }

    pub fn download_retry(&self, entry_ids: Vec<String>) -> Result<Vec<JobRef>, ApplicationError> {
        let entry_ids = normalize_entry_ids(entry_ids)?;
        match self.download_repository()?.download_retry(&entry_ids)? {
            DownloadMutationOutcome::Applied(job_refs) => Ok(job_refs),
            DownloadMutationOutcome::EntryNotFound(entry_id) => {
                Err(ApplicationError::DownloadEntryNotFound(entry_id))
            }
            DownloadMutationOutcome::InvalidState { entry_id, state } => {
                Err(ApplicationError::InvalidDownloadState {
                    entry_id,
                    state,
                    operation: "retry",
                })
            }
        }
    }

    pub fn download_cancel(
        &self,
        entry_ids: Vec<String>,
    ) -> Result<Vec<DownloadEntry>, ApplicationError> {
        let entry_ids = normalize_entry_ids(entry_ids)?;
        match self.download_repository()?.download_cancel(&entry_ids)? {
            DownloadMutationOutcome::Applied(entries) => Ok(entries),
            DownloadMutationOutcome::EntryNotFound(entry_id) => {
                Err(ApplicationError::DownloadEntryNotFound(entry_id))
            }
            DownloadMutationOutcome::InvalidState { entry_id, state } => {
                Err(ApplicationError::InvalidDownloadState {
                    entry_id,
                    state,
                    operation: "cancel",
                })
            }
        }
    }

    pub fn search_submit(
        &self,
        request: SearchRequest,
    ) -> Result<SearchSubmission, ApplicationError> {
        let request = request.normalized()?;
        tracing::info!(
            operation_id = "search_submit",
            has_text = !request.text.is_empty(),
            include_tag_count = request.include_tags.len(),
            exclude_tag_count = request.exclude_tags.len(),
            language_count = request.languages.len(),
            sort = ?request.sort,
            page_size = request.page_size,
            "submitting source search query"
        );
        self.search_repository()?
            .search_submit(&request)
            .map_err(Into::into)
    }

    pub fn search_page_get(
        &self,
        query_id: String,
        page: u32,
    ) -> Result<GalleryPage, ApplicationError> {
        let query_id = query_id.trim();
        if query_id.is_empty() {
            return Err(ValidationError::new("queryId", "must not be empty").into());
        }
        if query_id.len() > 200 {
            return Err(ValidationError::new("queryId", "must be at most 200 bytes").into());
        }
        if page == 0 {
            return Err(ValidationError::new("page", "must be one-based").into());
        }

        let result = self
            .search_repository()?
            .search_page_get(query_id, page)?
            .ok_or_else(|| ApplicationError::QueryNotFound(query_id.to_owned()))?;

        let is_out_of_range = if result.total_pages == 0 {
            page != 1
        } else {
            page > result.total_pages
        };
        if is_out_of_range {
            return Err(
                ValidationError::new("page", "must not exceed the search result range").into(),
            );
        }

        Ok(result)
    }

    pub fn gallery_detail_get(&self, gallery_id: i64) -> Result<GalleryDetail, ApplicationError> {
        let gallery_id = GalleryId::new(gallery_id)?;
        self.search_repository()?
            .gallery_detail_get(gallery_id)?
            .ok_or(ApplicationError::GalleryNotFound(gallery_id))
    }

    fn search_repository(&self) -> Result<&dyn SearchRepository, ApplicationError> {
        self.search_repository.as_deref().ok_or_else(|| {
            super::RepositoryError::Other("search repository is not configured".into()).into()
        })
    }

    fn download_repository(&self) -> Result<&dyn DownloadRepository, ApplicationError> {
        self.download_repository.as_deref().ok_or_else(|| {
            super::RepositoryError::Other("download repository is not configured".into()).into()
        })
    }
}

fn ensure_revision(
    resource: &'static str,
    expected: u64,
    actual: u64,
) -> Result<(), ApplicationError> {
    if expected == actual {
        Ok(())
    } else {
        Err(revision_conflict(resource, expected, actual))
    }
}

fn revision_conflict(resource: &'static str, expected: u64, actual: u64) -> ApplicationError {
    ApplicationError::RevisionConflict {
        resource,
        expected,
        actual,
    }
}

fn validate_request_id(request_id: &str) -> Result<(), ValidationError> {
    let request_id = request_id.trim();
    if request_id.is_empty() {
        return Err(ValidationError::new("requestId", "must not be empty"));
    }
    if request_id.len() > 200 {
        return Err(ValidationError::new(
            "requestId",
            "must be at most 200 bytes",
        ));
    }
    Ok(())
}

fn normalize_entry_ids(values: Vec<String>) -> Result<Vec<DownloadEntryId>, ValidationError> {
    if values.is_empty() {
        return Err(ValidationError::new("entryIds", "must not be empty"));
    }
    if values.len() > 200 {
        return Err(ValidationError::new(
            "entryIds",
            "must contain at most 200 IDs",
        ));
    }

    let mut seen = BTreeSet::new();
    let mut normalized = Vec::with_capacity(values.len());
    for value in values {
        let entry_id = DownloadEntryId::new(value)?;
        if seen.insert(entry_id.clone()) {
            normalized.push(entry_id);
        }
    }
    Ok(normalized)
}
