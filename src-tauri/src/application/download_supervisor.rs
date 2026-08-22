use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
        Arc, Condvar, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

use crate::{
    domain::{
        plan_artifact_relative_directory, ArtifactManifest, ArtifactRelativePath,
        ArtifactStorageFormat, DownloadArtifactState, DownloadEntry, DownloadEntryId,
        DownloadJobDescriptor, DownloadJobProjection, JobRef, JobState, PageArtifactState,
        ARTIFACT_MANIFEST_SCHEMA_VERSION, HASH_PROFILE_VERSION,
    },
    source::{SourceCandidateDiagnostic, SourceContractError, SourceErrorCode},
    thumbnail::CancellationToken,
};

use super::{
    ApplicationError, ArtifactLayout, ArtifactStore, DownloadArtifactPlan, DownloadPageAttempt,
    DownloadPageAttemptOutcome, DownloadPageAttemptResult, DownloadPipelineError,
    DownloadPipelineErrorCode, DownloadPipelineRepository, DownloadSourcePort,
    ExistingPageVerification, QuarantineSaga, QuarantineSagaState, ReconcileIssue, ReconcileReport,
    RepositoryError, StateRepository, StoredPage,
};

#[derive(Clone)]
pub struct DownloadSupervisor {
    inner: Arc<SupervisorInner>,
}

struct SupervisorInner {
    queue: Mutex<QueueState>,
    wake: Condvar,
    cancellations: Mutex<HashMap<String, ActiveCancellation>>,
    repository: Arc<dyn DownloadPipelineRepository>,
    settings: Arc<dyn StateRepository>,
    source: Arc<dyn DownloadSourcePort>,
    store: Arc<dyn ArtifactStore>,
    events: Sender<DownloadJobProjection>,
    workers: Mutex<Vec<JoinHandle<()>>>,
    shutting_down: AtomicBool,
}

struct QueueState {
    pending: VecDeque<DownloadJobDescriptor>,
    known: HashSet<String>,
    closed: bool,
}

struct ActiveCancellation {
    entry_id: String,
    token: CancellationToken,
}

const MAX_FULL_IMAGE_MEMORY_SLOTS: usize = 2;

impl DownloadSupervisor {
    pub fn new(
        repository: Arc<dyn DownloadPipelineRepository>,
        settings: Arc<dyn StateRepository>,
        source: Arc<dyn DownloadSourcePort>,
        store: Arc<dyn ArtifactStore>,
        events: Sender<DownloadJobProjection>,
        gallery_worker_count: usize,
    ) -> Result<Self, DownloadPipelineError> {
        if !(1..=8).contains(&gallery_worker_count) {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::WorkerUnavailable,
                "The gallery worker count must be between 1 and 8",
                false,
            ));
        }
        let inner = Arc::new(SupervisorInner {
            queue: Mutex::new(QueueState {
                pending: VecDeque::new(),
                known: HashSet::new(),
                closed: false,
            }),
            wake: Condvar::new(),
            cancellations: Mutex::new(HashMap::new()),
            repository,
            settings,
            source,
            store,
            events,
            workers: Mutex::new(Vec::new()),
            shutting_down: AtomicBool::new(false),
        });
        let supervisor = Self {
            inner: Arc::clone(&inner),
        };
        let mut workers = unpoison(inner.workers.lock());
        for index in 0..gallery_worker_count.min(MAX_FULL_IMAGE_MEMORY_SLOTS) {
            let worker_inner = Arc::clone(&inner);
            let handle = thread::Builder::new()
                .name(format!("atsumi-download-{index}"))
                .spawn(move || worker_loop(worker_inner))
                .map_err(|_| {
                    DownloadPipelineError::new(
                        DownloadPipelineErrorCode::WorkerUnavailable,
                        "A download worker thread could not be started",
                        true,
                    )
                })?;
            workers.push(handle);
        }
        drop(workers);
        Ok(supervisor)
    }

    pub fn enqueue(
        &self,
        descriptor: DownloadJobDescriptor,
    ) -> Result<bool, DownloadPipelineError> {
        let key = descriptor_key(&descriptor);
        let mut queue = unpoison(self.inner.queue.lock());
        if queue.closed || self.inner.shutting_down.load(Ordering::Acquire) {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::WorkerUnavailable,
                "The download worker is shutting down",
                true,
            ));
        }
        if !queue.known.insert(key) {
            return Ok(false);
        }
        queue.pending.push_back(descriptor);
        self.inner.wake.notify_one();
        Ok(true)
    }

    pub fn enqueue_all(
        &self,
        descriptors: impl IntoIterator<Item = DownloadJobDescriptor>,
    ) -> Result<usize, DownloadPipelineError> {
        let mut added = 0;
        for descriptor in descriptors {
            added += usize::from(self.enqueue(descriptor)?);
        }
        Ok(added)
    }

    pub fn cancel_entries(&self, entry_ids: &[String]) -> usize {
        let entry_ids = entry_ids.iter().collect::<HashSet<_>>();
        let cancellations = unpoison(self.inner.cancellations.lock());
        let mut cancelled = 0;
        for active in cancellations.values() {
            if entry_ids.contains(&active.entry_id) {
                active.token.cancel();
                cancelled += 1;
            }
        }
        cancelled
    }

    pub fn resume_interrupted(&self) -> Result<usize, RepositoryError> {
        let jobs = self.inner.repository.pipeline_resume_interrupted()?;
        self.enqueue_all(jobs).map_err(|error| {
            RepositoryError::Other(format!("could not resume interrupted jobs: {error}"))
        })
    }

    pub fn enqueue_retries(&self, jobs: &[JobRef]) -> Result<usize, RepositoryError> {
        let descriptors = self.inner.repository.pipeline_descriptors_for_jobs(jobs)?;
        self.enqueue_all(descriptors).map_err(|error| {
            RepositoryError::Other(format!("could not launch retried jobs: {error}"))
        })
    }

    pub fn open_first(&self, entry_id: String) -> Result<(), ApplicationError> {
        let entry_id = DownloadEntryId::new(entry_id)?;
        let bundle = self
            .inner
            .repository
            .pipeline_artifact_bundle(&entry_id)?
            .ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ArtifactMissing,
                    "The completed download has no artifact record",
                    false,
                )
            })?;
        let root = self.inner.repository.pipeline_artifact_root(&entry_id)?;
        let path = self.inner.store.first_verified_page_path(&root, &bundle)?;
        self.inner.store.open_with_default_viewer(&path)?;
        Ok(())
    }

    pub fn quarantine_entries(
        &self,
        entry_ids: Vec<String>,
        reason: String,
    ) -> Result<Vec<DownloadEntry>, ApplicationError> {
        let reason = reason.trim();
        if reason.is_empty() || reason.len() > 500 {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::QuarantineConflict,
                "A quarantine reason between 1 and 500 bytes is required",
                false,
            )
            .into());
        }
        let mut unique = BTreeMap::new();
        for raw in entry_ids {
            let entry_id = DownloadEntryId::new(raw)?;
            unique.insert(entry_id.to_string(), entry_id);
        }
        let mut entries = Vec::with_capacity(unique.len());
        for entry_id in unique.into_values() {
            let root = self.inner.store.validate_download_root(
                &self.inner.repository.pipeline_artifact_root(&entry_id)?,
            )?;
            let bundle = self
                .inner
                .repository
                .pipeline_artifact_bundle(&entry_id)?
                .ok_or_else(|| {
                    DownloadPipelineError::new(
                        DownloadPipelineErrorCode::ArtifactMissing,
                        "The download has no verified artifact to quarantine",
                        false,
                    )
                })?;
            if bundle.artifact.state != DownloadArtifactState::Complete {
                return Err(DownloadPipelineError::new(
                    DownloadPipelineErrorCode::QuarantineConflict,
                    "Only a verified complete artifact can be quarantined",
                    false,
                )
                .into());
            }
            let original_layout = layout_for_bundle(root.clone(), &bundle)?;
            let expected_manifest = ArtifactManifest::from_bundle(&bundle)?;
            match self.inner.store.read_manifest(&original_layout)? {
                Some(actual) if actual == expected_manifest => {}
                _ => {
                    return Err(DownloadPipelineError::new(
                        DownloadPipelineErrorCode::ManifestInvalid,
                        "The artifact manifest must be verified before quarantine",
                        false,
                    )
                    .into())
                }
            }
            let record_id = Uuid::new_v4().to_string();
            let quarantine_relative_path = ArtifactRelativePath::new(format!(
                ".atsumi-quarantine/{record_id}/{}",
                bundle.artifact.relative_directory.as_str()
            ))?;
            let saga = QuarantineSaga {
                record_id,
                entry_id: entry_id.clone(),
                original_relative_path: bundle.artifact.relative_directory.clone(),
                quarantine_relative_path,
                reason: reason.to_owned(),
                state: QuarantineSagaState::PendingQuarantine,
            };
            self.inner.repository.pipeline_quarantine_begin(&saga)?;
            self.inner.store.move_managed_directory(
                &root,
                &saga.original_relative_path,
                &saga.quarantine_relative_path,
            )?;
            write_quarantine_manifest(
                &self.inner,
                &root,
                &saga.quarantine_relative_path,
                &saga.quarantine_relative_path,
                expected_manifest,
                true,
            )?;
            let projection = self
                .inner
                .repository
                .pipeline_quarantine_complete(&saga.record_id)?;
            entries.push(download_entry_from_projection(&projection)?);
            emit(&self.inner, projection);
        }
        Ok(entries)
    }

    pub fn restore_entries(
        &self,
        entry_ids: Vec<String>,
    ) -> Result<Vec<DownloadEntry>, ApplicationError> {
        let mut unique = BTreeMap::new();
        for raw in entry_ids {
            let entry_id = DownloadEntryId::new(raw)?;
            unique.insert(entry_id.to_string(), entry_id);
        }
        let mut entries = Vec::with_capacity(unique.len());
        for entry_id in unique.into_values() {
            let saga = self.inner.repository.pipeline_restore_begin(&entry_id)?;
            let root = self.inner.store.validate_download_root(
                &self.inner.repository.pipeline_artifact_root(&entry_id)?,
            )?;
            let quarantine_layout =
                layout_for_directory(root.clone(), saga.quarantine_relative_path.clone())?;
            let manifest = self
                .inner
                .store
                .read_manifest(&quarantine_layout)?
                .ok_or_else(|| {
                    DownloadPipelineError::new(
                        DownloadPipelineErrorCode::ManifestInvalid,
                        "The quarantined artifact manifest is missing",
                        false,
                    )
                })?;
            write_quarantine_manifest(
                &self.inner,
                &root,
                &saga.quarantine_relative_path,
                &saga.original_relative_path,
                manifest,
                false,
            )?;
            self.inner.store.move_managed_directory(
                &root,
                &saga.quarantine_relative_path,
                &saga.original_relative_path,
            )?;
            let projection = self
                .inner
                .repository
                .pipeline_restore_complete(&saga.record_id)?;
            entries.push(download_entry_from_projection(&projection)?);
            emit(&self.inner, projection);
        }
        Ok(entries)
    }

    pub fn reconcile(&self) -> Result<ReconcileReport, ApplicationError> {
        let mut report = ReconcileReport {
            inspected_artifacts: 0,
            verified_artifacts: 0,
            resumed_jobs: 0,
            issues: Vec::new(),
        };
        self.reconcile_quarantine_sagas(&mut report)?;
        let bundles = self.inner.repository.pipeline_artifact_bundles()?;
        report.inspected_artifacts = u64::try_from(bundles.len()).unwrap_or(u64::MAX);
        for bundle in bundles {
            if bundle.artifact.state == DownloadArtifactState::Quarantined {
                continue;
            }
            let root = self.inner.store.validate_download_root(
                &self
                    .inner
                    .repository
                    .pipeline_artifact_root(&bundle.artifact.entry_id)?,
            )?;
            let mut artifact_issues = inspect_bundle(&self.inner, &root, &bundle);
            artifact_issues.sort_unstable();
            artifact_issues.dedup();
            if artifact_issues.is_empty() {
                if bundle.artifact.state == DownloadArtifactState::Complete {
                    report.verified_artifacts = report.verified_artifacts.saturating_add(1);
                }
                continue;
            }
            for (code, message) in artifact_issues {
                report.issues.push(ReconcileIssue {
                    entry_id: bundle.artifact.entry_id.to_string(),
                    code: code.to_owned(),
                    message: message.to_owned(),
                    recoverable: true,
                });
                if let Some(projection) = self.inner.repository.pipeline_mark_artifact_issue(
                    &bundle.artifact.entry_id,
                    &code,
                    &message,
                )? {
                    emit(&self.inner, projection);
                }
            }
        }
        report.resumed_jobs = u64::try_from(self.resume_interrupted()?).unwrap_or(u64::MAX);
        Ok(report)
    }

    /// Performs only the recovery work that must happen before downloads can
    /// resume. Full artifact hash/decode verification stays behind the
    /// explicit `app_reconcile` command so opening the application does not
    /// scale with the user's completed library.
    pub fn recover_startup_state(&self) -> Result<ReconcileReport, ApplicationError> {
        let mut report = ReconcileReport {
            inspected_artifacts: 0,
            verified_artifacts: 0,
            resumed_jobs: 0,
            issues: Vec::new(),
        };
        self.reconcile_quarantine_sagas(&mut report)?;
        report.resumed_jobs = u64::try_from(self.resume_interrupted()?).unwrap_or(u64::MAX);
        Ok(report)
    }

    fn reconcile_quarantine_sagas(
        &self,
        report: &mut ReconcileReport,
    ) -> Result<(), ApplicationError> {
        for saga in self.inner.repository.pipeline_pending_quarantine_sagas()? {
            let root = self.inner.store.validate_download_root(
                &self
                    .inner
                    .repository
                    .pipeline_artifact_root(&saga.entry_id)?,
            )?;
            match self.reconcile_quarantine_saga(&root, &saga) {
                Ok(projection) => emit(&self.inner, projection),
                Err(error) => {
                    let (code, message) = stable_application_issue(&error);
                    report.issues.push(ReconcileIssue {
                        entry_id: saga.entry_id.to_string(),
                        code: code.to_owned(),
                        message: message.to_owned(),
                        recoverable: true,
                    });
                }
            }
        }
        Ok(())
    }

    fn reconcile_quarantine_saga(
        &self,
        root: &std::path::Path,
        saga: &QuarantineSaga,
    ) -> Result<DownloadJobProjection, ApplicationError> {
        let original_exists = self
            .inner
            .store
            .managed_path_exists(root, &saga.original_relative_path)?;
        let quarantine_exists = self
            .inner
            .store
            .managed_path_exists(root, &saga.quarantine_relative_path)?;
        match saga.state {
            QuarantineSagaState::PendingQuarantine => {
                match (original_exists, quarantine_exists) {
                    (true, false) => self.inner.store.move_managed_directory(
                        root,
                        &saga.original_relative_path,
                        &saga.quarantine_relative_path,
                    )?,
                    (false, true) => {}
                    _ => return Err(quarantine_path_conflict().into()),
                }
                let bundle = self
                    .inner
                    .repository
                    .pipeline_artifact_bundle(&saga.entry_id)?
                    .ok_or_else(|| {
                        DownloadPipelineError::new(
                            DownloadPipelineErrorCode::ArtifactMissing,
                            "The pending quarantine artifact no longer exists",
                            false,
                        )
                    })?;
                let manifest = ArtifactManifest::from_bundle(&bundle)?;
                write_quarantine_manifest(
                    &self.inner,
                    root,
                    &saga.quarantine_relative_path,
                    &saga.quarantine_relative_path,
                    manifest,
                    true,
                )?;
                Ok(self
                    .inner
                    .repository
                    .pipeline_quarantine_complete(&saga.record_id)?)
            }
            QuarantineSagaState::PendingRestore => {
                match (original_exists, quarantine_exists) {
                    (false, true) => {
                        let layout = layout_for_directory(
                            root.to_path_buf(),
                            saga.quarantine_relative_path.clone(),
                        )?;
                        let manifest =
                            self.inner.store.read_manifest(&layout)?.ok_or_else(|| {
                                DownloadPipelineError::new(
                                    DownloadPipelineErrorCode::ManifestInvalid,
                                    "The pending restore manifest is missing",
                                    false,
                                )
                            })?;
                        write_quarantine_manifest(
                            &self.inner,
                            root,
                            &saga.quarantine_relative_path,
                            &saga.original_relative_path,
                            manifest,
                            false,
                        )?;
                        self.inner.store.move_managed_directory(
                            root,
                            &saga.quarantine_relative_path,
                            &saga.original_relative_path,
                        )?;
                    }
                    (true, false) => {}
                    _ => return Err(quarantine_path_conflict().into()),
                }
                Ok(self
                    .inner
                    .repository
                    .pipeline_restore_complete(&saga.record_id)?)
            }
            QuarantineSagaState::Quarantined | QuarantineSagaState::Restored => {
                Err(quarantine_path_conflict().into())
            }
        }
    }

    pub fn shutdown_and_wait(&self) {
        if self.inner.shutting_down.swap(true, Ordering::AcqRel) {
            return;
        }
        {
            let mut queue = unpoison(self.inner.queue.lock());
            queue.closed = true;
            queue.pending.clear();
        }
        {
            let cancellations = unpoison(self.inner.cancellations.lock());
            for active in cancellations.values() {
                active.token.cancel();
            }
        }
        self.inner.wake.notify_all();
        let workers = {
            let mut workers = unpoison(self.inner.workers.lock());
            std::mem::take(&mut *workers)
        };
        for worker in workers {
            if worker.thread().id() != thread::current().id() {
                let _ = worker.join();
            }
        }
    }

    #[cfg(test)]
    pub fn is_shutting_down(&self) -> bool {
        self.inner.shutting_down.load(Ordering::Acquire)
    }
}

fn layout_for_bundle(
    root: PathBuf,
    bundle: &crate::domain::ArtifactBundle,
) -> Result<ArtifactLayout, ApplicationError> {
    let manifest_relative_path =
        bundle
            .artifact
            .manifest_relative_path
            .clone()
            .unwrap_or(ArtifactRelativePath::new(format!(
                "{}/manifest.json",
                bundle.artifact.relative_directory.as_str()
            ))?);
    Ok(ArtifactLayout {
        root,
        relative_directory: bundle.artifact.relative_directory.clone(),
        manifest_relative_path,
    })
}

fn layout_for_directory(
    root: PathBuf,
    relative_directory: ArtifactRelativePath,
) -> Result<ArtifactLayout, ApplicationError> {
    let manifest_relative_path =
        ArtifactRelativePath::new(format!("{}/manifest.json", relative_directory.as_str()))?;
    Ok(ArtifactLayout {
        root,
        relative_directory,
        manifest_relative_path,
    })
}

fn write_quarantine_manifest(
    inner: &SupervisorInner,
    root: &std::path::Path,
    storage_directory: &ArtifactRelativePath,
    target_page_directory: &ArtifactRelativePath,
    mut manifest: ArtifactManifest,
    quarantined: bool,
) -> Result<(), ApplicationError> {
    for page in &mut manifest.pages {
        let file_name = std::path::Path::new(&page.relative_path)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "A manifest page path is invalid",
                    false,
                )
            })?;
        page.relative_path = format!("{}/{file_name}", target_page_directory.as_str());
        page.quarantined = quarantined;
    }
    let layout = layout_for_directory(root.to_path_buf(), storage_directory.clone())?;
    inner.store.write_manifest(&layout, &manifest)?;
    Ok(())
}

fn download_entry_from_projection(
    projection: &DownloadJobProjection,
) -> Result<DownloadEntry, ApplicationError> {
    Ok(DownloadEntry {
        entry_id: DownloadEntryId::new(projection.download.entry_id.clone())?,
        gallery_id: crate::domain::GalleryId::new(projection.download.gallery_id)?,
        revision: projection.download.revision,
        state: projection.download.state,
        progress: projection.download.progress,
        attempt: projection.download.attempt,
        error_code: projection.download.error_code.clone(),
        error_message: projection.download.error_message.clone(),
        error_retryable: None,
        review_kind: None,
        review_id: None,
    })
}

fn inspect_bundle(
    inner: &SupervisorInner,
    root: &std::path::Path,
    bundle: &crate::domain::ArtifactBundle,
) -> Vec<(String, String)> {
    let layout = match layout_for_bundle(root.to_path_buf(), bundle) {
        Ok(layout) => layout,
        Err(error) => return vec![stable_application_issue_owned(&error)],
    };
    let mut issues = Vec::new();
    for page in &bundle.pages {
        let expected = page_verification(page);
        if page.state == PageArtifactState::Present && expected.is_none() {
            issues.push((
                "ARTIFACT_MANIFEST_INVALID".into(),
                "A present page is missing verification metadata".into(),
            ));
            continue;
        }
        let Some(expected) = expected.as_ref() else {
            continue;
        };
        match inner.store.verify_existing_page(
            &layout,
            page.page_id.source_page_number,
            &expected.source_revision,
            Some(expected),
        ) {
            Ok(ExistingPageVerification::Verified(_)) => {}
            Ok(ExistingPageVerification::Missing) => issues.push((
                "FILESYSTEM_MISSING".into(),
                "A verified page is missing from disk".into(),
            )),
            Ok(ExistingPageVerification::Invalid { .. }) => issues.push((
                "ARTIFACT_HASH_MISMATCH".into(),
                "A page no longer matches its verified digest".into(),
            )),
            Err(error) => issues.push((error.code.as_str().into(), error.message)),
        }
    }
    if bundle.artifact.state == DownloadArtifactState::Complete {
        match ArtifactManifest::from_bundle(bundle) {
            Ok(expected_manifest) => match inner.store.read_manifest(&layout) {
                Ok(Some(actual)) if actual == expected_manifest => {}
                Ok(Some(_)) => issues.push((
                    "ARTIFACT_MANIFEST_INVALID".into(),
                    "The artifact manifest does not match the database snapshot".into(),
                )),
                Ok(None) => issues.push((
                    "FILESYSTEM_MISSING".into(),
                    "The completed artifact manifest is missing".into(),
                )),
                Err(error) => issues.push((error.code.as_str().into(), error.message)),
            },
            Err(_) => issues.push((
                "ARTIFACT_MANIFEST_INVALID".into(),
                "The database artifact cannot produce a valid manifest".into(),
            )),
        }
    }
    issues
}

fn stable_application_issue(error: &ApplicationError) -> (&str, &str) {
    match error {
        ApplicationError::DownloadPipeline(error) => (error.code.as_str(), &error.message),
        ApplicationError::Repository(_) => (
            "DATABASE_ERROR",
            "The recovery state could not be updated safely",
        ),
        _ => (
            "QUARANTINE_CONFLICT",
            "The quarantine operation could not be reconciled safely",
        ),
    }
}

fn stable_application_issue_owned(error: &ApplicationError) -> (String, String) {
    let (code, message) = stable_application_issue(error);
    (code.to_owned(), message.to_owned())
}

fn quarantine_path_conflict() -> DownloadPipelineError {
    DownloadPipelineError::new(
        DownloadPipelineErrorCode::QuarantineConflict,
        "The original and quarantine paths are in an ambiguous state; no file was deleted",
        false,
    )
}

fn worker_loop(inner: Arc<SupervisorInner>) {
    loop {
        let descriptor = {
            let mut queue = unpoison(inner.queue.lock());
            loop {
                if let Some(descriptor) = queue.pending.pop_front() {
                    break Some(descriptor);
                }
                if queue.closed {
                    break None;
                }
                queue = unpoison(inner.wake.wait(queue));
            }
        };
        let Some(descriptor) = descriptor else {
            break;
        };
        let key = descriptor_key(&descriptor);
        let cancellation = CancellationToken::new();
        unpoison(inner.cancellations.lock()).insert(
            key.clone(),
            ActiveCancellation {
                entry_id: descriptor.entry_id.clone(),
                token: cancellation.clone(),
            },
        );
        if let Err(error) = run_download(&inner, &descriptor, &cancellation) {
            handle_download_error(&inner, &descriptor, &cancellation, error);
        }
        unpoison(inner.cancellations.lock()).remove(&key);
        unpoison(inner.queue.lock()).known.remove(&key);
    }
}

fn run_download(
    inner: &SupervisorInner,
    descriptor: &DownloadJobDescriptor,
    cancellation: &CancellationToken,
) -> Result<(), RunError> {
    emit(inner, inner.repository.pipeline_begin(descriptor)?);
    check_cancelled(cancellation)?;
    let settings = inner.settings.settings_get()?;
    if settings.download_root.trim().is_empty() {
        return Err(DownloadPipelineError::root_required().into());
    }

    let snapshot = inner
        .source
        .gallery_snapshot(descriptor.gallery_id, cancellation)?;
    check_cancelled(cancellation)?;
    let root = PathBuf::from(settings.download_root);
    let planned_relative_directory =
        plan_artifact_relative_directory(&settings.folder_name_template, &snapshot.gallery)
            .map_err(|error| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::PathOutsideRoot,
                    format!("The configured artifact folder template is invalid: {error}"),
                    false,
                )
            })?;
    let planned_manifest_relative_path = ArtifactRelativePath::new(format!(
        "{}/manifest.json",
        planned_relative_directory.as_str()
    ))
    .map_err(|error| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::PathOutsideRoot,
            format!("The planned manifest path is invalid: {error}"),
            false,
        )
    })?;
    let prepared = inner.repository.pipeline_prepare(&DownloadArtifactPlan {
        descriptor: descriptor.clone(),
        gallery: snapshot.gallery,
        source_revision: snapshot.source_revision,
        root_snapshot: root.clone(),
        relative_directory: planned_relative_directory,
        manifest_relative_path: planned_manifest_relative_path,
        source_pages: snapshot.pages.clone(),
    })?;
    // A pre-existing row is the durable DB reservation for this immutable destination.
    // Files inside it are still verified against checkpoints or moved to recovery review.
    let allow_existing_directory = !prepared.artifact_created;
    let layout = inner.store.prepare_layout(
        &prepared.root_snapshot,
        &prepared.relative_directory,
        allow_existing_directory,
    )?;
    if layout.manifest_relative_path != prepared.manifest_relative_path {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ManifestInvalid,
            "The persisted artifact manifest path does not match its immutable directory",
            false,
        )
        .into());
    }
    emit(inner, prepared.projection);
    let checkpoints = prepared
        .checkpoints
        .into_iter()
        .map(|checkpoint| (checkpoint.page.source_page_number, checkpoint))
        .collect::<BTreeMap<_, _>>();

    for source_page in &snapshot.pages {
        check_cancelled(cancellation)?;
        let checkpoint = checkpoints.get(&source_page.source_page_number);
        let existing = inner.store.verify_existing_page(
            &layout,
            source_page.source_page_number,
            &source_page.source_revision,
            checkpoint.map(|checkpoint| &checkpoint.page),
        )?;
        match existing {
            ExistingPageVerification::Verified(page) => {
                if checkpoint.is_none() {
                    emit(
                        inner,
                        inner.repository.pipeline_page_verified(descriptor, &page)?,
                    );
                }
                continue;
            }
            ExistingPageVerification::Invalid { .. } => {
                if let Some(projection) = inner.repository.pipeline_mark_artifact_issue(
                    &DownloadEntryId::new(descriptor.entry_id.clone()).map_err(|_| {
                        DownloadPipelineError::new(
                            DownloadPipelineErrorCode::ManifestInvalid,
                            "The download entry identity is invalid",
                            false,
                        )
                    })?,
                    "RECOVERY_CONFLICT",
                    "Ambiguous page files were moved aside for review",
                )? {
                    emit(inner, projection);
                }
                return Err(DownloadPipelineError::new(
                    DownloadPipelineErrorCode::HashMismatch,
                    "A stored page does not match its verified checkpoint",
                    false,
                )
                .into());
            }
            ExistingPageVerification::Missing => {}
        }

        let payload = match inner.source.download_page(
            descriptor.gallery_id,
            source_page.source_page_number,
            cancellation,
        ) {
            Ok(payload) => payload,
            Err(error) => {
                let diagnostics = if error.candidate_diagnostics.is_empty() {
                    vec![SourceCandidateDiagnostic {
                        candidate_index: 0,
                        format: "unknown".into(),
                        http_status: error.http_status,
                        content_type: None,
                        bytes_received: None,
                        error_code: Some(error.code),
                        retryable: error.retryable,
                    }]
                } else {
                    error.candidate_diagnostics.clone()
                };
                persist_candidate_diagnostics(
                    inner,
                    descriptor,
                    source_page.source_page_number,
                    &diagnostics,
                )?;
                return Err(error.into());
            }
        };
        let diagnostics = if payload.candidate_diagnostics.is_empty() {
            vec![SourceCandidateDiagnostic {
                candidate_index: payload.candidate_index,
                format: payload.source_format.as_str().to_owned(),
                http_status: None,
                content_type: None,
                bytes_received: u64::try_from(payload.bytes.len()).ok(),
                error_code: None,
                retryable: false,
            }]
        } else {
            payload.candidate_diagnostics.clone()
        };
        persist_candidate_diagnostics(
            inner,
            descriptor,
            source_page.source_page_number,
            &diagnostics,
        )?;
        if payload.source_page_number != source_page.source_page_number
            || payload.source_revision != source_page.source_revision
        {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ManifestInvalid,
                "The downloaded page identity does not match the immutable source mapping",
                false,
            )
            .into());
        }
        let stored = inner.store.store_page(&layout, &payload, cancellation)?;
        emit(
            inner,
            inner
                .repository
                .pipeline_page_verified(descriptor, &stored)?,
        );
    }

    emit(
        inner,
        inner
            .repository
            .pipeline_stage(descriptor, JobState::Hashing, "Rechecking page hashes")?,
    );
    check_cancelled(cancellation)?;
    let mut bundle = inner
        .repository
        .pipeline_artifact_bundle(
            &DownloadEntryId::new(descriptor.entry_id.clone())
                .map_err(|error| RepositoryError::Other(error.to_string()))?,
        )?
        .ok_or_else(|| RepositoryError::Corrupt("prepared artifact is missing".into()))?;
    verify_bundle_files(inner, &layout, &bundle)?;

    emit(
        inner,
        inner.repository.pipeline_stage(
            descriptor,
            JobState::Verifying,
            "Writing and verifying the artifact manifest",
        )?,
    );
    check_cancelled(cancellation)?;
    let completed_at = now_unix_ms();
    bundle.artifact.state = DownloadArtifactState::Complete;
    bundle.artifact = bundle
        .artifact
        .with_manifest(
            layout.manifest_relative_path.clone(),
            ARTIFACT_MANIFEST_SCHEMA_VERSION,
            env!("CARGO_PKG_VERSION"),
            HASH_PROFILE_VERSION,
            completed_at,
        )
        .map_err(|error| RepositoryError::Other(error.to_string()))?;
    let manifest = ArtifactManifest::from_bundle(&bundle)
        .map_err(|error| RepositoryError::Other(error.to_string()))?;
    inner.store.write_manifest(&layout, &manifest)?;
    let persisted = inner.store.read_manifest(&layout)?.ok_or_else(|| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::ManifestInvalid,
            "The artifact manifest disappeared before completion",
            false,
        )
    })?;
    if persisted != manifest {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ManifestInvalid,
            "The persisted artifact manifest does not match the verified snapshot",
            false,
        )
        .into());
    }
    emit(
        inner,
        inner.repository.pipeline_complete(
            descriptor,
            &manifest,
            &layout.manifest_relative_path,
        )?,
    );
    Ok(())
}

fn verify_bundle_files(
    inner: &SupervisorInner,
    layout: &ArtifactLayout,
    bundle: &crate::domain::ArtifactBundle,
) -> Result<(), RunError> {
    if bundle.pages.len() != bundle.artifact.expected_page_count as usize {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ManifestInvalid,
            "The database page map does not match the expected page count",
            false,
        )
        .into());
    }
    for page in &bundle.pages {
        if page.state != PageArtifactState::Present || page.excluded {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ArtifactMissing,
                "A required artifact page is not present",
                false,
            )
            .into());
        }
        let expected = StoredPage {
            source_page_number: page.page_id.source_page_number,
            relative_path: page.relative_path.clone(),
            byte_length: page.byte_length.ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "A page checkpoint is missing its byte length",
                    false,
                )
            })?,
            sha256: page.sha256.clone().ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "A page checkpoint is missing its SHA-256 digest",
                    false,
                )
            })?,
            storage_format: page.storage_format.unwrap_or(ArtifactStorageFormat::Webp),
            source_revision: page.source_revision.clone().ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "A page checkpoint is missing its source revision",
                    false,
                )
            })?,
            verified_at: page.verified_at.clone().unwrap_or_default(),
        };
        match inner.store.verify_existing_page(
            layout,
            page.page_id.source_page_number,
            &expected.source_revision,
            Some(&expected),
        )? {
            ExistingPageVerification::Verified(_) => {}
            ExistingPageVerification::Missing => {
                return Err(DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ArtifactMissing,
                    "A verified artifact page is missing from disk",
                    false,
                )
                .into())
            }
            ExistingPageVerification::Invalid { .. } => {
                return Err(DownloadPipelineError::new(
                    DownloadPipelineErrorCode::HashMismatch,
                    "A verified artifact page no longer matches its digest",
                    false,
                )
                .into())
            }
        }
    }
    Ok(())
}

fn page_verification(page: &crate::domain::PageArtifact) -> Option<StoredPage> {
    Some(StoredPage {
        source_page_number: page.page_id.source_page_number,
        relative_path: page.relative_path.clone(),
        byte_length: page.byte_length?,
        sha256: page.sha256.clone()?,
        storage_format: page.storage_format?,
        source_revision: page.source_revision.clone()?,
        verified_at: page.verified_at.clone()?,
    })
}

fn handle_download_error(
    inner: &SupervisorInner,
    descriptor: &DownloadJobDescriptor,
    cancellation: &CancellationToken,
    error: RunError,
) {
    if cancellation.is_cancelled() {
        tracing::info!(
            job_id = descriptor.job_id,
            worker_attempt = descriptor.worker_attempt,
            "download worker stopped after cancellation"
        );
        return;
    }
    let (code, message, retryable) = error.stable();
    tracing::error!(
        job_id = descriptor.job_id,
        gallery_id = descriptor.gallery_id.get(),
        worker_attempt = descriptor.worker_attempt,
        error_code = code,
        retryable,
        "download worker stopped before verification"
    );
    match inner
        .repository
        .pipeline_fail(descriptor, code, message, retryable)
    {
        Ok(Some(projection)) => emit(inner, projection),
        Ok(None) => {}
        Err(repository_error) => tracing::error!(
            job_id = descriptor.job_id,
            error_code = repository_error.stable_code(),
            "download failure state could not be persisted"
        ),
    }
}

fn emit(inner: &SupervisorInner, projection: DownloadJobProjection) {
    if inner.events.send(projection).is_err() {
        tracing::warn!("download event receiver is no longer available");
    }
}

fn descriptor_key(descriptor: &DownloadJobDescriptor) -> String {
    format!("{}:{}", descriptor.job_id, descriptor.worker_attempt)
}

fn check_cancelled(cancellation: &CancellationToken) -> Result<(), RunError> {
    if cancellation.is_cancelled() {
        Err(DownloadPipelineError::cancelled().into())
    } else {
        Ok(())
    }
}

enum RunError {
    Repository(RepositoryError),
    Source(SourceContractError),
    Pipeline(DownloadPipelineError),
}

impl RunError {
    fn stable(&self) -> (&str, &str, bool) {
        match self {
            Self::Repository(error) => {
                let busy = matches!(error, RepositoryError::Busy(_));
                (
                    if busy {
                        "DATABASE_BUSY"
                    } else {
                        "DATABASE_ERROR"
                    },
                    "The download state could not be updated safely",
                    busy,
                )
            }
            Self::Source(error) => {
                let (code, message) = stable_source_failure(error);
                (code, message, error.retryable)
            }
            Self::Pipeline(error) => (error.code.as_str(), &error.message, error.retryable),
        }
    }
}

impl From<RepositoryError> for RunError {
    fn from(error: RepositoryError) -> Self {
        Self::Repository(error)
    }
}

impl From<SourceContractError> for RunError {
    fn from(error: SourceContractError) -> Self {
        Self::Source(error)
    }
}

impl From<DownloadPipelineError> for RunError {
    fn from(error: DownloadPipelineError) -> Self {
        Self::Pipeline(error)
    }
}

fn persist_candidate_diagnostics(
    inner: &SupervisorInner,
    descriptor: &DownloadJobDescriptor,
    source_page_number: crate::domain::SourcePageNumber,
    diagnostics: &[SourceCandidateDiagnostic],
) -> Result<(), RepositoryError> {
    for diagnostic in diagnostics {
        let attempt = DownloadPageAttempt {
            descriptor: descriptor.clone(),
            source_page_number,
            candidate_index: diagnostic.candidate_index,
            candidate_format: diagnostic.format.clone(),
        };
        inner.repository.pipeline_page_attempt_start(&attempt)?;
        let outcome = match diagnostic.error_code {
            None => DownloadPageAttemptOutcome::Succeeded,
            Some(SourceErrorCode::Cancelled) => DownloadPageAttemptOutcome::Cancelled,
            Some(_) => DownloadPageAttemptOutcome::Failed,
        };
        inner
            .repository
            .pipeline_page_attempt_finish(&DownloadPageAttemptResult {
                attempt,
                outcome,
                bytes_received: diagnostic.bytes_received,
                http_status: diagnostic.http_status,
                content_type: diagnostic.content_type.clone(),
                error_code: diagnostic.error_code.map(|code| code.as_str().to_owned()),
                error_message: None,
                retryable: diagnostic.retryable,
            })?;
    }
    Ok(())
}

fn stable_source_failure(error: &SourceContractError) -> (&'static str, &'static str) {
    match error.code {
        SourceErrorCode::Cancelled => ("REQUEST_CANCELLED", "The source request was cancelled"),
        SourceErrorCode::Validation => (
            "SOURCE_VALIDATION",
            "The source request did not pass validation",
        ),
        SourceErrorCode::NotFound => ("SOURCE_NOT_FOUND", "A required source page was not found"),
        SourceErrorCode::Protocol => (
            "SOURCE_PROTOCOL",
            "The source response did not match the supported protocol",
        ),
        SourceErrorCode::InvalidData => (
            "SOURCE_INVALID_DATA",
            "The source returned metadata that could not be read safely",
        ),
        SourceErrorCode::RateLimited => (
            "SOURCE_RATE_LIMITED",
            "The source is rate limiting page downloads",
        ),
        SourceErrorCode::TemporarilyUnavailable => (
            "SOURCE_TEMPORARILY_UNAVAILABLE",
            "The source is temporarily unavailable",
        ),
        SourceErrorCode::Timeout => (
            "SOURCE_TIMEOUT",
            "The source did not return the page in time",
        ),
        SourceErrorCode::Unauthorized => (
            "SOURCE_UNAUTHORIZED",
            "The source rejected the page request",
        ),
        SourceErrorCode::Transport => (
            "NETWORK_OFFLINE",
            "A connection to the source could not be established",
        ),
        SourceErrorCode::ImageCandidatesExhausted => (
            "IMAGE_CANDIDATES_EXHAUSTED",
            "All supported page image candidates were exhausted",
        ),
        SourceErrorCode::ImageResponseInvalid => (
            "IMAGE_RESPONSE_INVALID",
            "The source returned a response that is not a supported image",
        ),
        SourceErrorCode::ImageDecodeFailed => (
            "IMAGE_DECODE_FAILED",
            "The downloaded page could not be decoded safely",
        ),
        SourceErrorCode::ImageFormatUnsupported => (
            "IMAGE_FORMAT_UNSUPPORTED",
            "The downloaded page format is not supported safely",
        ),
    }
}

fn now_unix_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn unpoison<T>(result: std::sync::LockResult<MutexGuard<'_, T>>) -> MutexGuard<'_, T> {
    result.unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use std::{
        io::Cursor,
        sync::{mpsc, Mutex},
        time::{Duration, Instant},
    };

    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use tempfile::tempdir;

    use super::*;
    use crate::{
        application::{
            ApplicationService, DownloadGallerySnapshot, DownloadPagePayload,
            DownloadSourceImageFormat, DownloadSourcePage,
        },
        domain::{
            DownloadListRequest, Gallery, GalleryId, GalleryMetadata, SettingsPatch,
            SourcePageNumber,
        },
        infrastructure::{FilesystemArtifactStore, SqliteRepository},
    };

    struct FakeDownloadSource {
        pages: u32,
        block_page: Option<u32>,
        gallery_revision: u64,
        calls: Mutex<Vec<u32>>,
    }

    impl FakeDownloadSource {
        fn new(pages: u32, block_page: Option<u32>) -> Self {
            Self {
                pages,
                block_page,
                gallery_revision: 1,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn with_gallery_revision(mut self, gallery_revision: u64) -> Self {
            self.gallery_revision = gallery_revision;
            self
        }

        fn calls(&self) -> Vec<u32> {
            unpoison(self.calls.lock()).clone()
        }
    }

    impl DownloadSourcePort for FakeDownloadSource {
        fn gallery_snapshot(
            &self,
            gallery_id: GalleryId,
            _cancellation: &CancellationToken,
        ) -> Result<DownloadGallerySnapshot, SourceContractError> {
            let metadata = GalleryMetadata::new(
                "Synthetic download fixture",
                Some("fixture artist".into()),
                Some("fixture group".into()),
                self.pages,
            )
            .unwrap();
            let pages = (1..=self.pages)
                .map(|number| DownloadSourcePage {
                    source_page_number: SourcePageNumber::new(number).unwrap(),
                    source_revision: format!("fixture-page-v1:{number}"),
                })
                .collect();
            Ok(DownloadGallerySnapshot {
                gallery: Gallery::new(gallery_id, self.gallery_revision, metadata),
                source_revision: format!("fixture-gallery:{:016x}", self.gallery_revision),
                pages,
            })
        }

        fn download_page(
            &self,
            _gallery_id: GalleryId,
            source_page_number: SourcePageNumber,
            cancellation: &CancellationToken,
        ) -> Result<DownloadPagePayload, SourceContractError> {
            unpoison(self.calls.lock()).push(source_page_number.get());
            if self.block_page == Some(source_page_number.get()) {
                while !cancellation.is_cancelled() {
                    thread::sleep(Duration::from_millis(5));
                }
                return Err(SourceContractError::cancelled());
            }
            let color = u8::try_from(source_page_number.get()).unwrap_or(u8::MAX);
            let image =
                DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([color, 20, 30, 255])));
            let mut bytes = Cursor::new(Vec::new());
            image.write_to(&mut bytes, ImageFormat::Png).unwrap();
            Ok(DownloadPagePayload {
                source_page_number,
                bytes: bytes.into_inner(),
                source_revision: format!("fixture-page-v1:{}", source_page_number.get()),
                source_format: DownloadSourceImageFormat::Png,
                width: 2,
                height: 2,
                candidate_index: 0,
                candidate_diagnostics: Vec::new(),
            })
        }
    }

    fn configured_repository(
        directory: &std::path::Path,
    ) -> (Arc<SqliteRepository>, ApplicationService) {
        let repository = Arc::new(SqliteRepository::open(directory.join("state.sqlite3")).unwrap());
        let service = ApplicationService::new(repository.clone())
            .with_download_repository(repository.clone());
        let settings = service.settings_get().unwrap();
        service
            .settings_update(
                SettingsPatch {
                    download_root: Some(directory.join("downloads").to_string_lossy().into_owned()),
                    ..SettingsPatch::default()
                },
                settings.revision,
            )
            .unwrap();
        (repository, service)
    }

    fn launch(
        repository: &Arc<SqliteRepository>,
        source: Arc<dyn DownloadSourcePort>,
    ) -> (DownloadSupervisor, mpsc::Receiver<DownloadJobProjection>) {
        let (events, receiver) = mpsc::channel();
        let pipeline_repository: Arc<dyn DownloadPipelineRepository> = repository.clone();
        let settings_repository: Arc<dyn StateRepository> = repository.clone();
        let store: Arc<dyn ArtifactStore> = Arc::new(FilesystemArtifactStore::new());
        (
            DownloadSupervisor::new(
                pipeline_repository,
                settings_repository,
                source,
                store,
                events,
                1,
            )
            .unwrap(),
            receiver,
        )
    }

    fn wait_for_state(
        service: &ApplicationService,
        entry_id: &str,
        expected: JobState,
        minimum_progress: f64,
    ) -> crate::domain::DownloadEntry {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            let page = service
                .download_entries_list(DownloadListRequest {
                    state: None,
                    query: None,
                    page: 1,
                    page_size: 20,
                })
                .unwrap();
            if let Some(entry) = page.entries.into_iter().find(|entry| {
                entry.entry_id.as_str() == entry_id
                    && entry.state == expected
                    && entry.progress.unwrap_or_default() >= minimum_progress
            }) {
                return entry;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("download {entry_id} did not reach {expected}");
    }

    #[test]
    fn source_failures_are_redacted_and_stable() {
        let error = SourceContractError::image_response_invalid(
            "https://private.invalid/image?token=secret returned HTML",
        );
        let (code, message) = stable_source_failure(&error);
        assert_eq!(code, "IMAGE_RESPONSE_INVALID");
        assert!(!message.contains("private"));
        assert!(!message.contains("secret"));
    }

    #[test]
    fn retryability_is_persisted_for_job_attempt_and_list_projection() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let queued = service
            .download_queue_add(vec![41], "retryability-persistence".into())
            .unwrap();
        let descriptor = queued.jobs.into_iter().next().unwrap();
        repository.pipeline_begin(&descriptor).unwrap();
        repository
            .pipeline_fail(&descriptor, "NETWORK_OFFLINE", "Source unavailable", true)
            .unwrap();

        let entries = service
            .download_entries_list(crate::domain::DownloadListRequest {
                state: None,
                query: None,
                page: 1,
                page_size: 20,
            })
            .unwrap();
        assert_eq!(entries.entries[0].error_retryable, Some(true));
        let stored: i64 = rusqlite::Connection::open(root.join("state.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT error_retryable FROM download_attempts WHERE job_id = ?1 AND attempt = ?2",
                rusqlite::params![descriptor.job_id, descriptor.worker_attempt],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, 1);
    }

    #[test]
    fn real_pipeline_writes_verified_webp_manifest_before_completed() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let source = Arc::new(FakeDownloadSource::new(2, None));
        let (supervisor, _events) = launch(&repository, source.clone());
        let queued = service
            .download_queue_add(vec![42], "pipeline-complete".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        supervisor.enqueue_all(queued.jobs).unwrap();

        let completed = wait_for_state(&service, &entry_id, JobState::Completed, 100.0);
        assert_eq!(completed.attempt, Some(1));
        supervisor.shutdown_and_wait();

        let diagnostics = rusqlite::Connection::open(root.join("state.sqlite3"))
            .unwrap()
            .query_row(
                r#"
                    SELECT COUNT(*), MIN(candidate_format), SUM(retryable),
                           SUM(CASE WHEN finished_at IS NOT NULL THEN 1 ELSE 0 END)
                    FROM download_page_attempts
                "#,
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(diagnostics, (2, "png".into(), 0, 2));

        let bundle = repository
            .pipeline_artifact_bundle(&DownloadEntryId::new(entry_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(bundle.artifact.state, DownloadArtifactState::Complete);
        assert_eq!(bundle.pages.len(), 2);
        assert!(bundle.pages.iter().all(|page| {
            page.state == PageArtifactState::Present
                && page.sha256.is_some()
                && page.storage_format == Some(ArtifactStorageFormat::Webp)
        }));
        let manifest_path = root.join("downloads").join(
            bundle
                .artifact
                .manifest_relative_path
                .as_ref()
                .unwrap()
                .as_str(),
        );
        let manifest: ArtifactManifest =
            serde_json::from_reader(std::fs::File::open(manifest_path).unwrap()).unwrap();
        assert_eq!(manifest.schema_version, ARTIFACT_MANIFEST_SCHEMA_VERSION);
        assert_eq!(manifest.pages.len(), 2);
        let first_page = FilesystemArtifactStore::new()
            .first_verified_page_path(&root.join("downloads"), &bundle)
            .unwrap();
        assert_eq!(
            first_page.file_name().and_then(|name| name.to_str()),
            Some("0001.webp")
        );
        assert_eq!(source.calls(), vec![1, 2]);
        let part_files = std::fs::read_dir(
            root.join("downloads")
                .join(bundle.artifact.relative_directory.as_str()),
        )
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
        .count();
        assert_eq!(part_files, 0);
    }

    #[test]
    fn unsigned_source_revision_never_overflows_sqlite_gallery_revision() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let source = Arc::new(FakeDownloadSource::new(1, None).with_gallery_revision(u64::MAX));
        let (supervisor, _events) = launch(&repository, source);
        let queued = service
            .download_queue_add(vec![4_113_714], "unsigned-source-revision".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        supervisor.enqueue_all(queued.jobs).unwrap();
        wait_for_state(&service, &entry_id, JobState::Completed, 100.0);
        supervisor.shutdown_and_wait();

        let stored: (i64, String) = rusqlite::Connection::open(root.join("state.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT revision, source_revision FROM galleries WHERE gallery_id=4113714",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(stored.0, 0);
        assert_eq!(stored.1, "fixture-gallery:ffffffffffffffff");
    }

    #[test]
    fn interrupted_pipeline_resumes_from_verified_page_checkpoint() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let blocking_source = Arc::new(FakeDownloadSource::new(2, Some(2)));
        let (first_supervisor, _events) = launch(&repository, blocking_source.clone());
        let queued = service
            .download_queue_add(vec![77], "pipeline-resume".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        first_supervisor.enqueue_all(queued.jobs).unwrap();
        wait_for_state(&service, &entry_id, JobState::Downloading, 50.0);
        first_supervisor.shutdown_and_wait();
        assert_eq!(blocking_source.calls(), vec![1, 2]);

        let entry_key = DownloadEntryId::new(entry_id.clone()).unwrap();
        let reserved_directory = repository
            .pipeline_artifact_bundle(&entry_key)
            .unwrap()
            .unwrap()
            .artifact
            .relative_directory;
        let settings = service.settings_get().unwrap();
        service
            .settings_update(
                SettingsPatch {
                    folder_name_template: Some("{id} renamed".into()),
                    ..SettingsPatch::default()
                },
                settings.revision,
            )
            .unwrap();

        assert_eq!(service.download_recover_interrupted().unwrap(), 1);
        let resumed_source = Arc::new(FakeDownloadSource::new(2, None));
        let (second_supervisor, _events) = launch(&repository, resumed_source.clone());
        assert_eq!(second_supervisor.resume_interrupted().unwrap(), 1);
        let completed = wait_for_state(&service, &entry_id, JobState::Completed, 100.0);
        second_supervisor.shutdown_and_wait();

        assert_eq!(completed.attempt, Some(2));
        assert_eq!(resumed_source.calls(), vec![2]);
        let bundle = repository
            .pipeline_artifact_bundle(&DownloadEntryId::new(entry_id).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(bundle.artifact.relative_directory, reserved_directory);
        assert!(!root.join("downloads/77 renamed").exists());
        assert_eq!(bundle.pages.len(), 2);
        assert_eq!(bundle.pages[0].page_id.source_page_number.get(), 1);
        assert_eq!(bundle.pages[1].page_id.source_page_number.get(), 2);
    }

    #[test]
    fn occupied_first_destination_stays_typed_and_unmodified_across_retry() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let occupied = root
            .join("downloads")
            .join("[fixture artist] Synthetic download fixture [fixture group] 66");
        std::fs::create_dir_all(&occupied).unwrap();
        std::fs::write(occupied.join("user-owned.txt"), b"keep").unwrap();
        let source = Arc::new(FakeDownloadSource::new(1, None));
        let (supervisor, _events) = launch(&repository, source.clone());
        let queued = service
            .download_queue_add(vec![66], "pipeline-collision".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        supervisor.enqueue_all(queued.jobs).unwrap();

        let first = wait_for_state(&service, &entry_id, JobState::Failed, 0.0);
        assert_eq!(
            first.error_code.as_deref(),
            Some("ARTIFACT_DESTINATION_OCCUPIED")
        );
        let retry = service.download_retry(vec![entry_id.clone()]).unwrap();
        supervisor.enqueue_retries(&retry).unwrap();
        let second = wait_for_state(&service, &entry_id, JobState::Failed, 0.0);
        assert_eq!(second.attempt, Some(2));
        assert_eq!(
            second.error_code.as_deref(),
            Some("ARTIFACT_DESTINATION_OCCUPIED")
        );
        assert_eq!(
            std::fs::read(occupied.join("user-owned.txt")).unwrap(),
            b"keep"
        );
        assert!(source.calls().is_empty());
        supervisor.shutdown_and_wait();
    }

    #[test]
    fn verified_artifact_quarantine_is_recoverable_and_never_purged_automatically() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let source = Arc::new(FakeDownloadSource::new(1, None));
        let (supervisor, _events) = launch(&repository, source);
        let queued = service
            .download_queue_add(vec![88], "pipeline-quarantine".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        supervisor.enqueue_all(queued.jobs).unwrap();
        wait_for_state(&service, &entry_id, JobState::Completed, 100.0);

        let replacement_root = root.join("replacement-downloads");
        std::fs::create_dir(&replacement_root).unwrap();
        let settings = service.settings_get().unwrap();
        service
            .settings_update(
                crate::domain::SettingsPatch {
                    download_root: Some(replacement_root.to_string_lossy().into_owned()),
                    ..crate::domain::SettingsPatch::default()
                },
                settings.revision,
            )
            .unwrap();

        let quarantined = supervisor
            .quarantine_entries(vec![entry_id.clone()], "integration test quarantine".into())
            .unwrap();
        assert_eq!(quarantined[0].state, JobState::Quarantined);
        let bundle = repository
            .pipeline_artifact_bundle(&DownloadEntryId::new(entry_id.clone()).unwrap())
            .unwrap()
            .unwrap();
        let relative_directory = bundle.artifact.relative_directory.as_str();
        assert!(!root.join("downloads").join(relative_directory).exists());
        let record_directory = std::fs::read_dir(root.join("downloads/.atsumi-quarantine"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let quarantine_directory = record_directory.join(relative_directory);
        assert!(quarantine_directory.join("0001.webp").is_file());
        let manifest: ArtifactManifest = serde_json::from_reader(
            std::fs::File::open(quarantine_directory.join("manifest.json")).unwrap(),
        )
        .unwrap();
        assert!(manifest.pages.iter().all(|page| page.quarantined));
        assert!(manifest
            .pages
            .iter()
            .all(|page| page.relative_path.starts_with(".atsumi-quarantine/")));

        let restored = supervisor.restore_entries(vec![entry_id.clone()]).unwrap();
        assert_eq!(restored[0].state, JobState::Completed);
        assert!(root
            .join("downloads")
            .join(relative_directory)
            .join("0001.webp")
            .is_file());
        assert!(!quarantine_directory.exists());
        let restored_manifest: ArtifactManifest = serde_json::from_reader(
            std::fs::File::open(
                root.join("downloads")
                    .join(relative_directory)
                    .join("manifest.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert!(restored_manifest.pages.iter().all(|page| !page.quarantined));
        assert!(restored_manifest
            .pages
            .iter()
            .all(|page| page.relative_path.starts_with(relative_directory)));
        supervisor.shutdown_and_wait();
    }

    #[test]
    fn startup_recovery_finishes_a_quarantine_move_without_scanning_completed_artifacts() {
        let temp = tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let (repository, service) = configured_repository(&root);
        let source = Arc::new(FakeDownloadSource::new(1, None));
        let (supervisor, _events) = launch(&repository, source);
        let queued = service
            .download_queue_add(vec![99], "pipeline-quarantine-crash".into())
            .unwrap();
        let entry_id = queued.entries[0].entry_id.to_string();
        supervisor.enqueue_all(queued.jobs).unwrap();
        wait_for_state(&service, &entry_id, JobState::Completed, 100.0);

        let entry = DownloadEntryId::new(entry_id.clone()).unwrap();
        let bundle = repository
            .pipeline_artifact_bundle(&entry)
            .unwrap()
            .unwrap();
        let saga = QuarantineSaga {
            record_id: "crash-window-record".into(),
            entry_id: entry,
            original_relative_path: bundle.artifact.relative_directory.clone(),
            quarantine_relative_path: ArtifactRelativePath::new(
                ".atsumi-quarantine/crash-window-record/gallery-99",
            )
            .unwrap(),
            reason: "fault injection".into(),
            state: QuarantineSagaState::PendingQuarantine,
        };
        repository.pipeline_quarantine_begin(&saga).unwrap();
        let store = FilesystemArtifactStore::new();
        store
            .move_managed_directory(
                &root.join("downloads"),
                &saga.original_relative_path,
                &saga.quarantine_relative_path,
            )
            .unwrap();

        let report = supervisor.recover_startup_state().unwrap();
        assert_eq!(report.inspected_artifacts, 0);
        assert_eq!(report.verified_artifacts, 0);
        assert!(
            report.issues.iter().all(|issue| issue.entry_id != entry_id),
            "{:?}",
            report.issues
        );
        let quarantined = wait_for_state(&service, &entry_id, JobState::Quarantined, 100.0);
        assert_eq!(quarantined.state, JobState::Quarantined);
        assert!(root
            .join("downloads/.atsumi-quarantine/crash-window-record/gallery-99/manifest.json")
            .is_file());
        supervisor.shutdown_and_wait();
    }
}
