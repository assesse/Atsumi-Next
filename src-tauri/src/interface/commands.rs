use std::{
    path::PathBuf,
    sync::{mpsc::Sender, Arc},
};

use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::{
    application::{
        ApplicationError, ApplicationService, ArtifactStore, AutoFindSupervisor,
        DownloadPipelineError, DownloadPipelineErrorCode, DownloadRootPicker, DownloadSupervisor,
        ReconcileReport,
    },
    domain::{
        AutoFindExclusionResult, AutoFindRun, AutoFindSnapshot, DownloadChangedEvent,
        DownloadEntry, DownloadListRequest, DownloadPage, FavoriteKey, FavoriteMutationResult,
        FavoriteRecord, GalleryDetail, GalleryPage, JobRef, SearchHistoryEntry, SearchRequest,
        SearchSubmission, SettingsPatch, SettingsSnapshot, WindowPlacement,
        WindowPlacementSnapshot,
    },
    thumbnail::{
        ThumbnailCompletionEventDto, ThumbnailCoordinator, ThumbnailCoordinatorError,
        ThumbnailInvalidationDto, ThumbnailKey, ThumbnailPriority, ThumbnailRequestDto,
        ThumbnailRequestTokenDto, ThumbnailRuntimeConfigDto, ThumbnailWorkerStatsDto,
    },
};

use super::{ApiError, ApiResult};

pub struct AppState {
    service: ApplicationService,
    thumbnails: ThumbnailCoordinator,
    thumbnail_completions: Sender<ThumbnailCompletionEventDto>,
    downloads: DownloadSupervisor,
    auto_find: AutoFindSupervisor,
    download_root_picker: Arc<dyn DownloadRootPicker>,
    artifact_store: Arc<dyn ArtifactStore>,
}

impl AppState {
    pub fn new(
        service: ApplicationService,
        thumbnails: ThumbnailCoordinator,
        thumbnail_completions: Sender<ThumbnailCompletionEventDto>,
        downloads: DownloadSupervisor,
        auto_find: AutoFindSupervisor,
        download_root_picker: Arc<dyn DownloadRootPicker>,
        artifact_store: Arc<dyn ArtifactStore>,
    ) -> Self {
        Self {
            service,
            thumbnails,
            thumbnail_completions,
            downloads,
            auto_find,
            download_root_picker,
            artifact_store,
        }
    }
}

#[tauri::command]
pub async fn favorites_list(
    state: State<'_, AppState>,
) -> Result<ApiResult<Vec<FavoriteRecord>>, ApiError> {
    Ok(state.service.favorites_list().into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn favorite_set(
    state: State<'_, AppState>,
    key: FavoriteKey,
    enabled: bool,
) -> Result<ApiResult<FavoriteMutationResult>, ApiError> {
    Ok(state.service.favorite_set(key, enabled).into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_history_list(
    state: State<'_, AppState>,
    limit: u32,
) -> Result<ApiResult<Vec<SearchHistoryEntry>>, ApiError> {
    Ok(state.service.search_history_list(limit).into())
}

#[tauri::command]
pub async fn auto_find_snapshot(
    state: State<'_, AppState>,
) -> Result<ApiResult<AutoFindSnapshot>, ApiError> {
    Ok(state.service.auto_find_snapshot().into())
}

#[tauri::command]
pub async fn auto_find_refresh(
    state: State<'_, AppState>,
) -> Result<ApiResult<AutoFindRun>, ApiError> {
    Ok(state.auto_find.refresh().into())
}

#[tauri::command]
pub async fn auto_find_cancel(
    state: State<'_, AppState>,
) -> Result<ApiResult<AutoFindRun>, ApiError> {
    Ok(state.auto_find.cancel().into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn auto_find_exclude(
    state: State<'_, AppState>,
    gallery_ids: Vec<i64>,
    reason: String,
) -> Result<ApiResult<AutoFindExclusionResult>, ApiError> {
    Ok(state.service.auto_find_exclude(gallery_ids, reason).into())
}

#[tauri::command]
pub async fn settings_get(
    state: State<'_, AppState>,
) -> Result<ApiResult<SettingsSnapshot>, ApiError> {
    Ok(state.service.settings_get().into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn settings_update(
    app: AppHandle,
    state: State<'_, AppState>,
    patch: SettingsPatch,
    expected_revision: u64,
) -> Result<ApiResult<SettingsSnapshot>, ApiError> {
    match state.service.settings_update(patch, expected_revision) {
        Ok(snapshot) => {
            if let Err(error) = state.thumbnails.reconfigure(ThumbnailRuntimeConfigDto {
                concurrent_image_requests: snapshot.concurrent_image_requests,
                request_start_interval_ms: snapshot.request_start_interval_ms,
            }) {
                tracing::warn!(error = %error, "could not apply thumbnail worker settings");
            }
            if let Err(error) = app.emit("settings:changed", &snapshot) {
                tracing::warn!(error = %error, "could not emit settings:changed");
            }
            Ok(ApiResult::success(snapshot))
        }
        Err(error) => Ok(ApiResult::failure(error.into())),
    }
}

#[tauri::command]
pub async fn window_placement_get(
    state: State<'_, AppState>,
) -> Result<ApiResult<WindowPlacementSnapshot>, ApiError> {
    Ok(state.service.window_placement_get().into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn window_placement_update(
    state: State<'_, AppState>,
    placement: WindowPlacement,
    expected_revision: u64,
) -> Result<ApiResult<WindowPlacementSnapshot>, ApiError> {
    Ok(state
        .service
        .window_placement_update(placement, expected_revision)
        .into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_submit(
    state: State<'_, AppState>,
    request: SearchRequest,
) -> Result<ApiResult<SearchSubmission>, ApiError> {
    let service = state.service.clone();
    Ok(run_application_blocking("search_submit", move || service.search_submit(request)).await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_queue_add(
    app: AppHandle,
    state: State<'_, AppState>,
    galleries: Vec<i64>,
    request_id: String,
) -> Result<ApiResult<Vec<DownloadEntry>>, ApiError> {
    if let Err(error) = ensure_download_root(
        &app,
        state.service.clone(),
        Arc::clone(&state.download_root_picker),
        Arc::clone(&state.artifact_store),
    )
    .await
    {
        return Ok(ApiResult::failure(error));
    }
    match state.service.download_queue_add(galleries, request_id) {
        Ok(launch) => {
            if let Err(error) = state.downloads.enqueue_all(launch.jobs) {
                return Ok(ApiResult::failure(ApplicationError::from(error).into()));
            }
            Ok(ApiResult::success(launch.entries))
        }
        Err(error) => Ok(ApiResult::failure(error.into())),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_entries_list(
    state: State<'_, AppState>,
    request: DownloadListRequest,
) -> Result<ApiResult<DownloadPage>, ApiError> {
    Ok(state.service.download_entries_list(request).into())
}

#[tauri::command]
pub async fn download_active_count(state: State<'_, AppState>) -> Result<ApiResult<u64>, ApiError> {
    Ok(state.service.download_active_count().into())
}

#[tauri::command(rename_all = "camelCase")]
pub fn thumbnail_request(
    state: State<'_, AppState>,
    request: ThumbnailRequestDto,
) -> Result<ApiResult<ThumbnailRequestTokenDto>, ApiError> {
    match state
        .thumbnails
        .request_with_completion(request, state.thumbnail_completions.clone())
    {
        Ok(token) => Ok(ApiResult::success(token)),
        Err(error) => Ok(ApiResult::failure(thumbnail_coordinator_error(error))),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub fn thumbnail_cancel(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<ApiResult<bool>, ApiError> {
    Ok(ApiResult::success(
        state.thumbnails.cancel(request_id.trim()),
    ))
}

#[tauri::command(rename_all = "camelCase")]
pub fn thumbnail_reprioritize(
    state: State<'_, AppState>,
    request_id: String,
    priority: ThumbnailPriority,
) -> Result<ApiResult<bool>, ApiError> {
    Ok(ApiResult::success(
        state.thumbnails.reprioritize(request_id.trim(), priority),
    ))
}

#[tauri::command(rename_all = "camelCase")]
pub fn thumbnail_invalidate(
    state: State<'_, AppState>,
    key: ThumbnailKey,
) -> Result<ApiResult<ThumbnailInvalidationDto>, ApiError> {
    match state.thumbnails.invalidate(&key) {
        Ok(result) => Ok(ApiResult::success(result)),
        Err(error) => Ok(ApiResult::failure(ApiError {
            code: "THUMBNAIL_REQUEST_INVALID".into(),
            message: error.to_string(),
            retryable: false,
            action: Some(super::ApiAction::None),
            details: None,
        })),
    }
}

#[tauri::command]
pub fn thumbnail_stats(
    state: State<'_, AppState>,
) -> Result<ApiResult<ThumbnailWorkerStatsDto>, ApiError> {
    Ok(ApiResult::success(state.thumbnails.stats()))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_retry(
    state: State<'_, AppState>,
    entry_ids: Vec<String>,
) -> Result<ApiResult<Vec<JobRef>>, ApiError> {
    match state.service.download_retry(entry_ids) {
        Ok(job_refs) => {
            if let Err(error) = state.downloads.enqueue_retries(&job_refs) {
                return Ok(ApiResult::failure(error.into()));
            }
            Ok(ApiResult::success(job_refs))
        }
        Err(error) => Ok(ApiResult::failure(error.into())),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_cancel(
    app: AppHandle,
    state: State<'_, AppState>,
    entry_ids: Vec<String>,
) -> Result<ApiResult<Vec<DownloadEntry>>, ApiError> {
    let cancellation_ids = entry_ids.clone();
    match state.service.download_cancel(entry_ids) {
        Ok(entries) => {
            state.downloads.cancel_entries(&cancellation_ids);
            for entry in &entries {
                let event = DownloadChangedEvent {
                    entry_id: entry.entry_id.to_string(),
                    gallery_id: entry.gallery_id.get(),
                    revision: entry.revision,
                    state: entry.state,
                    progress: entry.progress,
                    attempt: entry.attempt,
                    error_code: entry.error_code.clone(),
                    error_message: entry.error_message.clone(),
                };
                if let Err(error) = app.emit("download:changed", event) {
                    tracing::warn!(
                        entry_id = %entry.entry_id,
                        error = %error,
                        "could not emit download:changed"
                    );
                }
            }
            Ok(ApiResult::success(entries))
        }
        Err(error) => Ok(ApiResult::failure(error.into())),
    }
}

#[tauri::command(rename_all = "camelCase")]
pub async fn artifact_open_first(
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<ApiResult<()>, ApiError> {
    let downloads = state.downloads.clone();
    Ok(run_application_blocking("artifact_open_first", move || {
        downloads.open_first(entry_id)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_quarantine(
    state: State<'_, AppState>,
    entry_ids: Vec<String>,
    reason: String,
) -> Result<ApiResult<Vec<DownloadEntry>>, ApiError> {
    let downloads = state.downloads.clone();
    Ok(run_application_blocking("download_quarantine", move || {
        downloads.quarantine_entries(entry_ids, reason)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_quarantine_undo(
    state: State<'_, AppState>,
    entry_ids: Vec<String>,
) -> Result<ApiResult<Vec<DownloadEntry>>, ApiError> {
    let downloads = state.downloads.clone();
    Ok(
        run_application_blocking("download_quarantine_undo", move || {
            downloads.restore_entries(entry_ids)
        })
        .await,
    )
}

#[tauri::command]
pub async fn app_reconcile(
    state: State<'_, AppState>,
) -> Result<ApiResult<ReconcileReport>, ApiError> {
    let downloads = state.downloads.clone();
    Ok(run_application_blocking("app_reconcile", move || downloads.reconcile()).await)
}

#[tauri::command]
pub async fn app_minimize_to_tray(window: WebviewWindow) -> Result<ApiResult<()>, ApiError> {
    match window.hide() {
        Ok(()) => Ok(ApiResult::success(())),
        Err(error) => Ok(ApiResult::failure(ApiError {
            code: "WINDOW_HIDE_FAILED".into(),
            message: format!("could not hide Atsumi Next to the tray: {error}"),
            retryable: true,
            action: Some(super::ApiAction::Retry),
            details: None,
        })),
    }
}

#[tauri::command]
pub async fn app_quit(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ApiResult<()>, ApiError> {
    let downloads = state.downloads.clone();
    let auto_find = state.auto_find.clone();
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
        auto_find.shutdown_and_wait();
        downloads.shutdown_and_wait();
    })
    .await
    {
        tracing::warn!(error = %error, "download workers did not finish shutdown cleanly");
    }
    app.exit(0);
    Ok(ApiResult::success(()))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_page_get(
    state: State<'_, AppState>,
    query_id: String,
    page: u32,
) -> Result<ApiResult<GalleryPage>, ApiError> {
    let service = state.service.clone();
    Ok(run_application_blocking("search_page_get", move || {
        service.search_page_get(query_id, page)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn gallery_detail_get(
    state: State<'_, AppState>,
    gallery_id: i64,
) -> Result<ApiResult<GalleryDetail>, ApiError> {
    let service = state.service.clone();
    Ok(run_application_blocking("gallery_detail_get", move || {
        service.gallery_detail_get(gallery_id)
    })
    .await)
}

async fn ensure_download_root(
    app: &AppHandle,
    service: ApplicationService,
    picker: Arc<dyn DownloadRootPicker>,
    store: Arc<dyn ArtifactStore>,
) -> Result<(), ApiError> {
    let current = service.settings_get().map_err(ApiError::from)?;
    if !current.download_root.trim().is_empty() {
        let root = PathBuf::from(current.download_root);
        return match tauri::async_runtime::spawn_blocking(move || {
            store.validate_download_root(&root)
        })
        .await
        {
            Ok(Ok(_)) => Ok(()),
            Ok(Err(error)) => Err(ApplicationError::from(error).into()),
            Err(error) => Err(blocking_task_error("download_root_validate", &error)),
        };
    }

    let selected = match tauri::async_runtime::spawn_blocking(move || {
        let selected = picker.pick_download_root()?;
        selected
            .map(|path| store.validate_download_root(&path))
            .transpose()
    })
    .await
    {
        Ok(Ok(selected)) => selected,
        Ok(Err(error)) => return Err(ApplicationError::from(error).into()),
        Err(error) => return Err(blocking_task_error("download_root_choose", &error)),
    };
    let Some(selected) = selected else {
        return Err(ApplicationError::from(DownloadPipelineError::new(
            DownloadPipelineErrorCode::RootSelectionCancelled,
            "Download folder selection was cancelled; no queue entry was created",
            false,
        ))
        .into());
    };
    let selected = selected.to_str().ok_or_else(|| {
        ApiError::from(ApplicationError::from(DownloadPipelineError::new(
            DownloadPipelineErrorCode::RootUnavailable,
            "The selected folder path cannot be represented safely",
            false,
        )))
    })?;
    let updated = service
        .settings_update(
            SettingsPatch {
                download_root: Some(selected.to_owned()),
                ..SettingsPatch::default()
            },
            current.revision,
        )
        .map_err(ApiError::from)?;
    if let Err(error) = app.emit("settings:changed", &updated) {
        tracing::warn!(error = %error, "could not emit settings:changed after folder selection");
    }
    Ok(())
}

fn blocking_task_error(operation_id: &'static str, error: &tauri::Error) -> ApiError {
    tracing::error!(operation_id, error = %error, "blocking backend task did not complete");
    ApiError {
        code: "BACKEND_TASK_FAILED".into(),
        message: "The backend could not complete the request".into(),
        retryable: true,
        action: Some(super::ApiAction::Retry),
        details: None,
    }
}

async fn run_application_blocking<T, F>(operation_id: &'static str, operation: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, ApplicationError> + Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(operation).await {
        Ok(result) => result.into(),
        Err(error) => {
            let (cancelled, panicked) = match &error {
                tauri::Error::JoinError(join_error) => {
                    (join_error.is_cancelled(), join_error.is_panic())
                }
                _ => (false, false),
            };
            tracing::error!(
                operation_id,
                cancelled,
                panicked,
                "blocking application task did not complete"
            );
            ApiResult::failure(ApiError {
                code: "BACKEND_TASK_FAILED".into(),
                message: "The backend could not complete the request".into(),
                retryable: true,
                action: Some(super::ApiAction::Retry),
                details: None,
            })
        }
    }
}

fn thumbnail_coordinator_error(error: ThumbnailCoordinatorError) -> ApiError {
    let (code, retryable) = match &error {
        ThumbnailCoordinatorError::InvalidConfiguration(_)
        | ThumbnailCoordinatorError::InvalidKey(_) => ("THUMBNAIL_REQUEST_INVALID", false),
        ThumbnailCoordinatorError::Closed => ("THUMBNAIL_COORDINATOR_CLOSED", true),
        ThumbnailCoordinatorError::WorkerStart(_) => ("THUMBNAIL_WORKER_UNAVAILABLE", true),
    };
    ApiError {
        code: code.into(),
        message: error.to_string(),
        retryable,
        action: Some(if retryable {
            super::ApiAction::Retry
        } else {
            super::ApiAction::None
        }),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::domain::ValidationError;

    #[test]
    fn application_blocking_helper_runs_off_the_calling_thread() {
        let caller = thread::current().id();
        let result = tauri::async_runtime::block_on(run_application_blocking(
            "test_blocking_boundary",
            || Ok(thread::current().id()),
        ));

        match result {
            ApiResult::Success(worker) => assert_ne!(worker, caller),
            ApiResult::Failure(error) => panic!("blocking call unexpectedly failed: {error:?}"),
        }
    }

    #[test]
    fn application_blocking_helper_preserves_api_errors() {
        let result =
            tauri::async_runtime::block_on(run_application_blocking("test_error_boundary", || {
                Err::<(), _>(
                    ValidationError::new("query", "must not contain unsupported syntax").into(),
                )
            }));

        match result {
            ApiResult::Failure(error) => {
                assert_eq!(error.code, "VALIDATION_ERROR");
                assert!(!error.retryable);
            }
            ApiResult::Success(()) => panic!("validation error unexpectedly succeeded"),
        }
    }
}
