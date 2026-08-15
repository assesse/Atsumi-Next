use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Cursor, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use image::{
    codecs::webp::WebPEncoder, ExtendedColorType, GenericImageView, ImageEncoder, ImageFormat,
    ImageReader, Limits,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{
        ArtifactLayout, ArtifactStore, DownloadPagePayload, DownloadPipelineError,
        DownloadPipelineErrorCode, DownloadSourceImageFormat, ExistingPageVerification, StoredPage,
    },
    domain::{
        ArtifactBundle, ArtifactManifest, ArtifactRelativePath, ArtifactSha256,
        ArtifactStorageFormat, DownloadArtifactState, Gallery, PageArtifactState, SourcePageNumber,
    },
    thumbnail::CancellationToken,
};

const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
const MANIFEST_FILE_NAME: &str = "manifest.json";

#[derive(Debug, Default)]
pub struct FilesystemArtifactStore;

impl FilesystemArtifactStore {
    pub const fn new() -> Self {
        Self
    }
}

impl ArtifactStore for FilesystemArtifactStore {
    fn validate_download_root(&self, root: &Path) -> Result<PathBuf, DownloadPipelineError> {
        if root.as_os_str().is_empty() || !root.is_absolute() {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::RootUnavailable,
                "The download folder must be an absolute path",
                false,
            ));
        }
        fs::create_dir_all(root)
            .map_err(|_| filesystem_error("The download folder could not be created"))?;
        let root = root
            .canonicalize()
            .map_err(|_| filesystem_error("The download folder could not be resolved"))?;
        if !root.is_dir() {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::RootUnavailable,
                "The selected download path is not a folder",
                false,
            ));
        }

        let probe = root.join(format!(".atsumi-write-probe-{}", Uuid::new_v4()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)
            .map_err(|_| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::RootUnavailable,
                    "The selected download folder is not writable",
                    false,
                )
            })?;
        let probe_result = file.write_all(b"atsumi").and_then(|()| file.sync_all());
        drop(file);
        let cleanup_result = fs::remove_file(&probe);
        if probe_result.is_err() || cleanup_result.is_err() {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::RootUnavailable,
                "The selected download folder could not complete a safe write test",
                false,
            ));
        }
        Ok(root)
    }

    fn prepare_layout(
        &self,
        root: &Path,
        gallery: &Gallery,
    ) -> Result<ArtifactLayout, DownloadPipelineError> {
        let root = self.validate_download_root(root)?;
        let relative_directory = ArtifactRelativePath::new(format!("gallery-{}", gallery.id.get()))
            .map_err(|error| invalid_path(error.to_string()))?;
        let directory = root.join(relative_directory.as_str());
        fs::create_dir_all(&directory)
            .map_err(|_| filesystem_error("The gallery folder could not be created"))?;
        let canonical_directory = directory
            .canonicalize()
            .map_err(|_| filesystem_error("The gallery folder could not be resolved"))?;
        ensure_descendant(&root, &canonical_directory)?;
        let manifest_relative_path = ArtifactRelativePath::new(format!(
            "{}/{}",
            relative_directory.as_str(),
            MANIFEST_FILE_NAME
        ))
        .map_err(|error| invalid_path(error.to_string()))?;
        Ok(ArtifactLayout {
            root,
            relative_directory,
            manifest_relative_path,
        })
    }

    fn verify_existing_page(
        &self,
        layout: &ArtifactLayout,
        source_page_number: SourcePageNumber,
        source_revision: &str,
        expected: Option<&StoredPage>,
    ) -> Result<ExistingPageVerification, DownloadPipelineError> {
        let relative_path = page_relative_path(layout, source_page_number)?;
        let path = resolve_managed_path(&layout.root, &relative_path, false)?;
        if !path.exists() {
            return Ok(ExistingPageVerification::Missing);
        }
        let stored = verify_webp_file(
            &layout.root,
            relative_path.clone(),
            source_page_number,
            source_revision,
        )?;
        if let Some(expected) = expected {
            if expected.byte_length != stored.byte_length || expected.sha256 != stored.sha256 {
                return Ok(ExistingPageVerification::Invalid {
                    relative_path,
                    reason: "stored page length or SHA-256 does not match the database checkpoint",
                });
            }
        }
        Ok(ExistingPageVerification::Verified(stored))
    }

    fn store_page(
        &self,
        layout: &ArtifactLayout,
        page: &DownloadPagePayload,
        cancellation: &CancellationToken,
    ) -> Result<StoredPage, DownloadPipelineError> {
        ensure_not_cancelled(cancellation)?;
        let final_relative_path = page_relative_path(layout, page.source_page_number)?;
        let part_relative_path = ArtifactRelativePath::new(format!(
            "{}/.{:04}.webp.part",
            layout.relative_directory.as_str(),
            page.source_page_number.get()
        ))
        .map_err(|error| invalid_path(error.to_string()))?;
        let final_path = resolve_managed_path(&layout.root, &final_relative_path, false)?;
        let part_path = resolve_managed_path(&layout.root, &part_relative_path, false)?;
        let bytes = normalized_webp_bytes(page)?;
        ensure_not_cancelled(cancellation)?;

        let file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&part_path)
            .map_err(|_| filesystem_error("The temporary page file could not be created"))?;
        let mut writer = BufWriter::with_capacity(64 * 1024, file);
        for chunk in bytes.chunks(64 * 1024) {
            ensure_not_cancelled(cancellation)?;
            writer
                .write_all(chunk)
                .map_err(|_| filesystem_error("The temporary page file could not be written"))?;
        }
        writer
            .flush()
            .map_err(|_| filesystem_error("The temporary page file could not be flushed"))?;
        writer
            .get_ref()
            .sync_all()
            .map_err(|_| filesystem_error("The temporary page file could not be synchronized"))?;
        drop(writer);
        ensure_not_cancelled(cancellation)?;

        let part_stored = verify_webp_file(
            &layout.root,
            part_relative_path.clone(),
            page.source_page_number,
            &page.source_revision,
        )?;
        if final_path.exists() {
            let final_stored = verify_webp_file(
                &layout.root,
                final_relative_path.clone(),
                page.source_page_number,
                &page.source_revision,
            )?;
            if final_stored.sha256 != part_stored.sha256 {
                return Err(DownloadPipelineError::new(
                    DownloadPipelineErrorCode::HashMismatch,
                    "An existing final page differs from the verified temporary file",
                    false,
                ));
            }
            fs::remove_file(&part_path).map_err(|_| {
                filesystem_error("The duplicate temporary page could not be removed")
            })?;
            return Ok(final_stored);
        }
        fs::rename(&part_path, &final_path)
            .map_err(|_| filesystem_error("The verified page could not be finalized atomically"))?;
        verify_webp_file(
            &layout.root,
            final_relative_path,
            page.source_page_number,
            &page.source_revision,
        )
    }

    fn write_manifest(
        &self,
        layout: &ArtifactLayout,
        manifest: &ArtifactManifest,
    ) -> Result<(), DownloadPipelineError> {
        let final_path = resolve_managed_path(&layout.root, &layout.manifest_relative_path, false)?;
        let temp_relative = ArtifactRelativePath::new(format!(
            "{}/.manifest-{}.json.part",
            layout.relative_directory.as_str(),
            Uuid::new_v4()
        ))
        .map_err(|error| invalid_path(error.to_string()))?;
        let temp_path = resolve_managed_path(&layout.root, &temp_relative, false)?;
        let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| {
            DownloadPipelineError::new(
                DownloadPipelineErrorCode::ManifestInvalid,
                "The artifact manifest could not be serialized",
                false,
            )
        })?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|_| filesystem_error("The manifest temporary file could not be created"))?;
        file.write_all(&bytes)
            .and_then(|()| file.flush())
            .and_then(|()| file.sync_all())
            .map_err(|_| {
                filesystem_error("The manifest temporary file could not be synchronized")
            })?;
        drop(file);

        let parsed = read_manifest_file(&temp_path)?;
        if parsed != *manifest {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ManifestInvalid,
                "The manifest verification round trip did not match",
                false,
            ));
        }
        atomic_replace(&temp_path, &final_path)?;
        Ok(())
    }

    fn read_manifest(
        &self,
        layout: &ArtifactLayout,
    ) -> Result<Option<ArtifactManifest>, DownloadPipelineError> {
        let path = resolve_managed_path(&layout.root, &layout.manifest_relative_path, false)?;
        if !path.exists() {
            return Ok(None);
        }
        read_manifest_file(&path).map(Some)
    }

    fn first_verified_page_path(
        &self,
        root: &Path,
        bundle: &ArtifactBundle,
    ) -> Result<PathBuf, DownloadPipelineError> {
        if bundle.artifact.state != DownloadArtifactState::Complete {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ArtifactMissing,
                "Only a verified complete artifact can be opened",
                false,
            ));
        }
        let root = self.validate_download_root(root)?;
        let page = bundle
            .pages
            .iter()
            .filter(|page| !page.excluded && page.state == PageArtifactState::Present)
            .min_by_key(|page| page.page_id.source_page_number)
            .ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ArtifactMissing,
                    "The artifact has no verified page to open",
                    false,
                )
            })?;
        let expected = StoredPage {
            source_page_number: page.page_id.source_page_number,
            relative_path: page.relative_path.clone(),
            byte_length: page.byte_length.ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "The first page is missing its byte length",
                    false,
                )
            })?,
            sha256: page.sha256.clone().ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "The first page is missing its SHA-256 digest",
                    false,
                )
            })?,
            storage_format: page.storage_format.ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "The first page is missing its storage format",
                    false,
                )
            })?,
            source_revision: page.source_revision.clone().ok_or_else(|| {
                DownloadPipelineError::new(
                    DownloadPipelineErrorCode::ManifestInvalid,
                    "The first page is missing its source revision",
                    false,
                )
            })?,
            verified_at: page.verified_at.clone().unwrap_or_default(),
        };
        let layout = ArtifactLayout {
            root,
            relative_directory: bundle.artifact.relative_directory.clone(),
            manifest_relative_path: bundle.artifact.manifest_relative_path.clone().ok_or_else(
                || {
                    DownloadPipelineError::new(
                        DownloadPipelineErrorCode::ManifestInvalid,
                        "The artifact is missing its manifest path",
                        false,
                    )
                },
            )?,
        };
        match self.verify_existing_page(
            &layout,
            page.page_id.source_page_number,
            &expected.source_revision,
            Some(&expected),
        )? {
            ExistingPageVerification::Verified(_) => {
                resolve_managed_path(&layout.root, &page.relative_path, true)
            }
            ExistingPageVerification::Missing => Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ArtifactMissing,
                "The first verified page is missing from disk",
                false,
            )),
            ExistingPageVerification::Invalid { .. } => Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::HashMismatch,
                "The first page no longer matches its verified digest",
                false,
            )),
        }
    }

    fn open_with_default_viewer(&self, path: &Path) -> Result<(), DownloadPipelineError> {
        open_default_viewer(path)
    }

    fn move_managed_directory(
        &self,
        root: &Path,
        source: &ArtifactRelativePath,
        destination: &ArtifactRelativePath,
    ) -> Result<(), DownloadPipelineError> {
        let root = self.validate_download_root(root)?;
        let source_path = resolve_managed_path(&root, source, true)?;
        if !source_path.is_dir() {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::ArtifactMissing,
                "The managed artifact folder is missing",
                false,
            ));
        }
        let destination_path = resolve_managed_path(&root, destination, false)?;
        if destination_path.exists() {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::QuarantineConflict,
                "The quarantine destination already exists",
                false,
            ));
        }
        fs::rename(&source_path, &destination_path).map_err(|_| {
            DownloadPipelineError::new(
                DownloadPipelineErrorCode::Filesystem,
                "The managed artifact folder could not be moved atomically",
                true,
            )
        })?;
        let canonical_destination = destination_path
            .canonicalize()
            .map_err(|_| filesystem_error("The moved artifact folder could not be resolved"))?;
        ensure_descendant(&root, &canonical_destination)?;
        Ok(())
    }

    fn managed_path_exists(
        &self,
        root: &Path,
        relative_path: &ArtifactRelativePath,
    ) -> Result<bool, DownloadPipelineError> {
        let root = self.validate_download_root(root)?;
        let candidate = resolve_managed_path(&root, relative_path, false)?;
        if !candidate.exists() {
            return Ok(false);
        }
        let canonical = candidate
            .canonicalize()
            .map_err(|_| filesystem_error("The managed artifact path could not be resolved"))?;
        ensure_descendant(&root, &canonical)?;
        Ok(true)
    }
}

fn normalized_webp_bytes(page: &DownloadPagePayload) -> Result<Vec<u8>, DownloadPipelineError> {
    if page.source_format == DownloadSourceImageFormat::Webp {
        decode_image(&page.bytes, ImageFormat::WebP)?;
        return Ok(page.bytes.clone());
    }
    let format = match page.source_format {
        DownloadSourceImageFormat::Webp => ImageFormat::WebP,
        DownloadSourceImageFormat::Jpeg => ImageFormat::Jpeg,
        DownloadSourceImageFormat::Png => ImageFormat::Png,
        DownloadSourceImageFormat::Avif => ImageFormat::Avif,
    };
    let image = decode_image(&page.bytes, format)?;
    let rgba = image.to_rgba8();
    let mut output = Vec::new();
    WebPEncoder::new_lossless(&mut output)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|_| {
            DownloadPipelineError::new(
                DownloadPipelineErrorCode::ImageEncodeFailed,
                "The decoded page could not be encoded as WebP",
                false,
            )
        })?;
    Ok(output)
}

fn decode_image(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<image::DynamicImage, DownloadPipelineError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOC);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::ImageDecodeFailed,
            "The page image could not be decoded safely",
            false,
        )
    })?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ImageDecodeFailed,
            "The decoded page has invalid dimensions",
            false,
        ));
    }
    Ok(image)
}

fn verify_webp_file(
    root: &Path,
    relative_path: ArtifactRelativePath,
    source_page_number: SourcePageNumber,
    source_revision: &str,
) -> Result<StoredPage, DownloadPipelineError> {
    let path = resolve_managed_path(root, &relative_path, true)?;
    let mut file = File::open(&path)
        .map_err(|_| filesystem_error("The page file could not be opened for verification"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| filesystem_error("The page file could not be read for verification"))?;
    if bytes.is_empty() {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ImageDecodeFailed,
            "The page file is empty",
            false,
        ));
    }
    let format = image::guess_format(&bytes).map_err(|_| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::ImageDecodeFailed,
            "The stored page does not have a recognized image signature",
            false,
        )
    })?;
    if format != ImageFormat::WebP {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::ImageDecodeFailed,
            "The stored page is not a WebP image",
            false,
        ));
    }
    decode_image(&bytes, ImageFormat::WebP)?;
    let digest = ArtifactSha256::new(format!("{:x}", Sha256::digest(&bytes))).map_err(|error| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::HashMismatch,
            error.to_string(),
            false,
        )
    })?;
    Ok(StoredPage {
        source_page_number,
        relative_path,
        byte_length: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        sha256: digest,
        storage_format: ArtifactStorageFormat::Webp,
        source_revision: source_revision.to_owned(),
        verified_at: now_unix_ms(),
    })
}

fn page_relative_path(
    layout: &ArtifactLayout,
    source_page_number: SourcePageNumber,
) -> Result<ArtifactRelativePath, DownloadPipelineError> {
    ArtifactRelativePath::new(format!(
        "{}/{:04}.webp",
        layout.relative_directory.as_str(),
        source_page_number.get()
    ))
    .map_err(|error| invalid_path(error.to_string()))
}

fn resolve_managed_path(
    root: &Path,
    relative: &ArtifactRelativePath,
    must_exist: bool,
) -> Result<PathBuf, DownloadPipelineError> {
    let root = root
        .canonicalize()
        .map_err(|_| filesystem_error("The download root could not be resolved"))?;
    let candidate = root.join(relative.as_str());
    if must_exist {
        let candidate = candidate.canonicalize().map_err(|_| {
            DownloadPipelineError::new(
                DownloadPipelineErrorCode::ArtifactMissing,
                "A managed artifact file is missing",
                false,
            )
        })?;
        ensure_descendant(&root, &candidate)?;
        return Ok(candidate);
    }
    let parent = candidate.parent().ok_or_else(|| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::PathOutsideRoot,
            "The artifact path has no managed parent",
            false,
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|_| filesystem_error("The managed artifact directory could not be created"))?;
    let parent = parent
        .canonicalize()
        .map_err(|_| filesystem_error("The managed artifact directory could not be resolved"))?;
    if parent != root {
        ensure_descendant(&root, &parent)?;
    }
    Ok(candidate)
}

fn ensure_descendant(root: &Path, candidate: &Path) -> Result<(), DownloadPipelineError> {
    if candidate == root || !candidate.starts_with(root) {
        return Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::PathOutsideRoot,
            "The managed artifact path escapes the configured download folder",
            false,
        ));
    }
    Ok(())
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<(), DownloadPipelineError> {
    if cancellation.is_cancelled() {
        Err(DownloadPipelineError::cancelled())
    } else {
        Ok(())
    }
}

fn read_manifest_file(path: &Path) -> Result<ArtifactManifest, DownloadPipelineError> {
    let file = File::open(path)
        .map_err(|_| filesystem_error("The artifact manifest could not be opened"))?;
    serde_json::from_reader(file).map_err(|_| {
        DownloadPipelineError::new(
            DownloadPipelineErrorCode::ManifestInvalid,
            "The artifact manifest is malformed or uses an unsupported schema",
            false,
        )
    })
}

fn atomic_replace(source: &Path, destination: &Path) -> Result<(), DownloadPipelineError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::PCWSTR,
            Win32::Storage::FileSystem::{
                MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
            },
        };

        let source = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let destination = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|_| filesystem_error("The artifact manifest could not be finalized atomically"))?;
        Ok(())
    }

    #[cfg(not(windows))]
    {
        fs::rename(source, destination).map_err(|_| {
            filesystem_error("The artifact manifest could not be finalized atomically")
        })
    }
}

fn open_default_viewer(path: &Path) -> Result<(), DownloadPipelineError> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            core::{w, PCWSTR},
            Win32::{
                Foundation::HWND,
                UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
            },
        };

        let path = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                Some(HWND::default()),
                w!("open"),
                PCWSTR(path.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.0 as isize <= 32 {
            return Err(DownloadPipelineError::new(
                DownloadPipelineErrorCode::Filesystem,
                "Windows could not open the page with its default viewer",
                false,
            ));
        }
        Ok(())
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        Err(DownloadPipelineError::new(
            DownloadPipelineErrorCode::Filesystem,
            "Opening artifacts is supported only on Windows",
            false,
        ))
    }
}

fn now_unix_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_owned())
}

fn filesystem_error(message: &'static str) -> DownloadPipelineError {
    DownloadPipelineError::new(DownloadPipelineErrorCode::Filesystem, message, true)
}

fn invalid_path(_detail: String) -> DownloadPipelineError {
    DownloadPipelineError::new(
        DownloadPipelineErrorCode::PathOutsideRoot,
        "The artifact path is outside the configured download folder",
        false,
    )
}
