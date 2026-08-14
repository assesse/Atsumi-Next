use std::{sync::mpsc::Sender, time::Duration};

use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::{
    application::{ApplicationError, ApplicationService},
    domain::{
        DownloadChangedEvent, DownloadEntry, DownloadListRequest, DownloadPage,
        FixtureDownloadJobStep, GalleryDetail, GalleryPage, JobRef, SearchRequest,
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
}

impl AppState {
    pub fn new(
        service: ApplicationService,
        thumbnails: ThumbnailCoordinator,
        thumbnail_completions: Sender<ThumbnailCompletionEventDto>,
    ) -> Self {
        Self {
            service,
            thumbnails,
            thumbnail_completions,
        }
    }
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
    Ok(state.service.search_submit(request).into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn download_queue_add(
    app: AppHandle,
    state: State<'_, AppState>,
    galleries: Vec<i64>,
    request_id: String,
) -> Result<ApiResult<Vec<DownloadEntry>>, ApiError> {
    match state.service.download_queue_add(galleries, request_id) {
        Ok(launch) => {
            for descriptor in launch.fixture_jobs {
                let service = state.service.clone();
                tauri::async_runtime::spawn(run_fixture_download_job(
                    app.clone(),
                    service,
                    descriptor.job_id,
                    descriptor.worker_attempt,
                ));
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
    app: AppHandle,
    state: State<'_, AppState>,
    entry_ids: Vec<String>,
) -> Result<ApiResult<Vec<JobRef>>, ApiError> {
    match state.service.download_retry(entry_ids) {
        Ok(job_refs) => {
            for job_ref in &job_refs {
                if job_ref.reused {
                    continue;
                }
                let service = state.service.clone();
                tauri::async_runtime::spawn(run_fixture_download_job(
                    app.clone(),
                    service,
                    job_ref.job_id.clone(),
                    job_ref.worker_attempt,
                ));
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
    match state.service.download_cancel(entry_ids) {
        Ok(entries) => {
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
pub async fn app_quit(app: AppHandle) -> Result<ApiResult<()>, ApiError> {
    app.exit(0);
    Ok(ApiResult::success(()))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_page_get(
    state: State<'_, AppState>,
    query_id: String,
    page: u32,
) -> Result<ApiResult<GalleryPage>, ApiError> {
    Ok(state.service.search_page_get(query_id, page).into())
}

#[tauri::command(rename_all = "camelCase")]
pub async fn gallery_detail_get(
    state: State<'_, AppState>,
    gallery_id: i64,
) -> Result<ApiResult<GalleryDetail>, ApiError> {
    Ok(state.service.gallery_detail_get(gallery_id).into())
}

async fn run_fixture_download_job(
    app: AppHandle,
    service: ApplicationService,
    job_id: String,
    worker_attempt: u64,
) {
    let steps = [
        (
            Duration::from_millis(75),
            FixtureDownloadJobStep::ResolvingMetadata,
        ),
        (
            Duration::from_millis(150),
            FixtureDownloadJobStep::FoundationUnavailable,
        ),
    ];

    for (delay, step) in steps {
        tokio::time::sleep(delay).await;
        match service.fixture_download_job_advance(&job_id, worker_attempt, step) {
            Ok(projection) => {
                if let Err(error) = app.emit("job:changed", &projection.job) {
                    tracing::warn!(job_id, error = %error, "could not emit job:changed");
                }
                if let Err(error) = app.emit("download:changed", &projection.download) {
                    tracing::warn!(job_id, error = %error, "could not emit download:changed");
                }
            }
            Err(error) => {
                log_fixture_download_job_error(&job_id, &error);
                break;
            }
        }
    }
}

fn log_fixture_download_job_error(job_id: &str, error: &ApplicationError) {
    tracing::error!(
        operation_id = "fixture_download_job",
        job_id,
        stage = "advance",
        error_code = "FIXTURE_DOWNLOAD_JOB_ADVANCE_FAILED",
        error = %error,
        "fixture download job stopped"
    );
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
