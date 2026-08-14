pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod interface;
pub mod thumbnail;

#[cfg(test)]
mod tests;

use std::{
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use application::ApplicationService;
use infrastructure::{FixtureSearchRepository, SqliteRepository};
use interface::AppState;
use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconEvent},
    Emitter, Manager,
};
use thumbnail::{
    FixtureThumbnailResolver, ThumbnailCompletionEventDto, ThumbnailCoordinator,
    ThumbnailCoordinatorConfig,
};

pub fn run() {
    infrastructure::telemetry::init();

    let result = tauri::Builder::default()
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
            let database_path = data_dir.join("atsumi-next.sqlite3");
            let repository = SqliteRepository::open(&database_path)?;
            let search_repository = FixtureSearchRepository::new()?;
            let repository = Arc::new(repository);
            let service = ApplicationService::new(repository.clone())
                .with_download_repository(repository)
                .with_search_repository(Arc::new(search_repository));
            let settings = service.settings_get()?;
            let thumbnail_config = ThumbnailCoordinatorConfig {
                max_concurrency: settings.concurrent_image_requests as usize,
                request_start_interval: Duration::from_millis(settings.request_start_interval_ms),
                ..ThumbnailCoordinatorConfig::default()
            };
            let thumbnails = ThumbnailCoordinator::with_resolver(
                FixtureThumbnailResolver::new(),
                thumbnail_config,
            )?;
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
            app.manage(AppState::new(
                service,
                thumbnails,
                thumbnail_completion_tx,
            ));
            if let Some(window) = app.get_webview_window("main") {
                window.show()?;
                window.unminimize()?;
                window.set_focus()?;
            }
            tracing::info!(
                database_file = "atsumi-next.sqlite3",
                app_version = env!("CARGO_PKG_VERSION"),
                "Atsumi Next backend initialized"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            interface::commands::settings_get,
            interface::commands::settings_update,
            interface::commands::window_placement_get,
            interface::commands::window_placement_update,
            interface::commands::search_submit,
            interface::commands::search_page_get,
            interface::commands::gallery_detail_get,
            interface::commands::download_queue_add,
            interface::commands::download_entries_list,
            interface::commands::download_retry,
            interface::commands::download_cancel,
            interface::commands::download_active_count,
            interface::commands::thumbnail_request,
            interface::commands::thumbnail_cancel,
            interface::commands::thumbnail_invalidate,
            interface::commands::thumbnail_reprioritize,
            interface::commands::thumbnail_stats,
            interface::commands::app_minimize_to_tray,
            interface::commands::app_quit,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        tracing::error!(error = %error, "Atsumi Next exited with an error");
    }
}
