use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    domain::{
        ArtifactBundle, ArtifactManifest, ArtifactRelativePath, DownloadArtifact,
        DownloadArtifactState, DownloadEntryId, Gallery, GalleryId, GalleryMetadata, PageArtifact,
        PageArtifactState, SourcePageNumber, StoredClassicImport, ARTIFACT_MANIFEST_SCHEMA_VERSION,
        HASH_PROFILE_VERSION,
    },
    thumbnail::CancellationToken,
};

use super::{
    ArtifactStore, ClassicImportRepository, ClassicImportTransitionOutcome, DownloadPagePayload,
    RepositoryError, StateRepository,
};
use crate::domain::{
    ClassicConflictCode, ClassicConflictSeverity, ClassicImportApplyRequest,
    ClassicImportApplyResult, ClassicImportConflict, ClassicImportDryRunRequest,
    ClassicImportGalleryPlan, ClassicImportPagePlan, ClassicImportPlan, ClassicImportReport,
    ClassicImportRollbackRequest, ClassicImportState,
};

use super::ApplicationError;

#[derive(Debug, Clone)]
pub struct ClassicSourceInventory {
    pub data_root: PathBuf,
    pub download_root: Option<PathBuf>,
    pub data_root_label: String,
    pub download_root_label: Option<String>,
    pub plan: ClassicImportPlan,
}

pub trait ClassicSourceInspector: Send + Sync {
    fn inspect(
        &self,
        data_root: &Path,
        download_root: Option<&Path>,
    ) -> Result<ClassicSourceInventory, ApplicationError>;

    fn read_page(
        &self,
        data_root: &Path,
        download_root: Option<&Path>,
        page: &ClassicImportPagePlan,
        cancellation: &CancellationToken,
    ) -> Result<DownloadPagePayload, ApplicationError>;
}

#[derive(Clone)]
pub struct ClassicImportService {
    repository: Arc<dyn ClassicImportRepository>,
    settings: Arc<dyn StateRepository>,
    inspector: Arc<dyn ClassicSourceInspector>,
    store: Arc<dyn ArtifactStore>,
}

impl ClassicImportService {
    pub fn new(
        repository: Arc<dyn ClassicImportRepository>,
        settings: Arc<dyn StateRepository>,
        inspector: Arc<dyn ClassicSourceInspector>,
        store: Arc<dyn ArtifactStore>,
    ) -> Self {
        Self {
            repository,
            settings,
            inspector,
            store,
        }
    }

    pub fn dry_run(
        &self,
        request: ClassicImportDryRunRequest,
    ) -> Result<ClassicImportReport, ApplicationError> {
        let data_root = validated_input_path(&request.data_root, "dataRoot")?;
        let download_root = request
            .download_root
            .as_deref()
            .map(|value| validated_input_path(value, "downloadRoot"))
            .transpose()?;
        let mut inventory = self
            .inspector
            .inspect(&data_root, download_root.as_deref())?;
        self.add_next_conflicts(&mut inventory.plan)?;
        let data_root_text = path_text(&inventory.data_root)?;
        let download_root_text = inventory
            .download_root
            .as_deref()
            .map(path_text)
            .transpose()?;
        let stored = self.repository.classic_import_save_dry_run(
            &data_root_text,
            download_root_text.as_deref(),
            &inventory.data_root_label,
            inventory.download_root_label.as_deref(),
            &inventory.plan,
        )?;
        Ok(stored.report)
    }

    pub fn get(&self, import_id: &str) -> Result<ClassicImportReport, ApplicationError> {
        let import_id = normalized_import_id(import_id)?;
        self.repository
            .classic_import_get(&import_id)?
            .map(|stored| stored.report)
            .ok_or(ApplicationError::ClassicImportNotFound(import_id))
    }

    pub fn apply(
        &self,
        request: ClassicImportApplyRequest,
    ) -> Result<ClassicImportApplyResult, ApplicationError> {
        let import_id = normalized_import_id(&request.import_id)?;
        let stored = self
            .repository
            .classic_import_get(&import_id)?
            .ok_or_else(|| ApplicationError::ClassicImportNotFound(import_id.clone()))?;
        if stored.report.revision != request.expected_revision {
            return Err(ApplicationError::RevisionConflict {
                resource: "classicImport",
                expected: request.expected_revision,
                actual: stored.report.revision,
            });
        }
        if stored.report.state != ClassicImportState::DryRun {
            return Err(ApplicationError::ClassicImportInvalid(
                "only a current dry-run report can be applied".into(),
            ));
        }
        let accepted = request
            .accepted_conflict_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        let missing = stored
            .report
            .conflicts
            .iter()
            .filter(|conflict| conflict.requires_acknowledgement)
            .find(|conflict| !accepted.contains(conflict.conflict_id.as_str()));
        if let Some(conflict) = missing {
            return Err(ApplicationError::ClassicImportConflict(
                conflict.conflict_id.clone(),
            ));
        }
        if !stored.report.can_apply {
            return Err(ApplicationError::ClassicImportInvalid(
                "the dry-run report contains no safe changes to apply".into(),
            ));
        }

        let current = self.inspector.inspect(
            Path::new(&stored.data_root),
            stored.download_root.as_deref().map(Path::new),
        )?;
        if current.plan.source_fingerprint != stored.plan.source_fingerprint {
            return Err(ApplicationError::ClassicImportSourceChanged);
        }

        let settings = self.settings.settings_get()?;
        if settings.download_root.trim().is_empty()
            && stored.plan.galleries.iter().any(|gallery| gallery.eligible)
        {
            return Err(ApplicationError::ClassicImportInvalid(
                "select a Next download folder before importing Classic artifacts".into(),
            ));
        }
        let next_root = if settings.download_root.trim().is_empty() {
            None
        } else {
            Some(
                self.store
                    .validate_download_root(Path::new(&settings.download_root))?,
            )
        };

        let applying = match self
            .repository
            .classic_import_begin_apply(&import_id, request.expected_revision)?
        {
            ClassicImportTransitionOutcome::Applied(stored) => *stored,
            ClassicImportTransitionOutcome::NotFound => {
                return Err(ApplicationError::ClassicImportNotFound(import_id));
            }
            ClassicImportTransitionOutcome::RevisionConflict { actual_revision } => {
                return Err(ApplicationError::RevisionConflict {
                    resource: "classicImport",
                    expected: request.expected_revision,
                    actual: actual_revision,
                });
            }
            ClassicImportTransitionOutcome::InvalidState(reason) => {
                return Err(ApplicationError::ClassicImportInvalid(reason));
            }
        };

        let result = self.copy_and_commit(&applying, next_root.as_deref());
        if let Err(error) = &result {
            let _ = self.repository.classic_import_fail(
                &import_id,
                "CLASSIC_IMPORT_APPLY_FAILED",
                &safe_import_error(error),
            );
        }
        result
    }

    fn copy_and_commit(
        &self,
        applying: &StoredClassicImport,
        next_root: Option<&Path>,
    ) -> Result<ClassicImportApplyResult, ApplicationError> {
        let mut bundles = Vec::new();
        let mut copied_files = 0u32;
        let mut copied_bytes = 0u64;
        for gallery in applying
            .plan
            .galleries
            .iter()
            .filter(|gallery| gallery.eligible)
        {
            let root = next_root.ok_or_else(|| {
                ApplicationError::ClassicImportInvalid(
                    "a Next download folder is required for artifact copies".into(),
                )
            })?;
            let entry_id = classic_entry_id(&applying.report.import_id, gallery.gallery_id)?;
            let relative_directory = format!("gallery-{}", gallery.gallery_id);
            // Record the intended destination before the first filesystem write. If the
            // process exits mid-copy, startup recovery can quarantine the partial folder.
            self.repository.classic_import_copy_mark(
                &applying.report.import_id,
                gallery.gallery_id,
                entry_id.as_str(),
                &relative_directory,
                0,
                0,
            )?;
            let bundle = self.copy_gallery(applying, root, gallery)?;
            copied_files = copied_files.saturating_add(bundle.pages.len() as u32);
            copied_bytes = copied_bytes.saturating_add(
                bundle
                    .pages
                    .iter()
                    .filter_map(|page| page.byte_length)
                    .sum::<u64>(),
            );
            self.repository.classic_import_copy_mark(
                &applying.report.import_id,
                gallery.gallery_id,
                bundle.artifact.entry_id.as_str(),
                bundle.artifact.relative_directory.as_str(),
                bundle.pages.len() as u32,
                bundle
                    .pages
                    .iter()
                    .filter_map(|page| page.byte_length)
                    .sum(),
            )?;
            bundles.push(bundle);
        }

        let expected_revision = applying.report.revision;
        let committed = match self.repository.classic_import_commit_apply(
            &applying.report.import_id,
            expected_revision,
            &bundles,
        )? {
            ClassicImportTransitionOutcome::Applied(stored) => *stored,
            ClassicImportTransitionOutcome::NotFound => {
                return Err(ApplicationError::ClassicImportNotFound(
                    applying.report.import_id.clone(),
                ));
            }
            ClassicImportTransitionOutcome::RevisionConflict { actual_revision } => {
                return Err(ApplicationError::RevisionConflict {
                    resource: "classicImport",
                    expected: expected_revision,
                    actual: actual_revision,
                });
            }
            ClassicImportTransitionOutcome::InvalidState(reason) => {
                return Err(ApplicationError::ClassicImportInvalid(reason));
            }
        };
        Ok(ClassicImportApplyResult {
            report: committed.report,
            imported_gallery_ids: bundles
                .iter()
                .map(|bundle| bundle.gallery.id.get())
                .collect(),
            copied_files,
            copied_bytes,
        })
    }

    fn copy_gallery(
        &self,
        applying: &StoredClassicImport,
        next_root: &Path,
        plan: &ClassicImportGalleryPlan,
    ) -> Result<ArtifactBundle, ApplicationError> {
        let gallery_id = GalleryId::new(plan.gallery_id)?;
        let gallery = Gallery::new(
            gallery_id,
            0,
            GalleryMetadata::new(
                plan.title.clone(),
                plan.artist.clone(),
                plan.group.clone(),
                plan.expected_pages,
            )?,
        );
        let entry_id = classic_entry_id(&applying.report.import_id, plan.gallery_id)?;
        let layout = self.store.prepare_layout(next_root, &gallery)?;
        let cancellation = CancellationToken::new();
        let mut pages = Vec::with_capacity(plan.pages.len());
        for source in &plan.pages {
            let payload = self.inspector.read_page(
                Path::new(&applying.data_root),
                applying.download_root.as_deref().map(Path::new),
                source,
                &cancellation,
            )?;
            let stored = self.store.store_page(&layout, &payload, &cancellation)?;
            let mut relative_path = stored.relative_path.clone();
            let mut state = PageArtifactState::Present;
            if source.excluded {
                let destination = ArtifactRelativePath::new(format!(
                    "{}/.atsumi-page-quarantine/classic-import/{:04}.webp",
                    layout.relative_directory.as_str(),
                    source.source_page
                ))?;
                self.store
                    .move_managed_file(next_root, &stored.relative_path, &destination)?;
                relative_path = destination;
                state = PageArtifactState::Quarantined;
            }
            let page = PageArtifact::new(
                entry_id.clone(),
                gallery_id,
                SourcePageNumber::new(source.source_page)?,
                relative_path,
                state,
                Some(stored.byte_length),
            )?
            .with_verification(
                stored.sha256,
                stored.storage_format,
                stored.source_revision,
                stored.verified_at,
            )?
            .with_excluded(source.excluded);
            pages.push(page);
        }
        pages.sort_by_key(|page| page.page_id.source_page_number);
        let completed_at = now_unix_ms();
        let artifact = DownloadArtifact::new(
            entry_id,
            gallery_id,
            0,
            layout.relative_directory.clone(),
            plan.expected_pages,
            DownloadArtifactState::Complete,
        )?
        .with_manifest(
            layout.manifest_relative_path.clone(),
            ARTIFACT_MANIFEST_SCHEMA_VERSION,
            env!("CARGO_PKG_VERSION"),
            HASH_PROFILE_VERSION,
            completed_at,
        )?;
        let bundle = ArtifactBundle::new(gallery, artifact, pages)?;
        let manifest = ArtifactManifest::from_bundle(&bundle)?;
        self.store.write_manifest(&layout, &manifest)?;
        let persisted = self.store.read_manifest(&layout)?;
        if persisted.as_ref() != Some(&manifest) {
            return Err(ApplicationError::ClassicImportInvalid(
                "the copied artifact manifest did not verify".into(),
            ));
        }
        Ok(bundle)
    }

    pub fn rollback(
        &self,
        request: ClassicImportRollbackRequest,
    ) -> Result<ClassicImportReport, ApplicationError> {
        let import_id = normalized_import_id(&request.import_id)?;
        let rolling_back = match self
            .repository
            .classic_import_begin_rollback(&import_id, request.expected_revision)?
        {
            ClassicImportTransitionOutcome::Applied(stored) => *stored,
            ClassicImportTransitionOutcome::NotFound => {
                return Err(ApplicationError::ClassicImportNotFound(import_id));
            }
            ClassicImportTransitionOutcome::RevisionConflict { actual_revision } => {
                return Err(ApplicationError::RevisionConflict {
                    resource: "classicImport",
                    expected: request.expected_revision,
                    actual: actual_revision,
                });
            }
            ClassicImportTransitionOutcome::InvalidState(reason) => {
                return Err(ApplicationError::ClassicImportInvalid(reason));
            }
        };
        let result = self.finish_rollback(&rolling_back);
        if let Err(error) = &result {
            let _ = self.repository.classic_import_fail(
                &rolling_back.report.import_id,
                "CLASSIC_IMPORT_ROLLBACK_FAILED",
                &safe_import_error(error),
            );
        }
        result.map(|stored| stored.report)
    }

    fn finish_rollback(
        &self,
        rolling_back: &StoredClassicImport,
    ) -> Result<StoredClassicImport, ApplicationError> {
        let copies = self
            .repository
            .classic_import_copied_artifacts(&rolling_back.report.import_id)?;
        if !copies.is_empty() {
            let settings = self.settings.settings_get()?;
            let root = self
                .store
                .validate_download_root(Path::new(&settings.download_root))?;
            for copy in copies {
                let source = ArtifactRelativePath::new(&copy.relative_directory)?;
                let destination = ArtifactRelativePath::new(format!(
                    ".atsumi-quarantine/classic-import/{}/gallery-{}",
                    rolling_back.report.import_id, copy.gallery_id
                ))?;
                let source_exists = self.store.managed_path_exists(&root, &source)?;
                let destination_exists = self.store.managed_path_exists(&root, &destination)?;
                match (source_exists, destination_exists) {
                    (true, false) => {
                        self.store
                            .move_managed_directory(&root, &source, &destination)?
                    }
                    (false, true) => {}
                    (false, false) => {}
                    (true, true) => {
                        return Err(ApplicationError::ClassicImportInvalid(
                            "both the imported artifact and rollback quarantine exist; no path was overwritten"
                                .into(),
                        ));
                    }
                }
            }
        }
        match self.repository.classic_import_commit_rollback(
            &rolling_back.report.import_id,
            rolling_back.report.revision,
        )? {
            ClassicImportTransitionOutcome::Applied(stored) => Ok(*stored),
            ClassicImportTransitionOutcome::NotFound => Err(
                ApplicationError::ClassicImportNotFound(rolling_back.report.import_id.clone()),
            ),
            ClassicImportTransitionOutcome::RevisionConflict { actual_revision } => {
                Err(ApplicationError::RevisionConflict {
                    resource: "classicImport",
                    expected: rolling_back.report.revision,
                    actual: actual_revision,
                })
            }
            ClassicImportTransitionOutcome::InvalidState(reason) => {
                Err(ApplicationError::ClassicImportInvalid(reason))
            }
        }
    }

    pub fn recover_incomplete(&self) -> Result<u32, ApplicationError> {
        let imports = self.repository.classic_import_incomplete()?;
        let mut recovered = 0u32;
        for stored in imports {
            match stored.report.state {
                ClassicImportState::RollingBack => {
                    if self.finish_rollback(&stored).is_ok() {
                        recovered = recovered.saturating_add(1);
                    }
                }
                ClassicImportState::Applying => {
                    let Some(failed) = self.repository.classic_import_fail(
                        &stored.report.import_id,
                        "CLASSIC_IMPORT_INTERRUPTED",
                        "The previous import stopped before its database commit; Classic input was not changed",
                    )? else {
                        continue;
                    };
                    let rolling_back = match self.repository.classic_import_begin_rollback(
                        &failed.report.import_id,
                        failed.report.revision,
                    )? {
                        ClassicImportTransitionOutcome::Applied(value) => *value,
                        _ => continue,
                    };
                    if self.finish_rollback(&rolling_back).is_ok() {
                        recovered = recovered.saturating_add(1);
                    }
                }
                _ => {}
            }
        }
        Ok(recovered)
    }

    fn add_next_conflicts(&self, plan: &mut ClassicImportPlan) -> Result<(), ApplicationError> {
        let ids = plan
            .galleries
            .iter()
            .map(|gallery| gallery.gallery_id)
            .collect::<Vec<_>>();
        let existing = self
            .repository
            .classic_import_existing_gallery_ids(&ids)?
            .into_iter()
            .collect::<BTreeSet<_>>();
        let settings = self.settings.settings_get()?;
        let next_root = if settings.download_root.trim().is_empty() {
            None
        } else {
            Some(
                self.store
                    .validate_download_root(Path::new(&settings.download_root))?,
            )
        };
        for gallery in &mut plan.galleries {
            if existing.contains(&gallery.gallery_id) {
                let conflict_id = format!("existing-next-gallery:{}", gallery.gallery_id);
                gallery.eligible = false;
                gallery.conflict_ids.push(conflict_id.clone());
                plan.conflicts.push(ClassicImportConflict {
                    conflict_id,
                    code: ClassicConflictCode::ExistingNextGallery,
                    severity: ClassicConflictSeverity::Blocking,
                    gallery_id: Some(gallery.gallery_id),
                    message:
                        "Next already has a download record for this gallery; it was not overwritten"
                            .into(),
                    requires_acknowledgement: false,
                });
            }
            if let Some(root) = next_root.as_deref() {
                let destination =
                    ArtifactRelativePath::new(format!("gallery-{}", gallery.gallery_id))?;
                if self.store.managed_path_exists(root, &destination)? {
                    let conflict_id = format!("existing-next-destination:{}", gallery.gallery_id);
                    gallery.eligible = false;
                    gallery.conflict_ids.push(conflict_id.clone());
                    plan.conflicts.push(ClassicImportConflict {
                        conflict_id,
                        code: ClassicConflictCode::ExistingDestination,
                        severity: ClassicConflictSeverity::Blocking,
                        gallery_id: Some(gallery.gallery_id),
                        message:
                            "The Next destination folder already exists; it was not overwritten"
                                .into(),
                        requires_acknowledgement: false,
                    });
                }
            }
        }
        let mut available = existing;
        available.extend(
            plan.galleries
                .iter()
                .filter(|gallery| gallery.eligible)
                .map(|gallery| gallery.gallery_id),
        );
        for series in &plan.series {
            let mut members = vec![series.parent_gallery_id];
            members.extend(series.member_gallery_ids.iter().copied());
            members.sort_unstable();
            members.dedup();
            for gallery_id in members {
                if available.contains(&gallery_id) {
                    continue;
                }
                plan.conflicts.push(ClassicImportConflict {
                    conflict_id: format!(
                        "series-member-unavailable:{}:{}",
                        series.parent_gallery_id, gallery_id
                    ),
                    code: ClassicConflictCode::SeriesMemberUnavailable,
                    severity: ClassicConflictSeverity::Warning,
                    gallery_id: Some(gallery_id),
                    message: "A Classic series member is unavailable; only resolvable members will be linked"
                        .into(),
                    requires_acknowledgement: true,
                });
            }
        }
        plan.conflicts
            .sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));
        Ok(())
    }
}

fn classic_entry_id(import_id: &str, gallery_id: i64) -> Result<DownloadEntryId, ApplicationError> {
    DownloadEntryId::new(format!("classic-{import_id}-{gallery_id}")).map_err(Into::into)
}

fn validated_input_path(value: &str, field: &'static str) -> Result<PathBuf, ApplicationError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(crate::domain::ValidationError::new(field, "must not be empty").into());
    }
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(crate::domain::ValidationError::new(field, "must be absolute").into());
    }
    Ok(path)
}

fn path_text(path: &Path) -> Result<String, ApplicationError> {
    path.to_str().map(str::to_owned).ok_or_else(|| {
        ApplicationError::ClassicImportInvalid("a selected path is not valid Unicode".into())
    })
}

fn normalized_import_id(value: &str) -> Result<String, ApplicationError> {
    let value = value.trim();
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(ApplicationError::ClassicImportInvalid(
            "the import ID is invalid".into(),
        ));
    }
    Ok(value.to_owned())
}

fn now_unix_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn safe_import_error(error: &ApplicationError) -> String {
    match error {
        ApplicationError::ClassicImportSourceChanged => {
            "Classic source changed after the dry run".into()
        }
        ApplicationError::ClassicImportInvalid(message)
        | ApplicationError::ClassicImportConflict(message) => message.clone(),
        ApplicationError::DownloadPipeline(error) => error.message.clone(),
        ApplicationError::Repository(RepositoryError::Busy(_)) => {
            "The Next database is busy".into()
        }
        _ => "The import could not complete safely".into(),
    }
}
