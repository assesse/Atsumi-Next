use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{mpsc::Sender, Arc, Mutex},
    thread,
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    domain::GalleryId,
    infrastructure::{normalized_webp_bytes, HitomiLiveAdapter},
    thumbnail::CancellationToken,
};

use super::DownloadSourcePort;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailOriginalRequest {
    pub gallery_id: i64,
    pub source_page: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailOriginalToken {
    pub request_id: String,
    pub gallery_id: i64,
    pub source_page: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailOriginalReady {
    pub request_id: String,
    pub gallery_id: i64,
    pub source_page: u32,
    /// Opaque custom-protocol URL. A filesystem path never crosses IPC.
    pub media_url: String,
    pub content_type: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone)]
struct StoredOriginal {
    path: PathBuf,
    content_type: String,
}

struct ActiveOriginal {
    token: CancellationToken,
    stored: Option<StoredOriginal>,
}

#[derive(Default)]
struct DetailOriginalState {
    active: HashMap<String, ActiveOriginal>,
}

/// One active Detail original at a time. Original bytes bypass the thumbnail
/// coordinator cache and are exposed only through a request-ID protocol URL.
pub struct DetailOriginalSupervisor {
    source: Arc<HitomiLiveAdapter>,
    root: PathBuf,
    events: Sender<DetailOriginalReady>,
    state: Arc<Mutex<DetailOriginalState>>,
}

impl DetailOriginalSupervisor {
    pub fn new(
        source: Arc<HitomiLiveAdapter>,
        data_dir: &Path,
        events: Sender<DetailOriginalReady>,
    ) -> std::io::Result<Self> {
        let root = data_dir.join("detail-original");
        fs::create_dir_all(&root)?;
        // This directory is app-owned and contains only previous transient
        // originals. It deliberately does not overlap download/quarantine roots.
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(Self {
            source,
            root,
            events,
            state: Arc::new(Mutex::new(DetailOriginalState::default())),
        })
    }

    pub fn request(&self, request: DetailOriginalRequest) -> Result<DetailOriginalToken, String> {
        if request.gallery_id <= 0 || request.source_page != 1 {
            return Err(
                "detail original supports only a positive gallery ID and source page 1".into(),
            );
        }
        let token = DetailOriginalToken {
            request_id: Uuid::new_v4().to_string(),
            gallery_id: request.gallery_id,
            source_page: request.source_page,
        };
        let cancellation = CancellationToken::new();
        {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            for active in state.active.values() {
                active.token.cancel();
                if let Some(stored) = &active.stored {
                    let _ = fs::remove_file(&stored.path);
                }
            }
            state.active.clear();
            state.active.insert(
                token.request_id.clone(),
                ActiveOriginal {
                    token: cancellation.clone(),
                    stored: None,
                },
            );
        }
        let source = Arc::clone(&self.source);
        let root = self.root.clone();
        let events = self.events.clone();
        let state = Arc::clone(&self.state);
        let worker_token = token.clone();
        thread::Builder::new()
            .name("atsumi-detail-original".into())
            .spawn(move || {
                resolve_original(source, root, events, state, worker_token, cancellation)
            })
            .map_err(|_| "could not start detail original worker")?;
        Ok(token)
    }

    pub fn cancel(&self, request_id: &str) -> bool {
        self.remove(request_id, true)
    }
    pub fn release(&self, request_id: &str) -> bool {
        self.remove(request_id, true)
    }

    fn remove(&self, request_id: &str, cancel: bool) -> bool {
        let removed = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active
            .remove(request_id);
        let Some(active) = removed else {
            return false;
        };
        if cancel {
            active.token.cancel();
        }
        if let Some(stored) = active.stored {
            let _ = fs::remove_file(stored.path);
        }
        true
    }

    pub fn read_media(&self, request_id: &str) -> Option<(Vec<u8>, String)> {
        let stored = self
            .state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .active
            .get(request_id)?
            .stored
            .clone()?;
        let root = fs::canonicalize(&self.root).ok()?;
        let path = fs::canonicalize(&stored.path).ok()?;
        if !path.starts_with(root) {
            return None;
        }
        fs::read(path)
            .ok()
            .map(|bytes| (bytes, stored.content_type))
    }
}

fn resolve_original(
    source: Arc<HitomiLiveAdapter>,
    root: PathBuf,
    events: Sender<DetailOriginalReady>,
    state: Arc<Mutex<DetailOriginalState>>,
    token: DetailOriginalToken,
    cancellation: CancellationToken,
) {
    let gallery_id = match GalleryId::new(token.gallery_id) {
        Ok(value) => value,
        Err(_) => return,
    };
    let source_page = match crate::domain::SourcePageNumber::new(token.source_page) {
        Ok(value) => value,
        Err(_) => return,
    };
    let payload = match source.download_page(gallery_id, source_page, &cancellation) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::debug!(gallery_id = token.gallery_id, code = ?error.code, "detail original failed");
            return;
        }
    };
    if cancellation.is_cancelled() {
        return;
    }
    // WebView2 support for AVIF varies by runtime. Downloads keep their normal
    // storage pipeline, but the one transient Detail hero is normalized to
    // WebP so a valid AVIF result cannot silently fail at the display boundary.
    let (bytes, content_type, extension) =
        if payload.source_format == super::DownloadSourceImageFormat::Avif {
            match normalized_webp_bytes(&payload) {
                Ok(bytes) => (bytes, "image/webp".to_owned(), "webp"),
                Err(error) => {
                    tracing::debug!(
                        gallery_id = token.gallery_id,
                        code = ?error.code,
                        "detail original display conversion failed"
                    );
                    return;
                }
            }
        } else {
            let content_type = match payload.source_format {
                super::DownloadSourceImageFormat::Webp => "image/webp",
                super::DownloadSourceImageFormat::Jpeg => "image/jpeg",
                super::DownloadSourceImageFormat::Png => "image/png",
                super::DownloadSourceImageFormat::Avif => unreachable!("handled above"),
            }
            .to_owned();
            let extension = match payload.source_format {
                super::DownloadSourceImageFormat::Webp => "webp",
                super::DownloadSourceImageFormat::Jpeg => "jpg",
                super::DownloadSourceImageFormat::Png => "png",
                super::DownloadSourceImageFormat::Avif => unreachable!("handled above"),
            };
            (payload.bytes.clone(), content_type, extension)
        };
    let final_path = root.join(format!("{}.{}", token.request_id, extension));
    let temporary_path = root.join(format!("{}.part", token.request_id));
    if fs::write(&temporary_path, bytes).is_err() || cancellation.is_cancelled() {
        let _ = fs::remove_file(temporary_path);
        return;
    }
    if fs::rename(&temporary_path, &final_path).is_err() {
        let _ = fs::remove_file(temporary_path);
        return;
    }
    let accepted = {
        let mut guard = state.lock().unwrap_or_else(|error| error.into_inner());
        match guard.active.get_mut(&token.request_id) {
            Some(active) if !active.token.is_cancelled() && !cancellation.is_cancelled() => {
                active.stored = Some(StoredOriginal {
                    path: final_path.clone(),
                    content_type: content_type.clone(),
                });
                true
            }
            _ => false,
        }
    };
    if !accepted {
        let _ = fs::remove_file(final_path);
        return;
    }
    let _ = events.send(DetailOriginalReady {
        request_id: token.request_id.clone(),
        gallery_id: token.gallery_id,
        source_page: token.source_page,
        media_url: format!("detail-original://localhost/{}", token.request_id),
        content_type,
        width: payload.width,
        height: payload.height,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn startup_removes_only_stale_app_owned_originals_and_never_exposes_a_path() {
        let temp = tempfile::tempdir().expect("temp data dir");
        let stale_root = temp.path().join("detail-original");
        fs::create_dir_all(&stale_root).expect("create original root");
        fs::write(stale_root.join("stale.webp"), b"old").expect("write stale media");
        let (events, _receiver) = std::sync::mpsc::channel();
        let source = Arc::new(
            HitomiLiveAdapter::new(crate::infrastructure::HitomiLiveConfig::default())
                .expect("source"),
        );
        let supervisor =
            DetailOriginalSupervisor::new(source, temp.path(), events).expect("supervisor");
        assert!(!stale_root.join("stale.webp").exists());
        assert!(supervisor.read_media("../outside").is_none());
    }

    #[test]
    #[ignore = "opt-in live Floating Detail original regression smoke"]
    fn live_reported_gallery_produces_a_webview_displayable_original() {
        assert_eq!(
            std::env::var("ATSUMI_ALLOW_LIVE_SMOKE").as_deref(),
            Ok("1"),
            "live network access requires ATSUMI_ALLOW_LIVE_SMOKE=1"
        );
        let temp = tempfile::tempdir().expect("temp data dir");
        let (events, receiver) = std::sync::mpsc::channel();
        let source = Arc::new(
            HitomiLiveAdapter::new(crate::infrastructure::HitomiLiveConfig::default())
                .expect("source"),
        );
        let supervisor =
            DetailOriginalSupervisor::new(source, temp.path(), events).expect("supervisor");
        let token = supervisor
            .request(DetailOriginalRequest {
                gallery_id: 4_133_977,
                source_page: 1,
            })
            .expect("request original");
        let ready = receiver
            .recv_timeout(std::time::Duration::from_secs(60))
            .expect("original readiness event");
        assert_eq!(ready.request_id, token.request_id);
        assert_ne!(ready.content_type, "image/avif");
        let (bytes, content_type) = supervisor
            .read_media(&ready.request_id)
            .expect("scoped media bytes");
        assert_eq!(content_type, ready.content_type);
        assert!(match content_type.as_str() {
            "image/webp" => bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"),
            "image/jpeg" => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            _ => false,
        });
        assert!(supervisor.release(&ready.request_id));
        assert!(supervisor.read_media(&ready.request_id).is_none());
    }
}
