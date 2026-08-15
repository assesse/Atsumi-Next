use std::{
    path::PathBuf,
    sync::{mpsc::Sender, Arc, Mutex},
    thread::{self, JoinHandle},
};

use crate::{
    domain::{
        DuplicateDecisionApplyOutcome, DuplicateDecisionRequest, DuplicateReview, DuplicateScanRun,
        DuplicateScanState, DuplicateSnapshot, ExternalRelationEvidence, HashProfile,
    },
    thumbnail::CancellationToken,
};

use super::{
    duplicate_analyzer::{
        analyze_artifact_pair, compute_page_hash, gallery_ref, verified_scan_pages, HashedArtifact,
    },
    ApplicationError, ArtifactStore, DuplicateRelationProvider, DuplicateRepository,
    RepositoryError, StateRepository,
};

const PAIR_PROGRESS_EVENT_INTERVAL: u64 = 64;

#[derive(Debug, Default)]
pub struct DisabledDuplicateRelationProvider;

impl DuplicateRelationProvider for DisabledDuplicateRelationProvider {
    fn enabled(&self) -> bool {
        false
    }

    fn relation(
        &self,
        _parent_gallery_id: crate::domain::GalleryId,
        _candidate_gallery_id: crate::domain::GalleryId,
    ) -> Result<Option<ExternalRelationEvidence>, RepositoryError> {
        Ok(None)
    }
}

#[derive(Clone)]
pub struct DuplicateSupervisor {
    inner: Arc<DuplicateSupervisorInner>,
}

struct DuplicateSupervisorInner {
    repository: Arc<dyn DuplicateRepository>,
    settings: Arc<dyn StateRepository>,
    store: Arc<dyn ArtifactStore>,
    relations: Arc<dyn DuplicateRelationProvider>,
    events: Sender<DuplicateScanRun>,
    control: Mutex<()>,
    active: Mutex<Option<ActiveRun>>,
}

struct ActiveRun {
    run_id: String,
    cancellation: CancellationToken,
    worker: Option<JoinHandle<()>>,
}

impl DuplicateSupervisor {
    pub fn new(
        repository: Arc<dyn DuplicateRepository>,
        settings: Arc<dyn StateRepository>,
        store: Arc<dyn ArtifactStore>,
        relations: Arc<dyn DuplicateRelationProvider>,
        events: Sender<DuplicateScanRun>,
    ) -> Self {
        Self {
            inner: Arc::new(DuplicateSupervisorInner {
                repository,
                settings,
                store,
                relations,
                events,
                control: Mutex::new(()),
                active: Mutex::new(None),
            }),
        }
    }

    pub fn recover_interrupted(&self) -> Result<usize, ApplicationError> {
        self.inner
            .repository
            .duplicate_recover_interrupted()
            .map_err(Into::into)
    }

    pub fn snapshot(&self) -> Result<DuplicateSnapshot, ApplicationError> {
        self.inner
            .repository
            .duplicate_snapshot()
            .map_err(Into::into)
    }

    pub fn review_get(&self, candidate_id: &str) -> Result<DuplicateReview, ApplicationError> {
        let candidate_id = normalized_candidate_id(candidate_id)?;
        self.inner
            .repository
            .duplicate_review_get(candidate_id)?
            .ok_or_else(|| ApplicationError::DuplicateCandidateNotFound(candidate_id.to_owned()))
    }

    pub fn decision_apply(
        &self,
        request: DuplicateDecisionRequest,
    ) -> Result<DuplicateReview, ApplicationError> {
        validate_decision_request(&request)?;
        match self.inner.repository.duplicate_decision_apply(&request)? {
            DuplicateDecisionApplyOutcome::Applied(review) => Ok(*review),
            DuplicateDecisionApplyOutcome::CandidateNotFound => Err(
                ApplicationError::DuplicateCandidateNotFound(request.candidate_id),
            ),
            DuplicateDecisionApplyOutcome::RevisionConflict { actual_revision } => {
                Err(ApplicationError::RevisionConflict {
                    resource: "duplicateCandidate",
                    expected: request.expected_revision,
                    actual: actual_revision,
                })
            }
        }
    }

    pub fn start(&self) -> Result<DuplicateScanRun, ApplicationError> {
        let _control = self.control_lock()?;
        self.reap_finished_worker();
        if let Some(run) = self.active_run()? {
            return Ok(run);
        }

        let settings = self.inner.settings.settings_get()?;
        if settings.download_root.trim().is_empty() {
            return Err(super::DownloadPipelineError::root_required().into());
        }
        // Validate once at the scan boundary.  Individual reads use a
        // canonical read-only resolver and therefore never create probes.
        let root = self
            .inner
            .store
            .validate_download_root(&PathBuf::from(settings.download_root))?;
        let bundles = select_scan_bundles(self.inner.repository.duplicate_artifact_bundles()?);
        let total_artifacts = u32::try_from(bundles.len()).unwrap_or(u32::MAX);
        let total_pairs = pair_count(bundles.len());
        let profile = HashProfile::current();
        let run = self.inner.repository.duplicate_scan_start(
            profile.profile_version,
            total_artifacts,
            total_pairs,
        )?;
        if run.state != DuplicateScanState::Running {
            return Ok(run);
        }

        let cancellation = CancellationToken::new();
        let worker_cancellation = cancellation.clone();
        let inner = Arc::clone(&self.inner);
        let run_id = run.run_id.clone();
        let worker_run_id = run_id.clone();
        let worker = thread::Builder::new()
            .name(format!("atsumi-duplicate-{}", short_id(&run_id)))
            .spawn(move || {
                run_scan(
                    inner,
                    worker_run_id,
                    root,
                    bundles,
                    profile,
                    worker_cancellation,
                );
            })
            .map_err(|error| {
                let _ = self.inner.repository.duplicate_scan_finish(
                    &run_id,
                    DuplicateScanState::Failed,
                    Some("DUPLICATE_WORKER_UNAVAILABLE"),
                    Some("The duplicate scan worker could not be started"),
                );
                RepositoryError::Other(format!("could not start duplicate scan worker: {error}"))
            })?;
        *self.active_lock()? = Some(ActiveRun {
            run_id,
            cancellation,
            worker: Some(worker),
        });
        let _ = self.inner.events.send(run.clone());
        Ok(run)
    }

    pub fn cancel(&self) -> Result<DuplicateScanRun, ApplicationError> {
        let _control = self.control_lock()?;
        self.reap_finished_worker();
        let run_id = {
            let active = self.active_lock()?;
            let active = active
                .as_ref()
                .ok_or(ApplicationError::DuplicateScanNotRunning)?;
            active.cancellation.cancel();
            active.run_id.clone()
        };
        let run = self
            .inner
            .repository
            .duplicate_scan_finish(
                &run_id,
                DuplicateScanState::Cancelled,
                Some("DUPLICATE_SCAN_CANCELLED"),
                Some("The duplicate scan was cancelled"),
            )?
            .ok_or(ApplicationError::DuplicateScanNotRunning)?;
        // Do not detach the cancelled worker: a replacement scan must never
        // overlap reads or writes from the previous run.
        if let Some(mut active) = self.active_lock()?.take() {
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
        }
        let _ = self.inner.events.send(run.clone());
        Ok(run)
    }

    pub fn shutdown_and_wait(&self) {
        let _control = self.control_lock().ok();
        let active = self.active_lock().ok().and_then(|mut active| active.take());
        if let Some(mut active) = active {
            active.cancellation.cancel();
            let _ = self.inner.repository.duplicate_scan_finish(
                &active.run_id,
                DuplicateScanState::Cancelled,
                Some("DUPLICATE_SCAN_APP_EXIT"),
                Some("The application closed during duplicate scanning"),
            );
            if let Some(worker) = active.worker.take() {
                let _ = worker.join();
            }
        }
    }

    fn control_lock(&self) -> Result<std::sync::MutexGuard<'_, ()>, ApplicationError> {
        self.inner.control.lock().map_err(|_| {
            RepositoryError::Other("duplicate supervisor control mutex was poisoned".into()).into()
        })
    }

    fn active_lock(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<ActiveRun>>, ApplicationError> {
        self.inner.active.lock().map_err(|_| {
            RepositoryError::Other("duplicate supervisor mutex was poisoned".into()).into()
        })
    }

    fn active_run(&self) -> Result<Option<DuplicateScanRun>, ApplicationError> {
        let run_id = self
            .active_lock()?
            .as_ref()
            .map(|active| active.run_id.clone());
        let Some(run_id) = run_id else {
            return Ok(None);
        };
        Ok(self
            .inner
            .repository
            .duplicate_snapshot()?
            .run
            .filter(|run| run.run_id == run_id && run.state == DuplicateScanState::Running))
    }

    fn reap_finished_worker(&self) {
        let finished = self.active_lock().ok().and_then(|mut active| {
            active
                .as_ref()
                .and_then(|run| run.worker.as_ref())
                .is_some_and(JoinHandle::is_finished)
                .then(|| active.take())
                .flatten()
        });
        if let Some(mut finished) = finished {
            if let Some(worker) = finished.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

fn run_scan(
    inner: Arc<DuplicateSupervisorInner>,
    run_id: String,
    root: PathBuf,
    bundles: Vec<crate::domain::ArtifactBundle>,
    profile: HashProfile,
    cancellation: CancellationToken,
) {
    let result = scan_inner(&inner, &run_id, &root, &bundles, &profile, &cancellation);
    if let Err(error) = result {
        if !cancellation.is_cancelled()
            && inner
                .repository
                .duplicate_scan_is_running(&run_id)
                .unwrap_or(false)
        {
            if let Ok(Some(run)) = inner.repository.duplicate_scan_finish(
                &run_id,
                DuplicateScanState::Failed,
                Some("DUPLICATE_SCAN_FAILED"),
                Some(&stable_scan_error(&error)),
            ) {
                let _ = inner.events.send(run);
            }
        }
    }
}

fn scan_inner(
    inner: &DuplicateSupervisorInner,
    run_id: &str,
    root: &std::path::Path,
    bundles: &[crate::domain::ArtifactBundle],
    profile: &HashProfile,
    cancellation: &CancellationToken,
) -> Result<(), RepositoryError> {
    let mut artifacts = Vec::with_capacity(bundles.len());
    for (artifact_index, bundle) in bundles.iter().enumerate() {
        if cancelled(inner, run_id, cancellation)? {
            return Ok(());
        }
        let pages = verified_scan_pages(bundle).ok_or_else(|| {
            RepositoryError::Corrupt("artifact lost its verified scan eligibility".into())
        })?;
        let mut hashes = Vec::with_capacity(pages.len());
        for page in pages {
            if cancelled(inner, run_id, cancellation)? {
                return Ok(());
            }
            let sha = page
                .sha256
                .as_ref()
                .expect("verified_scan_pages guarantees SHA-256");
            let hash = if let Some(hash) = inner.repository.duplicate_page_hash_get(
                bundle.artifact.entry_id.as_str(),
                page.page_id.source_page_number,
                profile.profile_version,
                sha.as_str(),
            )? {
                hash
            } else {
                let bytes = inner
                    .store
                    .read_verified_page_bytes(root, page)
                    .map_err(|error| RepositoryError::Other(error.to_string()))?;
                let hash = compute_page_hash(
                    bundle.artifact.entry_id.as_str(),
                    bundle.gallery.id,
                    page.page_id.source_page_number,
                    sha.clone(),
                    &bytes,
                    profile,
                )?;
                inner.repository.duplicate_page_hash_upsert(&hash)?;
                hash
            };
            hashes.push(hash);
        }
        hashes.sort_by_key(|hash| hash.source_page_number);
        artifacts.push(HashedArtifact {
            gallery: gallery_ref(bundle, hashes.len() as u32),
            pages: hashes,
        });
        if let Some(run) = inner.repository.duplicate_scan_progress(
            run_id,
            u32::try_from(artifact_index + 1).unwrap_or(u32::MAX),
            0,
        )? {
            let _ = inner.events.send(run);
        }
    }

    let mut compared_pairs = 0_u64;
    for candidate_pair in candidate_pairs(&artifacts) {
        let parent_index = candidate_pair.parent_index;
        let candidate_index = candidate_pair.candidate_index;
        if cancelled(inner, run_id, cancellation)? {
            return Ok(());
        }
        let parent = &artifacts[parent_index];
        let candidate = &artifacts[candidate_index];
        let preliminary = analyze_artifact_pair(run_id, parent, candidate, profile, None);
        if preliminary.is_some() || candidate_pair.metadata_affinity > 0 {
            let external = if inner.relations.enabled() {
                inner
                    .relations
                    .relation(parent.gallery.gallery_id, candidate.gallery.gallery_id)?
            } else {
                None
            };
            let record = if external.is_some() {
                analyze_artifact_pair(run_id, parent, candidate, profile, external)
            } else {
                preliminary
            };
            if let Some(record) = record {
                let _ = inner.repository.duplicate_candidate_replace(&record)?;
            }
        }
        compared_pairs += 1;
        if compared_pairs.is_multiple_of(PAIR_PROGRESS_EVENT_INTERVAL) {
            if let Some(run) = inner.repository.duplicate_scan_progress(
                run_id,
                artifacts.len() as u32,
                compared_pairs,
            )? {
                let _ = inner.events.send(run);
            }
        }
    }

    if cancelled(inner, run_id, cancellation)? {
        return Ok(());
    }
    if let Some(run) =
        inner
            .repository
            .duplicate_scan_progress(run_id, artifacts.len() as u32, compared_pairs)?
    {
        let _ = inner.events.send(run);
    }
    if let Some(run) =
        inner
            .repository
            .duplicate_scan_finish(run_id, DuplicateScanState::Completed, None, None)?
    {
        let _ = inner.events.send(run);
    }
    Ok(())
}

fn cancelled(
    inner: &DuplicateSupervisorInner,
    run_id: &str,
    cancellation: &CancellationToken,
) -> Result<bool, RepositoryError> {
    Ok(cancellation.is_cancelled() || !inner.repository.duplicate_scan_is_running(run_id)?)
}

fn normalized_candidate_id(candidate_id: &str) -> Result<&str, ApplicationError> {
    let candidate_id = candidate_id.trim();
    if candidate_id.is_empty() || candidate_id.len() > 200 {
        return Err(crate::domain::ValidationError::new(
            "candidateId",
            "must be non-empty and at most 200 bytes",
        )
        .into());
    }
    Ok(candidate_id)
}

fn validate_decision_request(request: &DuplicateDecisionRequest) -> Result<(), ApplicationError> {
    normalized_candidate_id(&request.candidate_id)?;
    if request.series_group_id.as_ref().is_some_and(|value| {
        let value = value.trim();
        value.is_empty() || value.len() > 200
    }) {
        return Err(crate::domain::ValidationError::new(
            "seriesGroupId",
            "must be non-empty and at most 200 bytes",
        )
        .into());
    }
    if request.series_name.as_ref().is_some_and(|value| {
        let value = value.trim();
        value.is_empty() || value.len() > 200
    }) {
        return Err(crate::domain::ValidationError::new(
            "seriesName",
            "must be non-empty and at most 200 bytes",
        )
        .into());
    }
    Ok(())
}

fn pair_count(artifacts: usize) -> u64 {
    let artifacts = artifacts as u128;
    u64::try_from(artifacts.saturating_mul(artifacts.saturating_sub(1)) / 2).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CandidatePair {
    parent_index: usize,
    candidate_index: usize,
    metadata_affinity: u8,
}

/// Builds a deterministic candidate worklist from title/artist/group/page
/// metadata.  It deliberately retains zero-affinity pairs as an exhaustive
/// fallback: metadata only prioritizes expensive local evidence work and can
/// never suppress a real visual duplicate.
fn candidate_pairs(artifacts: &[HashedArtifact]) -> Vec<CandidatePair> {
    let mut pairs = Vec::with_capacity(pair_count(artifacts.len()) as usize);
    for parent_index in 0..artifacts.len() {
        for candidate_index in (parent_index + 1)..artifacts.len() {
            pairs.push(CandidatePair {
                parent_index,
                candidate_index,
                metadata_affinity: metadata_affinity(
                    &artifacts[parent_index].gallery,
                    &artifacts[candidate_index].gallery,
                ),
            });
        }
    }
    pairs.sort_by_key(|pair| {
        (
            std::cmp::Reverse(pair.metadata_affinity),
            pair.parent_index,
            pair.candidate_index,
        )
    });
    pairs
}

fn metadata_affinity(
    left: &crate::domain::DuplicateGalleryRef,
    right: &crate::domain::DuplicateGalleryRef,
) -> u8 {
    let same_group = normalized_optional_metadata(left.group.as_deref())
        .zip(normalized_optional_metadata(right.group.as_deref()))
        .is_some_and(|(left, right)| left == right);
    let same_artist = normalized_optional_metadata(left.artist.as_deref())
        .zip(normalized_optional_metadata(right.artist.as_deref()))
        .is_some_and(|(left, right)| left == right);
    let left_title = metadata_title_tokens(&left.title);
    let right_title = metadata_title_tokens(&right.title);
    let title_overlap = left_title
        .iter()
        .filter(|token| right_title.contains(*token))
        .count();
    let page_delta = left.page_count.abs_diff(right.page_count);
    let similar_length = page_delta <= left.page_count.max(right.page_count).div_ceil(10).max(1);
    u8::from(same_group) * 4
        + u8::from(same_artist) * 3
        + u8::try_from(title_overlap.min(3)).unwrap_or(3)
        + u8::from(similar_length)
}

fn normalized_optional_metadata(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

fn metadata_title_tokens(value: &str) -> std::collections::BTreeSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .map(str::trim)
        .filter(|token| token.chars().count() >= 2)
        .map(str::to_lowercase)
        .collect()
}

fn select_scan_bundles(
    bundles: Vec<crate::domain::ArtifactBundle>,
) -> Vec<crate::domain::ArtifactBundle> {
    let mut bundles = bundles
        .into_iter()
        .filter(|bundle| verified_scan_pages(bundle).is_some())
        .collect::<Vec<_>>();
    bundles.sort_by(|left, right| {
        left.gallery
            .id
            .cmp(&right.gallery.id)
            .then_with(|| right.artifact.completed_at.cmp(&left.artifact.completed_at))
            .then_with(|| right.artifact.revision.cmp(&left.artifact.revision))
            .then_with(|| left.artifact.entry_id.cmp(&right.artifact.entry_id))
    });
    bundles.dedup_by(|left, right| left.gallery.id == right.gallery.id);
    bundles
}

fn stable_scan_error(error: &RepositoryError) -> String {
    match error {
        RepositoryError::Busy(_) => "The duplicate evidence database is busy; retry the scan",
        RepositoryError::Corrupt(_) => {
            "Verified artifact metadata changed during duplicate scanning; reconcile downloads"
        }
        RepositoryError::Source(_) => "Optional relation evidence could not be loaded",
        _ => "A verified artifact could not be analyzed; reconcile downloads and retry",
    }
    .into()
}

fn short_id(value: &str) -> &str {
    value.rsplit('-').next().unwrap_or("run")
}

#[cfg(test)]
mod tests {
    use crate::domain::{
        ArtifactBundle, ArtifactRelativePath, ArtifactSha256, ArtifactStorageFormat,
        DownloadArtifact, DownloadArtifactState, DownloadEntryId, Gallery, GalleryId,
        GalleryMetadata, PageArtifact, PageArtifactState, SourcePageNumber,
    };

    use super::{candidate_pairs, select_scan_bundles, HashedArtifact};

    fn bundle(
        gallery_id: i64,
        entry_id: &str,
        revision: u64,
        completed_at: &str,
    ) -> ArtifactBundle {
        let gallery_id = GalleryId::new(gallery_id).unwrap();
        let entry_id = DownloadEntryId::new(entry_id).unwrap();
        let directory = ArtifactRelativePath::new(format!("gallery-{entry_id}")).unwrap();
        let gallery = Gallery::new(
            gallery_id,
            revision,
            GalleryMetadata::new("Gallery", None, None, 1).unwrap(),
        );
        let artifact = DownloadArtifact::new(
            entry_id.clone(),
            gallery_id,
            revision,
            directory.clone(),
            1,
            DownloadArtifactState::Complete,
        )
        .unwrap()
        .with_manifest(
            ArtifactRelativePath::new(format!("{directory}/manifest.json")).unwrap(),
            1,
            "test",
            1,
            completed_at,
        )
        .unwrap();
        let page = PageArtifact::new(
            entry_id,
            gallery_id,
            SourcePageNumber::new(1).unwrap(),
            ArtifactRelativePath::new(format!("{directory}/page-1.webp")).unwrap(),
            PageArtifactState::Present,
            Some(10),
        )
        .unwrap()
        .with_verification(
            ArtifactSha256::new(format!("{:064x}", revision + 1)).unwrap(),
            ArtifactStorageFormat::Webp,
            "source",
            completed_at,
        )
        .unwrap();
        ArtifactBundle::new(gallery, artifact, vec![page]).unwrap()
    }

    #[test]
    fn latest_verified_artifact_per_gallery_is_selected_deterministically() {
        let older = bundle(1, "entry-z-older", 50, "2026-08-14T00:00:00.000Z");
        let latest_lower_revision = bundle(1, "entry-b-latest", 2, "2026-08-15T00:00:00.000Z");
        let latest_higher_revision = bundle(1, "entry-a-latest", 3, "2026-08-15T00:00:00.000Z");
        let other = bundle(2, "entry-other", 1, "2026-08-10T00:00:00.000Z");
        let selected = select_scan_bundles(vec![
            older,
            latest_lower_revision,
            other,
            latest_higher_revision,
        ]);
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].artifact.entry_id.as_str(), "entry-a-latest");
        assert_eq!(selected[1].artifact.entry_id.as_str(), "entry-other");
    }

    #[test]
    fn metadata_prioritizes_candidate_generation_without_dropping_fallback_pairs() {
        let related_left = bundle(1, "entry-related-left", 1, "2026-08-15T00:00:00.000Z");
        let mut related_right = bundle(2, "entry-related-right", 1, "2026-08-15T00:00:00.000Z");
        related_right.gallery.metadata.title = related_left.gallery.metadata.title.clone();
        related_right.gallery.metadata.primary_artist = Some("shared artist".into());
        let mut related_left = related_left;
        related_left.gallery.metadata.primary_artist = Some("Shared Artist".into());
        let unrelated = bundle(3, "entry-unrelated", 1, "2026-08-15T00:00:00.000Z");
        let artifacts = [related_left, unrelated, related_right]
            .into_iter()
            .map(|bundle| HashedArtifact {
                gallery: crate::application::duplicate_analyzer::gallery_ref(&bundle, 1),
                pages: Vec::new(),
            })
            .collect::<Vec<_>>();

        let pairs = candidate_pairs(&artifacts);
        assert_eq!(pairs.len(), 3, "the exhaustive fallback keeps every pair");
        assert_eq!((pairs[0].parent_index, pairs[0].candidate_index), (0, 2));
        assert!(pairs[0].metadata_affinity > pairs[1].metadata_affinity);
    }
}
