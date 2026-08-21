pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;
pub mod source;
pub mod thumbnail;

#[cfg(test)]
mod tests;

use std::{
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use application::{
    ApplicationService, ArtifactRepository, ArtifactStore, AutoFindSource, AutoFindSupervisor,
    AutomationRepository, DisabledDuplicateRelationProvider, DownloadPipelineRepository,
    DownloadSourcePort, DownloadSupervisor, DuplicateRepository, DuplicateSupervisor,
    InternalDuplicateRepository, InternalDuplicateSupervisor, StateRepository,
};
use domain::{AutoFindRun, DownloadJobProjection, DuplicateScanRun, InternalScanRun};
use infrastructure::{
    CompositeThumbnailResolver, FilesystemArtifactStore, HitomiLiveAdapter, HitomiLiveConfig,
    SqliteRepository, WindowsFolderPicker,
};
use interface::AppState;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconEvent},
    Emitter, Manager,
};
use thumbnail::{
    ThumbnailCompletionEventDto, ThumbnailCoordinator, ThumbnailCoordinatorConfig,
    ThumbnailResolver,
};

fn apply_pending_factory_reset(data_dir: &std::path::Path) -> std::io::Result<()> {
    let marker = data_dir.join("factory-reset.pending");
    if !marker.exists() {
        return Ok(());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let backup = data_dir.join(format!("factory-reset-backup-{stamp}"));
    std::fs::create_dir_all(&backup)?;
    for suffix in ["", "-wal", "-shm"] {
        let source = data_dir.join(format!("atsumi-next.sqlite3{suffix}"));
        if source.exists() {
            std::fs::rename(&source, backup.join(format!("atsumi-next.sqlite3{suffix}")))?;
        }
    }
    std::fs::remove_file(marker)?;
    Ok(())
}

pub fn run() -> tauri::Result<()> {
    infrastructure::telemetry::init();

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = window
                    .show()
                    .and_then(|_| window.unminimize())
                    .and_then(|_| window.set_focus())
                {
                    tracing::warn!(error = %error, "could not focus the existing Atsumi Next window");
                }
            }
        }))
        .on_tray_icon_event(|app, event| {
            let restore = matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }
            );
            if !restore {
                return;
            }
            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) = window
                    .show()
                    .and_then(|_| window.unminimize())
                    .and_then(|_| window.set_focus())
                {
                    tracing::warn!(error = %error, "could not restore Atsumi Next from the tray");
                }
            }
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = window.emit("app:exit-requested", ()) {
                    tracing::warn!(error = %error, "could not request the exit confirmation dialog");
                }
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            apply_pending_factory_reset(&data_dir)?;
            let database_path = data_dir.join("atsumi-next.sqlite3");
            let repository = SqliteRepository::open(&database_path)?;
            let repository = Arc::new(repository);
            let settings = ApplicationService::new(repository.clone()).settings_get()?;
            let download_root_configured = !settings.download_root.trim().is_empty();
            let live_source = Arc::new(HitomiLiveAdapter::new(HitomiLiveConfig {
                max_concurrent_requests: settings.concurrent_image_requests as usize,
                request_start_interval: Duration::from_millis(
                    settings.request_start_interval_ms,
                ),
                ..HitomiLiveConfig::default()
            })?);
            let service = ApplicationService::new(repository.clone())
                .with_download_repository(repository.clone())
                .with_search_repository(live_source.clone())
                .with_automation_repository(repository.clone())
                .with_tag_catalog(repository.clone(), live_source.clone());
            let recovered_entries = service.download_recover_interrupted()?;
            let automation_repository: Arc<dyn AutomationRepository> = repository.clone();
            let auto_find_settings: Arc<dyn StateRepository> = repository.clone();
            let auto_find_source: Arc<dyn AutoFindSource> = live_source.clone();
            let (auto_find_event_tx, auto_find_event_rx) = mpsc::channel::<AutoFindRun>();
            let auto_find = AutoFindSupervisor::new(
                automation_repository,
                auto_find_settings,
                auto_find_source,
                auto_find_event_tx,
            );
            let recovered_auto_find_runs = auto_find.recover_interrupted()?;
            let auto_find_app = app.handle().clone();
            thread::Builder::new()
                .name("atsumi-auto-find-events".into())
                .spawn(move || {
                    while let Ok(run) = auto_find_event_rx.recv() {
                        if let Err(error) = auto_find_app.emit("auto-find:changed", &run) {
                            tracing::warn!(error = %error, "could not emit auto-find:changed");
                        }
                    }
                })?;
            let artifact_store: Arc<dyn ArtifactStore> =
                Arc::new(FilesystemArtifactStore::new());
            let thumbnail_config = ThumbnailCoordinatorConfig {
                max_concurrency: settings.concurrent_image_requests as usize,
                request_start_interval: Duration::from_millis(settings.request_start_interval_ms),
                ..ThumbnailCoordinatorConfig::default()
            };
            let remote_thumbnail_resolver: Arc<dyn ThumbnailResolver> = live_source.clone();
            let artifact_repository: Arc<dyn ArtifactRepository> = repository.clone();
            let thumbnail_settings: Arc<dyn StateRepository> = repository.clone();
            let thumbnail_resolver: Arc<dyn ThumbnailResolver> = Arc::new(
                CompositeThumbnailResolver::new(
                    remote_thumbnail_resolver,
                    Arc::clone(&artifact_repository),
                    thumbnail_settings,
                    Arc::clone(&artifact_store),
                ),
            );
            let thumbnails = ThumbnailCoordinator::new(thumbnail_resolver, thumbnail_config)?;
            let (thumbnail_completion_tx, thumbnail_completion_rx) =
                mpsc::channel::<ThumbnailCompletionEventDto>();
            let thumbnail_app = app.handle().clone();
            thread::Builder::new()
                .name("atsumi-thumbnail-events".into())
                .spawn(move || {
                    while let Ok(event) = thumbnail_completion_rx.recv() {
                        if let Err(error) = thumbnail_app.emit("thumbnail:ready", &event) {
                            tracing::warn!(error = %error, "could not emit thumbnail:ready");
                        }
                    }
                })?;
            let duplicate_repository: Arc<dyn DuplicateRepository> = repository.clone();
            let duplicate_settings: Arc<dyn StateRepository> = repository.clone();
            let (duplicate_event_tx, duplicate_event_rx) = mpsc::channel::<DuplicateScanRun>();
            let duplicates = DuplicateSupervisor::new(
                Arc::clone(&duplicate_repository),
                duplicate_settings,
                Arc::clone(&artifact_store),
                Arc::new(DisabledDuplicateRelationProvider),
                duplicate_event_tx,
            );
            let recovered_duplicate_runs = duplicates.recover_interrupted()?;
            let duplicate_app = app.handle().clone();
            thread::Builder::new()
                .name("atsumi-duplicate-events".into())
                .spawn(move || {
                    while let Ok(run) = duplicate_event_rx.recv() {
                        if let Err(error) = duplicate_app.emit("duplicate:changed", &run) {
                            tracing::warn!(error = %error, "could not emit duplicate:changed");
                        }
                    }
                })?;
            let internal_repository: Arc<dyn InternalDuplicateRepository> = repository.clone();
            let internal_artifact_repository: Arc<dyn ArtifactRepository> = repository.clone();
            let internal_settings: Arc<dyn StateRepository> = repository.clone();
            let (internal_event_tx, internal_event_rx) = mpsc::channel::<InternalScanRun>();
            let internal_duplicates = InternalDuplicateSupervisor::new(
                internal_repository,
                duplicate_repository,
                internal_artifact_repository,
                internal_settings,
                Arc::clone(&artifact_store),
                internal_event_tx,
            );
            let recovered_internal_runs = internal_duplicates.recover_interrupted()?;
            let reconciled_internal_pages = if download_root_configured {
                match internal_duplicates.reconcile_pending_page_moves() {
                    Ok(count) => count,
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "startup internal page quarantine reconciliation was deferred"
                        );
                        0
                    }
                }
            } else {
                0
            };
            let internal_app = app.handle().clone();
            thread::Builder::new()
                .name("atsumi-internal-duplicate-events".into())
                .spawn(move || {
                    while let Ok(run) = internal_event_rx.recv() {
                        if let Err(error) = internal_app.emit("internal-duplicate:changed", &run) {
                            tracing::warn!(
                                error = %error,
                                "could not emit internal-duplicate:changed"
                            );
                        }
                    }
                })?;
            let download_repository: Arc<dyn DownloadPipelineRepository> = repository.clone();
            let settings_repository: Arc<dyn StateRepository> = repository.clone();
            let download_source: Arc<dyn DownloadSourcePort> = live_source.clone();
            let (download_event_tx, download_event_rx) =
                mpsc::channel::<DownloadJobProjection>();
            let download_app = app.handle().clone();
            thread::Builder::new()
                .name("atsumi-download-events".into())
                .spawn(move || {
                    while let Ok(projection) = download_event_rx.recv() {
                        if let Err(error) = download_app.emit("job:changed", &projection.job) {
                            tracing::warn!(error = %error, "could not emit job:changed");
                        }
                        if let Err(error) =
                            download_app.emit("download:changed", &projection.download)
                        {
                            tracing::warn!(error = %error, "could not emit download:changed");
                        }
                    }
                })?;
            let downloads = DownloadSupervisor::new(
                download_repository,
                settings_repository,
                download_source,
                Arc::clone(&artifact_store),
                download_event_tx,
                2,
            )?;
            let (startup_recovery_issues, resumed_jobs) =
                if download_root_configured {
                    match downloads.recover_startup_state() {
                        Ok(report) => (report.issues.len(), report.resumed_jobs),
                        Err(_) => {
                            tracing::warn!(
                                "startup download recovery was deferred; no ambiguous file was changed"
                            );
                            (1, 0)
                        }
                    }
                } else {
                    (0, 0)
                };
            app.manage(AppState::new(
                service,
                thumbnails,
                thumbnail_completion_tx,
                downloads,
                auto_find,
                duplicates,
                internal_duplicates,
                Arc::new(WindowsFolderPicker::new()),
                artifact_store,
                live_source.clone(),
                data_dir,
            ));
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.unminimize()?;
                window.set_focus()?;
            }
            tracing::info!(
                database_file = "atsumi-next.sqlite3",
                app_version = env!("CARGO_PKG_VERSION"),
                recovered_entries,
                recovered_auto_find_runs,
                recovered_duplicate_runs,
                recovered_internal_runs,
                reconciled_internal_pages,
                startup_recovery_issues,
                resumed_jobs,
                "Atsumi Next backend initialized"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            interface::commands::settings_get,
            interface::commands::settings_update,
            interface::commands::folder_name_template_preview,
            interface::commands::window_placement_get,
            interface::commands::window_placement_update,
            interface::commands::search_submit,
            interface::commands::search_page_get,
            interface::commands::search_page_cancel,
            interface::commands::gallery_detail_get,
            interface::commands::favorites_list,
            interface::commands::favorite_set,
            interface::commands::search_history_list,
            interface::commands::tag_catalog_status,
            interface::commands::tag_catalog_refresh,
            interface::commands::tag_suggestions_search,
            interface::commands::auto_find_snapshot,
            interface::commands::auto_find_refresh,
            interface::commands::auto_find_cancel,
            interface::commands::auto_find_exclude,
            interface::commands::exploration_data_reset,
            interface::commands::maintenance_preview,
            interface::commands::maintenance_execute,
            interface::commands::duplicate_snapshot,
            interface::commands::duplicate_scan_start,
            interface::commands::duplicate_scan_cancel,
            interface::commands::duplicate_review_get,
            interface::commands::duplicate_decision_apply,
            interface::commands::internal_duplicate_snapshot,
            interface::commands::internal_duplicate_scan_start,
            interface::commands::internal_duplicate_scan_cancel,
            interface::commands::internal_duplicate_review_get,
            interface::commands::internal_removal_plan,
            interface::commands::internal_removal_apply,
            interface::commands::internal_removal_undo,
            interface::commands::download_queue_add,
            interface::commands::download_entries_list,
            interface::commands::download_retry,
            interface::commands::download_cancel,
            interface::commands::download_quarantine,
            interface::commands::download_quarantine_undo,
            interface::commands::download_active_count,
            interface::commands::artifact_open_first,
            interface::commands::app_reconcile,
            interface::commands::thumbnail_request,
            interface::commands::thumbnail_cancel,
            interface::commands::thumbnail_invalidate,
            interface::commands::thumbnail_reprioritize,
            interface::commands::thumbnail_stats,
            interface::commands::thumbnail_cache_clear,
            interface::commands::app_minimize_to_tray,
            interface::commands::app_quit,
        ])
        .run(tauri::generate_context!());

    if let Err(ref error) = result {
        tracing::error!(error_type = %std::any::type_name_of_val(error), "Atsumi Next exited with an error");
    }
    result
}
