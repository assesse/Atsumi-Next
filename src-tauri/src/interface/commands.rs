use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{mpsc::Sender, Arc, Mutex},
};

use tauri::{AppHandle, Emitter, State, WebviewWindow};

use crate::{
    application::{
        ApplicationError, ApplicationService, ArtifactStore, AutoFindSupervisor,
        DownloadPipelineError, DownloadPipelineErrorCode, DownloadRootPicker, DownloadSupervisor,
        DuplicateSupervisor, InternalDuplicateSupervisor, ReconcileReport,
    },
    domain::{
        AutoFindExclusionResult, AutoFindRun, AutoFindSnapshot, DownloadChangedEvent,
        DownloadEntry, DownloadListRequest, DownloadPage, DuplicateDecisionRequest,
        DuplicateReview, DuplicateScanRun, DuplicateSnapshot, ExplorationDataResetRequest,
        ExplorationDataResetResult, FavoriteKey, FavoriteMutationResult, FavoriteRecord,
        GalleryDetail, GalleryPage, InternalDuplicateReview, InternalDuplicateSnapshot,
        InternalRemovalApplyRequest, InternalRemovalPlan, InternalRemovalPlanRequest,
        InternalRemovalResult, InternalRemovalUndoRequest, InternalScanRun, JobRef,
        MaintenanceAction, MaintenancePreview, MaintenanceResult, SearchHistoryEntry,
        SearchRequest, SearchSubmission, SettingsPatch, SettingsSnapshot, ValidationError,
        WindowPlacement, WindowPlacementSnapshot,
    },
    infrastructure::HitomiLiveAdapter,
    thumbnail::{
        CancellationToken, ThumbnailCacheClearDto, ThumbnailCompletionEventDto,
        ThumbnailCoordinator, ThumbnailCoordinatorError, ThumbnailInvalidationDto, ThumbnailKey,
        ThumbnailPriority, ThumbnailRequestDto, ThumbnailRequestTokenDto,
        ThumbnailRuntimeConfigDto, ThumbnailWorkerStatsDto,
    },
};

use super::{ApiError, ApiResult};

pub struct AppState {
    service: ApplicationService,
    thumbnails: ThumbnailCoordinator,
    thumbnail_completions: Sender<ThumbnailCompletionEventDto>,
    downloads: DownloadSupervisor,
    auto_find: AutoFindSupervisor,
    duplicates: DuplicateSupervisor,
    internal_duplicates: InternalDuplicateSupervisor,
    download_root_picker: Arc<dyn DownloadRootPicker>,
    artifact_store: Arc<dyn ArtifactStore>,
    live_source: Arc<HitomiLiveAdapter>,
    data_dir: PathBuf,
    search_pages: SearchPageRequests,
    maintenance_previews: Mutex<HashMap<String, MaintenanceAction>>,
}

#[derive(Default)]
struct SearchPageRequests {
    inner: Mutex<SearchPageRequestsInner>,
}

#[derive(Default)]
struct SearchPageRequestsInner {
    active: HashMap<String, CancellationToken>,
    cancelled: HashSet<String>,
    cancelled_order: VecDeque<String>,
}

impl SearchPageRequests {
    fn start(&self, request_id: &str) -> CancellationToken {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let token = CancellationToken::new();
        if inner.cancelled.remove(request_id) {
            inner
                .cancelled_order
                .retain(|candidate| candidate != request_id);
            token.cancel();
        }
        if let Some(previous) = inner.active.insert(request_id.to_owned(), token.clone()) {
            previous.cancel();
        }
        token
    }

    fn cancel(&self, request_id: &str) -> bool {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(token) = inner.active.get(request_id) {
            token.cancel();
            return true;
        }
        if inner.cancelled.insert(request_id.to_owned()) {
            inner.cancelled_order.push_back(request_id.to_owned());
        }
        while inner.cancelled_order.len() > 256 {
            if let Some(oldest) = inner.cancelled_order.pop_front() {
                inner.cancelled.remove(&oldest);
            }
        }
        true
    }

    fn finish(&self, request_id: &str) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.active.remove(request_id);
        inner.cancelled.remove(request_id);
        inner
            .cancelled_order
            .retain(|candidate| candidate != request_id);
    }

    fn cancel_all(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        for token in inner.active.values() {
            token.cancel();
        }
        inner.active.clear();
        inner.cancelled.clear();
        inner.cancelled_order.clear();
    }
}

impl AppState {
    // This is the single composition root for application services and ports.
    // Keeping dependencies explicit here makes production/test wiring auditable.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        service: ApplicationService,
        thumbnails: ThumbnailCoordinator,
        thumbnail_completions: Sender<ThumbnailCompletionEventDto>,
        downloads: DownloadSupervisor,
        auto_find: AutoFindSupervisor,
        duplicates: DuplicateSupervisor,
        internal_duplicates: InternalDuplicateSupervisor,
        download_root_picker: Arc<dyn DownloadRootPicker>,
        artifact_store: Arc<dyn ArtifactStore>,
        live_source: Arc<HitomiLiveAdapter>,
        data_dir: PathBuf,
    ) -> Self {
        Self {
            service,
            thumbnails,
            thumbnail_completions,
            downloads,
            auto_find,
            duplicates,
            internal_duplicates,
            download_root_picker,
            artifact_store,
            live_source,
            data_dir,
            search_pages: SearchPageRequests::default(),
            maintenance_previews: Mutex::new(HashMap::new()),
        }
    }

    fn remember_maintenance_preview(&self, action: MaintenanceAction) -> String {
        let id = format!("maintenance-{}", uuid::Uuid::new_v4());
        self.maintenance_previews
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(id.clone(), action);
        id
    }

    fn consume_maintenance_preview(&self, preview_id: &str, action: &MaintenanceAction) -> bool {
        self.maintenance_previews
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(preview_id)
            .is_some_and(|previewed| previewed == *action)
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

#[tauri::command(rename_all = "camelCase")]
pub async fn exploration_data_reset(
    state: State<'_, AppState>,
    request: ExplorationDataResetRequest,
) -> Result<ApiResult<ExplorationDataResetResult>, ApiError> {
    let service = state.service.clone();
    Ok(run_application_blocking("exploration_data_reset", move || {
        service.exploration_data_reset(request)
    })
    .await)
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
pub async fn duplicate_snapshot(
    state: State<'_, AppState>,
) -> Result<ApiResult<DuplicateSnapshot>, ApiError> {
    let duplicates = state.duplicates.clone();
    Ok(run_application_blocking("duplicate_snapshot", move || duplicates.snapshot()).await)
}

#[tauri::command]
pub async fn duplicate_scan_start(
    state: State<'_, AppState>,
) -> Result<ApiResult<DuplicateScanRun>, ApiError> {
    let duplicates = state.duplicates.clone();
    Ok(run_application_blocking("duplicate_scan_start", move || duplicates.start()).await)
}

#[tauri::command]
pub async fn duplicate_scan_cancel(
    state: State<'_, AppState>,
) -> Result<ApiResult<DuplicateScanRun>, ApiError> {
    let duplicates = state.duplicates.clone();
    Ok(run_application_blocking("duplicate_scan_cancel", move || duplicates.cancel()).await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn duplicate_review_get(
    state: State<'_, AppState>,
    candidate_id: String,
) -> Result<ApiResult<DuplicateReview>, ApiError> {
    let duplicates = state.duplicates.clone();
    Ok(run_application_blocking("duplicate_review_get", move || {
        duplicates.review_get(&candidate_id)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn duplicate_decision_apply(
    state: State<'_, AppState>,
    request: DuplicateDecisionRequest,
) -> Result<ApiResult<DuplicateReview>, ApiError> {
    let duplicates = state.duplicates.clone();
    Ok(
        run_application_blocking("duplicate_decision_apply", move || {
            duplicates.decision_apply(request)
        })
        .await,
    )
}

#[tauri::command]
pub async fn internal_duplicate_snapshot(
    state: State<'_, AppState>,
) -> Result<ApiResult<InternalDuplicateSnapshot>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(
        run_application_blocking("internal_duplicate_snapshot", move || supervisor.snapshot())
            .await,
    )
}

#[tauri::command]
pub async fn internal_duplicate_scan_start(
    state: State<'_, AppState>,
) -> Result<ApiResult<InternalScanRun>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(run_application_blocking("internal_duplicate_scan_start", move || supervisor.start()).await)
}

#[tauri::command]
pub async fn internal_duplicate_scan_cancel(
    state: State<'_, AppState>,
) -> Result<ApiResult<InternalScanRun>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(
        run_application_blocking("internal_duplicate_scan_cancel", move || {
            supervisor.cancel()
        })
        .await,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub async fn internal_duplicate_review_get(
    state: State<'_, AppState>,
    entry_id: String,
) -> Result<ApiResult<InternalDuplicateReview>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(
        run_application_blocking("internal_duplicate_review_get", move || {
            supervisor.review_get(&entry_id)
        })
        .await,
    )
}

#[tauri::command(rename_all = "camelCase")]
pub async fn internal_removal_plan(
    state: State<'_, AppState>,
    request: InternalRemovalPlanRequest,
) -> Result<ApiResult<InternalRemovalPlan>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(run_application_blocking("internal_removal_plan", move || {
        supervisor.removal_plan(request)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn internal_removal_apply(
    state: State<'_, AppState>,
    request: InternalRemovalApplyRequest,
) -> Result<ApiResult<InternalRemovalResult>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(run_application_blocking("internal_removal_apply", move || {
        supervisor.removal_apply(request)
    })
    .await)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn internal_removal_undo(
    state: State<'_, AppState>,
    request: InternalRemovalUndoRequest,
) -> Result<ApiResult<InternalRemovalResult>, ApiError> {
    let supervisor = state.internal_duplicates.clone();
    Ok(run_application_blocking("internal_removal_undo", move || {
        supervisor.removal_undo(request)
    })
    .await)
}

#[tauri::command]
pub async fn settings_get(
    state: State<'_, AppState>,
) -> Result<ApiResult<SettingsSnapshot>, ApiError> {
    Ok(state.service.settings_get().into())
}

#[tauri::command]
pub async fn folder_name_template_preview(
    state: State<'_, AppState>,
    template: String,
) -> Result<ApiResult<String>, ApiError> {
    Ok(state.service.folder_name_template_preview(&template).into())
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

#[tauri::command]
pub fn thumbnail_cache_clear(
    state: State<'_, AppState>,
) -> Result<ApiResult<ThumbnailCacheClearDto>, ApiError> {
    Ok(ApiResult::success(state.thumbnails.clear_cache()))
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

#[tauri::command(rename_all = "camelCase")]
pub async fn maintenance_preview(
    state: State<'_, AppState>,
    action: MaintenanceAction,
) -> Result<ApiResult<MaintenancePreview>, ApiError> {
    if let Err(error) = action.validate() {
        return Ok(ApiResult::failure(ApplicationError::from(error).into()));
    }
    let preview_id = state.remember_maintenance_preview(action.clone());
    let (original_files_deleted, user_decisions_preserved, restart_required, steps, warnings) =
        match &action {
            MaintenanceAction::QuickRepair => (
                false,
                true,
                false,
                vec![
                    "완료된 썸네일·검색 cache를 비웁니다".into(),
                    "중단된 다운로드와 검사 작업을 안전하게 복구합니다".into(),
                    "보류된 격리·복원 작업을 다시 확인합니다".into(),
                ],
                vec!["유효한 HTTP host cooldown과 Retry-After는 유지됩니다".into()],
            ),
            MaintenanceAction::RebuildLibrary { .. } => (
                false,
                true,
                false,
                vec![
                    "SQLite artifact, manifest와 저장 파일을 검사합니다".into(),
                    "선택한 파생 분석만 다시 실행합니다".into(),
                ],
                vec![
                    "모호한 final/.part 충돌은 덮어쓰거나 삭제하지 않고 recovery로 보냅니다".into(),
                ],
            ),
            MaintenanceAction::FactoryReset { .. } => (
                false,
                false,
                true,
                vec![
                    "모든 worker를 취소하고 종료합니다".into(),
                    "다음 시작 전에 앱 SQLite 상태를 recovery backup으로 옮깁니다".into(),
                    "새 SQLite DB와 기본 설정으로 다시 시작합니다".into(),
                ],
                vec!["외부 download root와 quarantine/recovery 원본 파일은 유지됩니다".into()],
            ),
        };
    Ok(ApiResult::success(MaintenancePreview {
        preview_id,
        action,
        original_files_deleted,
        user_decisions_preserved,
        restart_required,
        warnings,
        steps,
    }))
}

#[tauri::command(rename_all = "camelCase")]
pub async fn maintenance_execute(
    app: AppHandle,
    state: State<'_, AppState>,
    preview_id: String,
    action: MaintenanceAction,
) -> Result<ApiResult<MaintenanceResult>, ApiError> {
    if let Err(error) = action.validate() {
        return Ok(ApiResult::failure(ApplicationError::from(error).into()));
    }
    if !state.consume_maintenance_preview(preview_id.trim(), &action) {
        return Ok(ApiResult::failure(
            ApplicationError::from(ValidationError::new(
                "previewId",
                "a matching maintenance preview is required before execution",
            ))
            .into(),
        ));
    }
    if matches!(action, MaintenanceAction::QuickRepair) {
        state.search_pages.cancel_all();
    }

    let thumbnails = state.thumbnails.clone();
    let live_source = Arc::clone(&state.live_source);
    let downloads = state.downloads.clone();
    let auto_find = state.auto_find.clone();
    let duplicates = state.duplicates.clone();
    let internal_duplicates = state.internal_duplicates.clone();
    let data_dir = state.data_dir.clone();
    let execute_action = action.clone();
    let result = run_application_blocking("maintenance_execute", move || {
        let mut completed_steps = Vec::new();
        let mut warnings = Vec::new();
        match &execute_action {
            MaintenanceAction::QuickRepair => {
                thumbnails.clear_cache();
                live_source.clear_derived_caches();
                downloads.recover_startup_state()?;
                auto_find.recover_interrupted()?;
                duplicates.recover_interrupted()?;
                internal_duplicates.recover_interrupted()?;
                match internal_duplicates.reconcile_pending_page_moves() {
                    Ok(_) => {}
                    Err(error) => warnings.push(format!("internal page recovery deferred: {error}")),
                }
                completed_steps.extend([
                    "thumbnail and source caches cleared".into(),
                    "interrupted work recovery completed".into(),
                ]);
                Ok(MaintenanceResult { action: execute_action, completed_steps, warnings, restart_required: false })
            }
            MaintenanceAction::RebuildLibrary {
                rebuild_thumbnail_data,
                rebuild_duplicate_analysis,
                rebuild_internal_analysis,
                rebuild_auto_find_results,
            } => {
                let report = downloads.reconcile()?;
                completed_steps.push(format!("{} artifacts inspected", report.inspected_artifacts));
                if *rebuild_thumbnail_data {
                    thumbnails.clear_cache();
                    live_source.clear_derived_caches();
                    completed_steps.push("thumbnail derived caches cleared".into());
                }
                if *rebuild_duplicate_analysis {
                    duplicates.start()?;
                    completed_steps.push("gallery duplicate analysis started".into());
                }
                if *rebuild_internal_analysis {
                    internal_duplicates.start()?;
                    completed_steps.push("internal duplicate analysis started".into());
                }
                if *rebuild_auto_find_results {
                    auto_find.refresh()?;
                    completed_steps.push("Auto Find refresh started".into());
                }
                Ok(MaintenanceResult { action: execute_action, completed_steps, warnings, restart_required: false })
            }
            MaintenanceAction::FactoryReset { .. } => {
                internal_duplicates.shutdown_and_wait();
                duplicates.shutdown_and_wait();
                auto_find.shutdown_and_wait();
                downloads.shutdown_and_wait();
                std::fs::write(data_dir.join("factory-reset.pending"), b"v1\n")
                    .map_err(|error| ApplicationError::from(crate::application::RepositoryError::Other(format!("could not schedule factory reset: {error}"))))?;
                Ok(MaintenanceResult {
                    action: execute_action,
                    completed_steps: vec!["factory reset scheduled for the next startup".into()],
                    warnings: vec!["the app will now exit; external originals are unchanged".into()],
                    restart_required: true,
                })
            }
        }
    }).await;
    if matches!(action, MaintenanceAction::FactoryReset { .. })
        && matches!(result, ApiResult::Success(_))
    {
        app.exit(0);
    }
    Ok(result)
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
    let duplicates = state.duplicates.clone();
    let internal_duplicates = state.internal_duplicates.clone();
    if let Err(error) = tauri::async_runtime::spawn_blocking(move || {
        internal_duplicates.shutdown_and_wait();
        duplicates.shutdown_and_wait();
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
    request_id: String,
) -> Result<ApiResult<GalleryPage>, ApiError> {
    let request_id = request_id.trim().to_owned();
    if request_id.is_empty() || request_id.len() > 200 {
        return Ok(ApiResult::failure(
            ApplicationError::from(ValidationError::new(
                "requestId",
                "must contain between 1 and 200 bytes",
            ))
            .into(),
        ));
    }
    let cancellation = state.search_pages.start(&request_id);
    let service = state.service.clone();
    let result = run_application_blocking("search_page_get", move || {
        service.search_page_get_cancellable(query_id, page, &cancellation)
    })
    .await;
    state.search_pages.finish(&request_id);
    Ok(result)
}

#[tauri::command(rename_all = "camelCase")]
pub async fn search_page_cancel(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<ApiResult<bool>, ApiError> {
    let request_id = request_id.trim();
    if request_id.is_empty() || request_id.len() > 200 {
        return Ok(ApiResult::failure(
            ApplicationError::from(ValidationError::new(
                "requestId",
                "must contain between 1 and 200 bytes",
            ))
            .into(),
        ));
    }
    Ok(ApiResult::success(state.search_pages.cancel(request_id)))
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

    #[test]
    fn search_page_cancellation_is_replayed_when_cancel_arrives_before_start() {
        let requests = SearchPageRequests::default();
        assert!(requests.cancel("request-before-start"));
        let token = requests.start("request-before-start");
        assert!(token.is_cancelled());
        requests.finish("request-before-start");
    }

    #[test]
    fn search_page_cancellation_reaches_an_active_request() {
        let requests = SearchPageRequests::default();
        let token = requests.start("request-active");
        assert!(!token.is_cancelled());
        assert!(requests.cancel("request-active"));
        assert!(token.is_cancelled());
        requests.finish("request-active");
    }
}
