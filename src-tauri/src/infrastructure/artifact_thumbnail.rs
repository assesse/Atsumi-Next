use std::{io::Cursor, path::PathBuf, sync::Arc};

use image::{
    codecs::webp::WebPEncoder, ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat,
    ImageReader, Limits,
};

use crate::{
    application::{ArtifactRepository, ArtifactStore, StateRepository},
    domain::{DownloadArtifactState, DownloadEntryId},
    thumbnail::{
        CancellationToken, ResolvedThumbnail, ThumbnailFailureCode, ThumbnailKey,
        ThumbnailPriority, ThumbnailResolveError, ThumbnailResolver,
    },
};

const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

/// Routes remote gallery artwork to the source resolver and verified local
/// artifact pages to the managed artifact store.  Artifact requests never
/// fall back to the network, so Review displays exactly the bytes that were
/// hashed by duplicate analysis.
pub struct CompositeThumbnailResolver {
    remote: Arc<dyn ThumbnailResolver>,
    repository: Arc<dyn ArtifactRepository>,
    settings: Arc<dyn StateRepository>,
    store: Arc<dyn ArtifactStore>,
}

impl CompositeThumbnailResolver {
    pub fn new(
        remote: Arc<dyn ThumbnailResolver>,
        repository: Arc<dyn ArtifactRepository>,
        settings: Arc<dyn StateRepository>,
        store: Arc<dyn ArtifactStore>,
    ) -> Self {
        Self {
            remote,
            repository,
            settings,
            store,
        }
    }

    fn resolve_artifact(
        &self,
        entry_id: &str,
        source_page: u32,
        cancellation: &CancellationToken,
    ) -> Result<ResolvedThumbnail, ThumbnailResolveError> {
        if cancellation.is_cancelled() {
            return Err(ThumbnailResolveError::cancelled());
        }
        let entry_id = DownloadEntryId::new(entry_id).map_err(|_| {
            ThumbnailResolveError::new(
                ThumbnailFailureCode::InvalidData,
                "artifact thumbnail entry ID is invalid",
                false,
            )
        })?;
        let bundle = self
            .repository
            .artifact_bundle_get(&entry_id)
            .map_err(repository_error)?
            .ok_or_else(|| not_found("verified artifact was not found"))?;
        if bundle.artifact.state != DownloadArtifactState::Complete {
            return Err(not_found("artifact is not a verified complete bundle"));
        }
        let page = bundle
            .pages
            .iter()
            .find(|page| page.page_id.source_page_number.get() == source_page)
            .ok_or_else(|| not_found("verified artifact page was not found"))?;
        let settings = self.settings.settings_get().map_err(repository_error)?;
        if settings.download_root.trim().is_empty() {
            return Err(not_found("download root is not configured"));
        }
        if cancellation.is_cancelled() {
            return Err(ThumbnailResolveError::cancelled());
        }
        let bytes = self
            .store
            .read_verified_page_bytes(&PathBuf::from(settings.download_root), page)
            .map_err(|error| {
                ThumbnailResolveError::new(
                    match error.code {
                        crate::application::DownloadPipelineErrorCode::ArtifactMissing => {
                            ThumbnailFailureCode::NotFound
                        }
                        crate::application::DownloadPipelineErrorCode::HashMismatch
                        | crate::application::DownloadPipelineErrorCode::ManifestInvalid => {
                            ThumbnailFailureCode::InvalidData
                        }
                        _ => ThumbnailFailureCode::DecodeFailed,
                    },
                    error.message,
                    error.retryable,
                )
            })?;
        if cancellation.is_cancelled() {
            return Err(ThumbnailResolveError::cancelled());
        }
        let mut reader = ImageReader::with_format(Cursor::new(&bytes), ImageFormat::WebP);
        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
        limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
        limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOC);
        reader.limits(limits);
        let image = reader.decode().map_err(|_| {
            ThumbnailResolveError::new(
                ThumbnailFailureCode::DecodeFailed,
                "verified artifact page could not be decoded",
                false,
            )
        })?;
        let image = image.thumbnail(1_024, 1_024);
        let (width, height) = image.dimensions();
        let rgba = image.to_rgba8();
        let mut preview_bytes = Vec::new();
        WebPEncoder::new_lossless(&mut preview_bytes)
            .write_image(&rgba, width, height, ExtendedColorType::Rgba8)
            .map_err(|_| {
                ThumbnailResolveError::new(
                    ThumbnailFailureCode::DecodeFailed,
                    "verified artifact preview could not be encoded",
                    false,
                )
            })?;
        if cancellation.is_cancelled() {
            return Err(ThumbnailResolveError::cancelled());
        }
        Ok(ResolvedThumbnail {
            content_type: "image/webp".into(),
            bytes: preview_bytes,
            width,
            height,
            source_revision: page.sha256.as_ref().map(ToString::to_string),
        })
    }
}

impl ThumbnailResolver for CompositeThumbnailResolver {
    fn resolve(
        &self,
        key: &ThumbnailKey,
        cancellation: &CancellationToken,
    ) -> Result<ResolvedThumbnail, ThumbnailResolveError> {
        self.resolve_with_priority(key, cancellation, ThumbnailPriority::Visible)
    }

    fn resolve_with_priority(
        &self,
        key: &ThumbnailKey,
        cancellation: &CancellationToken,
        priority: ThumbnailPriority,
    ) -> Result<ResolvedThumbnail, ThumbnailResolveError> {
        match key {
            ThumbnailKey::ArtifactPage {
                entry_id,
                source_page,
            } => self.resolve_artifact(entry_id, *source_page, cancellation),
            ThumbnailKey::GalleryCover { .. } | ThumbnailKey::GalleryPage { .. } => self
                .remote
                .resolve_with_priority(key, cancellation, priority),
        }
    }
}

fn repository_error(_error: crate::application::RepositoryError) -> ThumbnailResolveError {
    ThumbnailResolveError::new(
        ThumbnailFailureCode::TemporarilyUnavailable,
        "verified artifact metadata is temporarily unavailable",
        true,
    )
}

fn not_found(message: &'static str) -> ThumbnailResolveError {
    ThumbnailResolveError::new(ThumbnailFailureCode::NotFound, message, false)
}
