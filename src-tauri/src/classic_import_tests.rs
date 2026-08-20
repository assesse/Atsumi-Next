use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
};

use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use serde_json::json;

use crate::{
    application::{
        ApplicationError, ApplicationService, ArtifactStore, AutomationRepository,
        ClassicImportRepository, ClassicImportService, ClassicImportTransitionOutcome,
        ClassicSourceInspector, DownloadRepository, StateRepository,
    },
    domain::{
        ClassicConflictCode, ClassicImportApplyRequest, ClassicImportDryRunRequest,
        ClassicImportRollbackRequest, ClassicImportState, DownloadListRequest, SettingsPatch,
    },
    infrastructure::{FilesystemArtifactStore, FilesystemClassicSource, SqliteRepository},
};

#[test]
fn classic_v1_dry_run_requires_a_new_typed_plan_before_any_copy() {
    let temporary = tempfile::tempdir().expect("create Classic import fixture");
    let fixture = create_classic_fixture(temporary.path());
    let (repository, _application, importer) =
        import_services(temporary.path(), &fixture.next_root);
    let inspector = FilesystemClassicSource::new();
    let mut inventory = inspector
        .inspect(&fixture.data_root, Some(&fixture.download_root))
        .expect("inspect Classic fixture");
    inventory.plan.schema_version = 1;
    let stored = repository
        .classic_import_save_dry_run(
            &path_text(&fixture.data_root),
            Some(&path_text(&fixture.download_root)),
            &inventory.data_root_label,
            inventory.download_root_label.as_deref(),
            &inventory.plan,
        )
        .expect("store legacy dry-run");

    let error = importer
        .apply(ClassicImportApplyRequest {
            import_id: stored.report.import_id,
            expected_revision: stored.report.revision,
            accepted_conflict_ids: Vec::new(),
        })
        .expect_err("v1 dry-run must be rerun");

    assert!(matches!(error, ApplicationError::ClassicImportPlanOutdated));
    assert!(fs::read_dir(&fixture.next_root).unwrap().next().is_none());
    assert_classic_unchanged(&fixture);
}

struct ClassicFixture {
    data_root: PathBuf,
    download_root: PathBuf,
    next_root: PathBuf,
    state_bytes: Vec<u8>,
    page_bytes: Vec<Vec<u8>>,
}

#[test]
fn classic_import_is_read_only_applies_verified_copies_and_rolls_back_only_next_data() {
    let temporary = tempfile::tempdir().expect("create Classic import fixture");
    let fixture = create_classic_fixture(temporary.path());
    let (repository, application, importer) = import_services(temporary.path(), &fixture.next_root);

    let dry_run = importer
        .dry_run(ClassicImportDryRunRequest {
            data_root: path_text(&fixture.data_root),
            download_root: Some(path_text(&fixture.download_root)),
        })
        .expect("inventory Classic source");
    assert_eq!(dry_run.state, ClassicImportState::DryRun);
    assert!(dry_run.can_apply);
    assert_eq!(dry_run.counts.galleries_eligible, 1);
    assert_eq!(dry_run.counts.page_files, 2);
    assert!(dry_run.conflicts.is_empty());
    let relative_directory = dry_run.galleries[0]
        .relative_directory
        .as_deref()
        .expect("v2 dry-run stores the planned relative directory")
        .to_owned();

    let applied = importer
        .apply(ClassicImportApplyRequest {
            import_id: dry_run.import_id.clone(),
            expected_revision: dry_run.revision,
            accepted_conflict_ids: Vec::new(),
        })
        .expect("apply safe Classic import");
    assert_eq!(applied.report.state, ClassicImportState::Applied);
    assert_eq!(applied.imported_gallery_ids, vec![123]);
    assert_eq!(applied.copied_files, 2);
    assert!(fixture
        .next_root
        .join(&relative_directory)
        .join("manifest.json")
        .is_file());
    assert!(fixture
        .next_root
        .join(&relative_directory)
        .join("0001.webp")
        .is_file());
    assert!(fixture
        .next_root
        .join(&relative_directory)
        .join("0002.webp")
        .is_file());
    assert_classic_unchanged(&fixture);

    let favorites = application
        .favorites_list()
        .expect("load imported favorites");
    assert!(favorites
        .iter()
        .any(|favorite| favorite.key().value == "alpha"));
    let downloads = application
        .download_entries_list(DownloadListRequest {
            state: None,
            query: None,
            page: 1,
            page_size: 20,
        })
        .expect("load imported download");
    assert_eq!(downloads.total_items, 1);

    let rolled_back = importer
        .rollback(ClassicImportRollbackRequest {
            import_id: applied.report.import_id.clone(),
            expected_revision: applied.report.revision,
        })
        .expect("rollback Classic import");
    assert_eq!(rolled_back.state, ClassicImportState::RolledBack);
    assert!(!fixture.next_root.join(&relative_directory).exists());
    assert!(fixture
        .next_root
        .join(format!(
            ".atsumi-quarantine/classic-import/{}",
            rolled_back.import_id
        ))
        .join(&relative_directory)
        .is_dir());
    assert!(application
        .favorites_list()
        .expect("load favorites after rollback")
        .is_empty());
    assert_eq!(
        application
            .download_entries_list(DownloadListRequest {
                state: None,
                query: None,
                page: 1,
                page_size: 20,
            })
            .expect("load downloads after rollback")
            .total_items,
        0
    );
    assert_classic_unchanged(&fixture);
    drop(repository);
}

#[test]
fn interrupted_classic_copy_is_quarantined_on_startup_recovery() {
    let temporary = tempfile::tempdir().expect("create recovery fixture");
    let fixture = create_classic_fixture(temporary.path());
    let (repository, _application, importer) =
        import_services(temporary.path(), &fixture.next_root);
    let report = importer
        .dry_run(ClassicImportDryRunRequest {
            data_root: path_text(&fixture.data_root),
            download_root: Some(path_text(&fixture.download_root)),
        })
        .expect("inventory Classic source");
    let applying = match repository
        .classic_import_begin_apply(&report.import_id, report.revision)
        .expect("enter applying state")
    {
        ClassicImportTransitionOutcome::Applied(value) => value,
        other => panic!("unexpected transition: {other:?}"),
    };
    repository
        .classic_import_copy_mark(
            &report.import_id,
            123,
            &format!("classic-{}-123", report.import_id),
            "gallery-123",
            0,
            0,
        )
        .expect("record planned destination");
    fs::create_dir_all(fixture.next_root.join("gallery-123")).expect("create partial output");
    fs::write(fixture.next_root.join("gallery-123/.partial"), b"partial")
        .expect("write partial output");

    assert_eq!(importer.recover_incomplete().expect("recover import"), 1);
    let recovered = importer
        .get(&report.import_id)
        .expect("load recovery report");
    assert_eq!(recovered.state, ClassicImportState::RolledBack);
    assert!(fixture
        .next_root
        .join(format!(
            ".atsumi-quarantine/classic-import/{}/gallery-123/.partial",
            report.import_id
        ))
        .is_file());
    assert!(!fixture.next_root.join("gallery-123").exists());
    assert_classic_unchanged(&fixture);
    assert_eq!(applying.report.state, ClassicImportState::Applying);
}

#[test]
fn classic_inventory_reports_conflicts_without_guessing_or_merging_folders() {
    let temporary = tempfile::tempdir().expect("create conflict fixture");
    let fixture = create_classic_fixture(temporary.path());
    let mut state: serde_json::Value =
        serde_json::from_slice(&fixture.state_bytes).expect("parse fixture state");
    state["atsumiSimilarExcludedGalleries"] = json!([123]);
    state["atsumiDownloads"]
        .as_array_mut()
        .expect("downloads array")
        .push(json!({
            "id": 456,
            "title": "Missing completed folder",
            "pages": 1,
            "status": "completed",
            "path": path_text(&fixture.download_root.join("does-not-exist"))
        }));
    fs::write(
        fixture.data_root.join("state.json"),
        serde_json::to_vec_pretty(&state).expect("serialize conflict state"),
    )
    .expect("write conflict state");

    let duplicate = fixture.download_root.join("duplicate-gallery");
    fs::create_dir_all(&duplicate).expect("create duplicate folder");
    fs::write(duplicate.join("001.png"), &fixture.page_bytes[0]).expect("write duplicate page 1");
    fs::write(duplicate.join("002.png"), &fixture.page_bytes[1]).expect("write duplicate page 2");
    fs::write(
        duplicate.join(".atsumi-download.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 2,
            "id": 123,
            "title": "Duplicate claim",
            "sourcePages": 2,
            "excludedPages": [],
            "expectedPages": 2,
            "completedPages": 2,
            "status": "completed"
        }))
        .expect("serialize duplicate manifest"),
    )
    .expect("write duplicate manifest");

    let (_repository, _application, importer) =
        import_services(temporary.path(), &fixture.next_root);
    let report = importer
        .dry_run(ClassicImportDryRunRequest {
            data_root: path_text(&fixture.data_root),
            download_root: Some(path_text(&fixture.download_root)),
        })
        .expect("inventory conflicts");
    let codes = report
        .conflicts
        .iter()
        .map(|conflict| conflict.code)
        .collect::<Vec<_>>();
    assert!(codes.contains(&ClassicConflictCode::DuplicateGalleryFolder));
    assert!(codes.contains(&ClassicConflictCode::HiddenGalleryHasFiles));
    assert!(codes.contains(&ClassicConflictCode::StateCompletedFolderMissing));
    assert_eq!(report.counts.galleries_eligible, 0);
    assert!(report
        .galleries
        .iter()
        .filter(|gallery| gallery.gallery_id == 123)
        .all(|gallery| !gallery.eligible));
}

#[test]
fn classic_unknown_source_manifest_schema_is_a_blocking_conflict() {
    let temporary = tempfile::tempdir().expect("create unknown-schema fixture");
    let fixture = create_classic_fixture(temporary.path());
    let manifest = fixture
        .download_root
        .join("legacy-gallery")
        .join(".atsumi-download.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(&manifest).unwrap()).unwrap();
    value["schema"] = json!(999);
    fs::write(&manifest, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    let (_repository, _application, importer) =
        import_services(temporary.path(), &fixture.next_root);

    let report = importer
        .dry_run(ClassicImportDryRunRequest {
            data_root: path_text(&fixture.data_root),
            download_root: Some(path_text(&fixture.download_root)),
        })
        .unwrap();

    assert_eq!(report.counts.galleries_eligible, 0);
    assert!(report
        .conflicts
        .iter()
        .any(|conflict| conflict.code == ClassicConflictCode::ManifestInvalid));
}

#[test]
fn classic_rollback_fails_if_copied_artifact_and_quarantine_are_both_missing() {
    let temporary = tempfile::tempdir().expect("create rollback-loss fixture");
    let fixture = create_classic_fixture(temporary.path());
    let (_repository, _application, importer) =
        import_services(temporary.path(), &fixture.next_root);
    let dry_run = importer
        .dry_run(ClassicImportDryRunRequest {
            data_root: path_text(&fixture.data_root),
            download_root: Some(path_text(&fixture.download_root)),
        })
        .unwrap();
    let relative_directory = dry_run.galleries[0].relative_directory.clone().unwrap();
    let applied = importer
        .apply(ClassicImportApplyRequest {
            import_id: dry_run.import_id,
            expected_revision: dry_run.revision,
            accepted_conflict_ids: Vec::new(),
        })
        .unwrap();
    fs::remove_dir_all(fixture.next_root.join(relative_directory)).unwrap();

    let error = importer
        .rollback(ClassicImportRollbackRequest {
            import_id: applied.report.import_id,
            expected_revision: applied.report.revision,
        })
        .expect_err("missing copied data must not be reported as rolled back");

    assert!(
        matches!(error, ApplicationError::ClassicImportInvalid(message) if message.contains("both missing"))
    );
}

fn import_services(
    root: &Path,
    next_root: &Path,
) -> (
    Arc<SqliteRepository>,
    ApplicationService,
    ClassicImportService,
) {
    let repository = Arc::new(
        SqliteRepository::open(root.join("next.sqlite3")).expect("open Next test database"),
    );
    let application = ApplicationService::new(repository.clone())
        .with_download_repository(repository.clone() as Arc<dyn DownloadRepository>)
        .with_automation_repository(repository.clone() as Arc<dyn AutomationRepository>);
    let settings = application.settings_get().expect("load settings");
    application
        .settings_update(
            SettingsPatch {
                download_root: Some(path_text(next_root)),
                ..SettingsPatch::default()
            },
            settings.revision,
        )
        .expect("set Next download root");
    let classic_repository: Arc<dyn ClassicImportRepository> = repository.clone();
    let state_repository: Arc<dyn StateRepository> = repository.clone();
    let inspector: Arc<dyn ClassicSourceInspector> = Arc::new(FilesystemClassicSource::new());
    let artifact_store: Arc<dyn ArtifactStore> = Arc::new(FilesystemArtifactStore::new());
    let importer = ClassicImportService::new(
        classic_repository,
        state_repository,
        inspector,
        artifact_store,
    );
    (repository, application, importer)
}

fn create_classic_fixture(root: &Path) -> ClassicFixture {
    let data_root = root.join("ClassicData");
    let download_root = root.join("ClassicDownloads");
    let gallery_root = download_root.join("legacy-gallery");
    let next_root = root.join("NextDownloads");
    fs::create_dir_all(&data_root).expect("create Classic data root");
    fs::create_dir_all(&gallery_root).expect("create Classic gallery root");
    fs::create_dir_all(&next_root).expect("create Next root");
    let page_bytes = vec![png_bytes([180, 60, 30, 255]), png_bytes([20, 90, 170, 255])];
    fs::write(gallery_root.join("001.png"), &page_bytes[0]).expect("write Classic page 1");
    fs::write(gallery_root.join("002.png"), &page_bytes[1]).expect("write Classic page 2");
    fs::write(
        gallery_root.join(".atsumi-download.json"),
        serde_json::to_vec_pretty(&json!({
            "schema": 2,
            "id": 123,
            "title": "Classic fixture",
            "artist": "alpha",
            "sourcePages": 2,
            "excludedPages": [],
            "expectedPages": 2,
            "completedPages": 2,
            "status": "completed"
        }))
        .expect("serialize Classic manifest"),
    )
    .expect("write Classic manifest");
    let state_bytes = serde_json::to_vec_pretty(&json!({
        "atsumiDownloads": [{
            "id": 123,
            "title": "Classic fixture",
            "artist": "alpha",
            "groups": ["archive"],
            "pages": 2,
            "status": "completed",
            "path": path_text(&gallery_root.canonicalize().expect("resolve gallery root"))
        }],
        "atsumiFavorites": {
            "artists": ["alpha"],
            "groups": [],
            "series": [],
            "characters": [],
            "tags": []
        },
        "atsumiExcludedGalleries": [999]
    }))
    .expect("serialize Classic state");
    fs::write(data_root.join("state.json"), &state_bytes).expect("write Classic state");
    ClassicFixture {
        data_root,
        download_root,
        next_root,
        state_bytes,
        page_bytes,
    }
}

fn png_bytes(color: [u8; 4]) -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_pixel(4, 3, Rgba(color)));
    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, ImageFormat::Png)
        .expect("encode PNG fixture");
    output.into_inner()
}

fn assert_classic_unchanged(fixture: &ClassicFixture) {
    assert_eq!(
        fs::read(fixture.data_root.join("state.json")).expect("read Classic state"),
        fixture.state_bytes
    );
    for (index, expected) in fixture.page_bytes.iter().enumerate() {
        assert_eq!(
            fs::read(
                fixture
                    .download_root
                    .join("legacy-gallery")
                    .join(format!("{:03}.png", index + 1))
            )
            .expect("read Classic page"),
            *expected
        );
    }
}

fn path_text(path: &Path) -> String {
    path.to_str().expect("fixture path is Unicode").to_owned()
}
