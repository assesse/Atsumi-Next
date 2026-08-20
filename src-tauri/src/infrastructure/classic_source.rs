use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use image::{GenericImageView, ImageFormat, ImageReader, Limits};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{
    application::{
        ApplicationError, ClassicSourceInspector, ClassicSourceInventory, DownloadPagePayload,
        DownloadSourceImageFormat,
    },
    domain::{
        ClassicConflictCode, ClassicConflictSeverity, ClassicImportConflict,
        ClassicImportGalleryPlan, ClassicImportPagePlan, ClassicImportPlan,
        ClassicLegacyHashSummary, ClassicPairPlan, ClassicSeriesPlan, ClassicSourceRootKind,
        FavoriteKey, FavoriteNamespace, Language, SearchRequest, SearchSort, SourcePageNumber,
        CLASSIC_IMPORT_SCHEMA_VERSION,
    },
    thumbnail::CancellationToken,
};

const MAX_STATE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_HASH_DATABASE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_SCAN_ENTRIES: usize = 100_000;
const MAX_GALLERIES: usize = 20_000;
const MAX_PAGES: usize = 500_000;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_IMAGE_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

#[derive(Debug, Default)]
pub struct FilesystemClassicSource;

impl FilesystemClassicSource {
    pub const fn new() -> Self {
        Self
    }
}

impl ClassicSourceInspector for FilesystemClassicSource {
    fn inspect(
        &self,
        selected_data_root: &Path,
        selected_download_root: Option<&Path>,
    ) -> Result<ClassicSourceInventory, ApplicationError> {
        let selected_data_root = canonical_directory(selected_data_root)?;
        let data_root = locate_data_root(&selected_data_root);
        let download_root = selected_download_root
            .map(canonical_directory)
            .transpose()?;
        let data_root_label = path_label(&data_root);
        let download_root_label = download_root.as_deref().map(path_label);
        let mut conflicts = Vec::new();
        let mut fingerprint = Sha256::new();
        fingerprint.update(b"atsumi-classic-import-v1\0");

        let state = read_classic_state(&data_root, &mut fingerprint, &mut conflicts)?;
        let favorites = parse_favorites(&state);
        let search_history = parse_search_history(&state);
        let auto_find_exclusions = numeric_array(state.get("atsumiExcludedGalleries"));
        let hidden_galleries = numeric_array(state.get("atsumiSimilarExcludedGalleries"));
        let pair_exclusions = parse_pairs(state.get("atsumiDismissedSimilarPairs"));
        let series = parse_series(state.get("atsumiSeriesGroups"));
        let state_downloads = parse_state_downloads(state.get("atsumiDownloads"));
        let legacy_hashes = inspect_hash_database(&data_root, &mut fingerprint, &mut conflicts)?;

        let mut galleries = if let Some(download_root) = download_root.as_deref() {
            scan_download_galleries(
                &data_root,
                download_root,
                &state_downloads,
                &hidden_galleries,
                &mut fingerprint,
                &mut conflicts,
            )?
        } else {
            for download in state_downloads.values().filter(|item| item.completed) {
                push_conflict(
                    &mut conflicts,
                    ClassicConflictCode::StateCompletedFolderMissing,
                    ClassicConflictSeverity::Blocking,
                    Some(download.gallery_id),
                    format!("state-folder-missing:{}", download.gallery_id),
                    "Classic marks this gallery complete, but no download folder was selected",
                    false,
                );
            }
            Vec::new()
        };

        let discovered = galleries
            .iter()
            .map(|gallery| gallery.gallery_id)
            .collect::<BTreeSet<_>>();
        for download in state_downloads.values().filter(|item| item.completed) {
            if !discovered.contains(&download.gallery_id) {
                push_conflict(
                    &mut conflicts,
                    ClassicConflictCode::StateCompletedFolderMissing,
                    ClassicConflictSeverity::Blocking,
                    Some(download.gallery_id),
                    format!("state-folder-missing:{}", download.gallery_id),
                    "Classic marks this gallery complete, but its folder was not found",
                    false,
                );
            }
        }
        for hash in &legacy_hashes {
            if !discovered.contains(&hash.gallery_id) {
                push_conflict(
                    &mut conflicts,
                    ClassicConflictCode::HashOnly,
                    ClassicConflictSeverity::Info,
                    Some(hash.gallery_id),
                    format!("hash-only:{}", hash.gallery_id),
                    "Classic hash rows exist without a matching artifact; they are audit evidence only",
                    false,
                );
            }
        }

        galleries.sort_by_key(|gallery| gallery.gallery_id);
        conflicts.sort_by(|left, right| left.conflict_id.cmp(&right.conflict_id));
        for favorite in &favorites {
            fingerprint.update(favorite.namespace.as_str().as_bytes());
            fingerprint.update(b":");
            fingerprint.update(favorite.value.as_bytes());
            fingerprint.update(b"\0");
        }
        let source_fingerprint = format!("{:x}", fingerprint.finalize());
        Ok(ClassicSourceInventory {
            data_root,
            download_root,
            data_root_label,
            download_root_label,
            plan: ClassicImportPlan {
                schema_version: CLASSIC_IMPORT_SCHEMA_VERSION,
                source_fingerprint,
                favorites,
                search_history,
                auto_find_exclusions,
                hidden_galleries,
                pair_exclusions,
                series,
                legacy_hashes,
                galleries,
                conflicts,
            },
        })
    }

    fn read_page(
        &self,
        data_root: &Path,
        download_root: Option<&Path>,
        page: &ClassicImportPagePlan,
        cancellation: &CancellationToken,
    ) -> Result<DownloadPagePayload, ApplicationError> {
        if cancellation.is_cancelled() {
            return Err(crate::application::DownloadPipelineError::cancelled().into());
        }
        let selected_root = match page.root_kind {
            ClassicSourceRootKind::Data => data_root,
            ClassicSourceRootKind::Downloads => download_root.ok_or_else(|| {
                ApplicationError::ClassicImportInvalid(
                    "the Classic download root is no longer available".into(),
                )
            })?,
        };
        let root = canonical_directory(selected_root)?;
        let path = resolve_read_only_path(&root, &page.relative_path)?;
        let bytes = read_bounded_file(&path, MAX_IMAGE_BYTES, "Classic image")?;
        let actual_sha = format!("{:x}", Sha256::digest(&bytes));
        if actual_sha != page.sha256 || bytes.len() as u64 != page.byte_length {
            return Err(ApplicationError::ClassicImportSourceChanged);
        }
        let (source_format, image_format) = source_image_format(&bytes)?;
        let image = decode_image(&bytes, image_format)?;
        let (width, height) = image.dimensions();
        Ok(DownloadPagePayload {
            source_page_number: SourcePageNumber::new(page.source_page)?,
            bytes,
            source_revision: format!("classic-import-v1:{actual_sha}"),
            source_format,
            width,
            height,
            candidate_index: 0,
        })
    }
}

#[derive(Debug, Clone)]
struct ClassicStateDownload {
    gallery_id: i64,
    title: String,
    artist: Option<String>,
    group: Option<String>,
    pages: Option<u32>,
    completed: bool,
    path: Option<PathBuf>,
}

#[derive(Debug)]
struct ScannedFolder {
    canonical_path: PathBuf,
    relative_path: String,
    manifest: Option<Value>,
    manifest_error: bool,
    images: BTreeMap<u32, ScannedPage>,
}

#[derive(Debug, Clone)]
struct ScannedPage {
    relative_path: String,
    byte_length: u64,
    sha256: String,
}

fn canonical_directory(path: &Path) -> Result<PathBuf, ApplicationError> {
    let canonical = path.canonicalize().map_err(|_| {
        ApplicationError::ClassicImportInvalid(
            "the selected Classic folder does not exist or cannot be read".into(),
        )
    })?;
    if !canonical.is_dir() {
        return Err(ApplicationError::ClassicImportInvalid(
            "the selected Classic path is not a folder".into(),
        ));
    }
    Ok(canonical)
}

fn locate_data_root(selected: &Path) -> PathBuf {
    let nested = selected.join("AtsumiData");
    if nested.is_dir()
        && (nested.join("state.json").is_file()
            || nested.join("state.json.bak").is_file()
            || nested.join("atsumi_cache.sqlite").is_file())
    {
        nested.canonicalize().unwrap_or(nested)
    } else {
        selected.to_path_buf()
    }
}

fn path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("selected folder")
        .to_owned()
}

fn read_classic_state(
    data_root: &Path,
    fingerprint: &mut Sha256,
    conflicts: &mut Vec<ClassicImportConflict>,
) -> Result<serde_json::Map<String, Value>, ApplicationError> {
    let candidates = [
        data_root.join("state.json"),
        data_root.join("state.json.bak"),
    ];
    let mut state = None;
    for path in candidates {
        if !path.is_file() {
            continue;
        }
        let bytes = read_bounded_file(&path, MAX_STATE_BYTES, "Classic state")?;
        fingerprint.update(b"state\0");
        fingerprint.update(Sha256::digest(&bytes));
        match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Object(object)) => {
                state = Some(object);
                break;
            }
            _ => push_conflict(
                conflicts,
                ClassicConflictCode::StateInvalid,
                ClassicConflictSeverity::Blocking,
                None,
                "state-invalid".into(),
                "Classic state.json is malformed; folder inventory can still be reviewed",
                false,
            ),
        }
    }
    let mut state = state.unwrap_or_else(|| {
        push_conflict(
            conflicts,
            ClassicConflictCode::StateMissing,
            ClassicConflictSeverity::Warning,
            None,
            "state-missing".into(),
            "Classic state.json was not found; only manifest-backed folders can be imported",
            true,
        );
        serde_json::Map::new()
    });

    for name in [
        "classic-local-storage-export.json",
        "atsumi-localstorage-export.json",
        "localStorage-export.json",
    ] {
        let path = data_root.join(name);
        if !path.is_file() {
            continue;
        }
        let bytes = read_bounded_file(&path, MAX_STATE_BYTES, "Classic localStorage export")?;
        fingerprint.update(b"local-storage\0");
        fingerprint.update(Sha256::digest(&bytes));
        let Ok(Value::Object(mut exported)) = serde_json::from_slice::<Value>(&bytes) else {
            push_conflict(
                conflicts,
                ClassicConflictCode::StateInvalid,
                ClassicConflictSeverity::Warning,
                None,
                format!("local-storage-invalid:{name}"),
                "A Classic localStorage export is malformed and was ignored",
                true,
            );
            continue;
        };
        if let Some(Value::Object(values)) = exported.remove("values") {
            exported = values;
        }
        for (key, value) in exported {
            if key.starts_with("atsumi") {
                state.insert(key, value);
            }
        }
    }
    Ok(state)
}

fn parse_favorites(state: &serde_json::Map<String, Value>) -> Vec<FavoriteKey> {
    let Some(Value::Object(namespaces)) = state.get("atsumiFavorites") else {
        return Vec::new();
    };
    let mapping = [
        ("artists", FavoriteNamespace::Artist),
        ("groups", FavoriteNamespace::Group),
        ("series", FavoriteNamespace::Series),
        ("characters", FavoriteNamespace::Character),
        ("tags", FavoriteNamespace::Tag),
    ];
    let mut result = BTreeMap::<(FavoriteNamespace, String), FavoriteKey>::new();
    for (classic, namespace) in mapping {
        let Some(Value::Array(values)) = namespaces.get(classic) else {
            continue;
        };
        for value in values.iter().filter_map(Value::as_str) {
            if let Ok(key) = (FavoriteKey {
                namespace,
                value: value.to_owned(),
            })
            .normalized()
            {
                result.insert((key.namespace, key.value.clone()), key);
            }
        }
    }
    result.into_values().collect()
}

fn parse_search_history(state: &serde_json::Map<String, Value>) -> Vec<SearchRequest> {
    let Some(Value::Array(items)) = state.get("atsumiSearchHistory") else {
        return Vec::new();
    };
    let mut unique = BTreeMap::new();
    for text in items.iter().filter_map(Value::as_str) {
        let request = SearchRequest {
            text: text.to_owned(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            languages: Vec::<Language>::new(),
            sort: SearchSort::Recent,
            page_size: 20,
        };
        if let Ok(request) = request.normalized() {
            if !request.text.is_empty() {
                unique.insert(request.text.clone(), request);
            }
        }
    }
    unique.into_values().collect()
}

fn numeric_array(value: Option<&Value>) -> Vec<i64> {
    let mut result = value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(numeric_id)
        .collect::<Vec<_>>();
    result.sort_unstable();
    result.dedup();
    result
}

fn parse_pairs(value: Option<&Value>) -> Vec<ClassicPairPlan> {
    let mut pairs = BTreeSet::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(text) = item.as_str() else { continue };
        let Some((left, right)) = text.split_once(':') else {
            continue;
        };
        let (Ok(left), Ok(right)) = (left.trim().parse::<i64>(), right.trim().parse::<i64>())
        else {
            continue;
        };
        if left <= 0 || right <= 0 || left == right {
            continue;
        }
        pairs.insert((left.min(right), left.max(right)));
    }
    pairs
        .into_iter()
        .map(|(left_gallery_id, right_gallery_id)| ClassicPairPlan {
            left_gallery_id,
            right_gallery_id,
        })
        .collect()
}

fn parse_series(value: Option<&Value>) -> Vec<ClassicSeriesPlan> {
    let Some(Value::Object(groups)) = value else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for (parent, members) in groups {
        let Ok(parent_gallery_id) = parent.parse::<i64>() else {
            continue;
        };
        if parent_gallery_id <= 0 {
            continue;
        }
        let mut member_gallery_ids = numeric_array(Some(members));
        member_gallery_ids.retain(|id| *id != parent_gallery_id);
        if !member_gallery_ids.is_empty() {
            result.push(ClassicSeriesPlan {
                parent_gallery_id,
                member_gallery_ids,
            });
        }
    }
    result.sort_by_key(|group| group.parent_gallery_id);
    result
}

fn parse_state_downloads(value: Option<&Value>) -> HashMap<i64, ClassicStateDownload> {
    let mut result = HashMap::new();
    for item in value.and_then(Value::as_array).into_iter().flatten() {
        let Some(object) = item.as_object() else {
            continue;
        };
        let Some(gallery_id) = object.get("id").and_then(numeric_id) else {
            continue;
        };
        let status = object.get("status").and_then(Value::as_str).unwrap_or("");
        let title = object
            .get("title")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("Classic gallery")
            .trim()
            .to_owned();
        let artist = optional_string(object.get("artist"));
        let group = object
            .get("groups")
            .and_then(Value::as_array)
            .and_then(|groups| groups.first())
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| optional_string(object.get("group")));
        let pages = object
            .get("pages")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .filter(|value| *value > 0);
        let path = object
            .get("path")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from);
        result.insert(
            gallery_id,
            ClassicStateDownload {
                gallery_id,
                title,
                artist,
                group,
                pages,
                completed: matches!(status, "downloaded" | "completed"),
                path,
            },
        );
    }
    result
}

fn inspect_hash_database(
    data_root: &Path,
    fingerprint: &mut Sha256,
    conflicts: &mut Vec<ClassicImportConflict>,
) -> Result<Vec<ClassicLegacyHashSummary>, ApplicationError> {
    let path = data_root.join("atsumi_cache.sqlite");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let metadata = path.metadata().map_err(|_| {
        ApplicationError::ClassicImportInvalid("the Classic hash database cannot be read".into())
    })?;
    if metadata.len() > MAX_HASH_DATABASE_BYTES {
        push_conflict(
            conflicts,
            ClassicConflictCode::InventoryLimitReached,
            ClassicConflictSeverity::Blocking,
            None,
            "hash-database-too-large".into(),
            "The Classic hash database exceeds the safe inventory limit",
            false,
        );
        return Ok(Vec::new());
    }
    fingerprint.update(b"hash-db\0");
    fingerprint.update(metadata.len().to_le_bytes());
    fingerprint.update(
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
            .to_le_bytes(),
    );
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| {
        ApplicationError::ClassicImportInvalid("the Classic hash database is not readable".into())
    })?;
    connection
        .execute_batch("PRAGMA query_only = ON; PRAGMA busy_timeout = 1000;")
        .map_err(|_| {
            ApplicationError::ClassicImportInvalid(
                "the Classic hash database could not enter read-only mode".into(),
            )
        })?;
    let mut counts = BTreeMap::<i64, (u32, u32)>::new();
    for (table, index) in [("page_hashes", 0usize), ("file_hashes", 1usize)] {
        let exists = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                [table],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or_default()
            > 0;
        if !exists {
            continue;
        }
        let sql = format!("SELECT gallery_id, COUNT(*) FROM {table} GROUP BY gallery_id");
        let mut statement = connection.prepare(&sql).map_err(|_| {
            ApplicationError::ClassicImportInvalid(
                "the Classic hash schema could not be inventoried".into(),
            )
        })?;
        let rows = statement
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
            .map_err(|_| {
                ApplicationError::ClassicImportInvalid(
                    "the Classic hash rows could not be inventoried".into(),
                )
            })?;
        for row in rows {
            let (gallery_id, count) = row.map_err(|_| {
                ApplicationError::ClassicImportInvalid("a Classic hash row is malformed".into())
            })?;
            if gallery_id <= 0 {
                continue;
            }
            let count = u32::try_from(count.max(0)).unwrap_or(u32::MAX);
            let entry = counts.entry(gallery_id).or_default();
            if index == 0 {
                entry.0 = count;
            } else {
                entry.1 = count;
            }
        }
    }
    Ok(counts
        .into_iter()
        .map(
            |(gallery_id, (page_hashes, file_hashes))| ClassicLegacyHashSummary {
                gallery_id,
                page_hashes,
                file_hashes,
            },
        )
        .collect())
}

fn scan_download_galleries(
    data_root: &Path,
    download_root: &Path,
    state_downloads: &HashMap<i64, ClassicStateDownload>,
    hidden_galleries: &[i64],
    fingerprint: &mut Sha256,
    conflicts: &mut Vec<ClassicImportConflict>,
) -> Result<Vec<ClassicImportGalleryPlan>, ApplicationError> {
    let folders = discover_gallery_folders(download_root, fingerprint, conflicts)?;
    let mut folder_counts = BTreeMap::<i64, usize>::new();
    let mut plans = Vec::new();
    for folder in folders.into_iter().take(MAX_GALLERIES) {
        let state_match = state_downloads.values().find(|state| {
            state.path.as_deref().is_some_and(|path| {
                path.canonicalize()
                    .ok()
                    .is_some_and(|candidate| candidate == folder.canonical_path)
            })
        });
        let manifest_id = folder
            .manifest
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(numeric_id);
        let gallery_id = manifest_id.or_else(|| state_match.map(|state| state.gallery_id));
        let Some(gallery_id) = gallery_id else {
            let conflict_id = format!(
                "manifest-missing:{}",
                stable_short_hash(&folder.relative_path)
            );
            push_conflict(
                conflicts,
                ClassicConflictCode::ManifestMissing,
                ClassicConflictSeverity::Blocking,
                None,
                conflict_id,
                "A folder has numbered images but no manifest or matching Classic state entry",
                false,
            );
            continue;
        };
        *folder_counts.entry(gallery_id).or_default() += 1;
        let state = state_downloads.get(&gallery_id).or(state_match);
        let mut conflict_ids = Vec::new();
        let mut eligible = true;
        if folder.manifest_error {
            let id = format!("manifest-invalid:{gallery_id}");
            conflict_ids.push(id.clone());
            eligible = false;
            push_conflict(
                conflicts,
                ClassicConflictCode::ManifestInvalid,
                ClassicConflictSeverity::Blocking,
                Some(gallery_id),
                id,
                "The Classic download manifest is malformed",
                false,
            );
        }
        if folder.manifest.is_none() {
            let id = format!("manifest-missing:{gallery_id}");
            conflict_ids.push(id.clone());
            eligible = false;
            push_conflict(
                conflicts,
                ClassicConflictCode::ManifestMissing,
                ClassicConflictSeverity::Blocking,
                Some(gallery_id),
                id,
                "The Classic folder has no .atsumi-download.json and cannot be auto-completed",
                false,
            );
        }
        if let (Some(manifest_id), Some(state)) = (manifest_id, state) {
            if manifest_id != state.gallery_id {
                let id = format!("manifest-gallery-mismatch:{gallery_id}");
                conflict_ids.push(id.clone());
                eligible = false;
                push_conflict(
                    conflicts,
                    ClassicConflictCode::ManifestGalleryMismatch,
                    ClassicConflictSeverity::Blocking,
                    Some(gallery_id),
                    id,
                    "The manifest gallery ID differs from the Classic state folder mapping",
                    false,
                );
            }
        }
        if state.is_none() {
            let id = format!("folder-without-state:{gallery_id}");
            conflict_ids.push(id.clone());
            push_conflict(
                conflicts,
                ClassicConflictCode::FolderWithoutState,
                ClassicConflictSeverity::Warning,
                Some(gallery_id),
                id,
                "A valid manifest-backed folder is absent from the Classic UI list",
                true,
            );
        }
        if hidden_galleries.binary_search(&gallery_id).is_ok() {
            let id = format!("hidden-gallery-files:{gallery_id}");
            conflict_ids.push(id.clone());
            push_conflict(
                conflicts,
                ClassicConflictCode::HiddenGalleryHasFiles,
                ClassicConflictSeverity::Warning,
                Some(gallery_id),
                id,
                "Classic hides this gallery even though artifact files exist; both facts will be preserved",
                true,
            );
        }
        let manifest = folder.manifest.as_ref();
        let excluded_pages = manifest
            .and_then(|value| value.get("excludedPages"))
            .map(|value| numeric_u32_array(Some(value)))
            .unwrap_or_default();
        let expected_pages = manifest
            .and_then(|value| value.get("sourcePages").and_then(Value::as_u64))
            .or_else(|| state.and_then(|state| state.pages.map(u64::from)))
            .or_else(|| {
                manifest
                    .and_then(|value| value.get("expectedPages").and_then(Value::as_u64))
                    .map(|count| count.saturating_add(excluded_pages.len() as u64))
            })
            .or_else(|| {
                folder
                    .images
                    .keys()
                    .next_back()
                    .map(|page| u64::from(*page))
            })
            .and_then(|value| u32::try_from(value).ok())
            .unwrap_or_default();
        if expected_pages == 0 || expected_pages as usize > MAX_PAGES {
            let id = format!("expected-page-count:{gallery_id}");
            conflict_ids.push(id.clone());
            eligible = false;
            push_conflict(
                conflicts,
                ClassicConflictCode::ExpectedPageCountMismatch,
                ClassicConflictSeverity::Blocking,
                Some(gallery_id),
                id,
                "The expected source page count is missing or outside the safe limit",
                false,
            );
        }
        let mut pages = Vec::new();
        for source_page in 1..=expected_pages {
            let excluded = excluded_pages.binary_search(&source_page).is_ok();
            let source = if let Some(page) = folder.images.get(&source_page).cloned() {
                Some((ClassicSourceRootKind::Downloads, page))
            } else if excluded {
                find_quarantine_page(data_root, gallery_id, source_page, fingerprint)?
                    .map(|page| (ClassicSourceRootKind::Data, page))
            } else {
                None
            };
            let Some((root_kind, source)) = source else {
                let id = format!("missing-page:{gallery_id}:{source_page}");
                conflict_ids.push(id.clone());
                eligible = false;
                push_conflict(
                    conflicts,
                    ClassicConflictCode::MissingPage,
                    ClassicConflictSeverity::Blocking,
                    Some(gallery_id),
                    id,
                    "A required source page is missing from both the folder and Classic quarantine",
                    false,
                );
                continue;
            };
            pages.push(ClassicImportPagePlan {
                source_page,
                root_kind,
                relative_path: source.relative_path,
                byte_length: source.byte_length,
                sha256: source.sha256,
                excluded,
            });
        }
        if pages.len() != expected_pages as usize {
            eligible = false;
        }
        let planned_bytes = pages.iter().map(|page| page.byte_length).sum();
        plans.push(ClassicImportGalleryPlan {
            gallery_id,
            title: state
                .map(|state| state.title.clone())
                .or_else(|| manifest.and_then(|value| optional_string(value.get("title"))))
                .unwrap_or_else(|| format!("Classic gallery {gallery_id}")),
            artist: state
                .and_then(|state| state.artist.clone())
                .or_else(|| manifest.and_then(|value| optional_string(value.get("artist")))),
            group: state.and_then(|state| state.group.clone()),
            source_folder: folder.relative_path,
            relative_directory: None,
            expected_pages,
            pages,
            planned_bytes,
            eligible,
            conflict_ids,
        });
    }

    for gallery in &mut plans {
        if folder_counts
            .get(&gallery.gallery_id)
            .copied()
            .unwrap_or_default()
            <= 1
        {
            continue;
        }
        let id = format!("duplicate-gallery-folder:{}", gallery.gallery_id);
        gallery.eligible = false;
        if !gallery.conflict_ids.contains(&id) {
            gallery.conflict_ids.push(id.clone());
            push_conflict(
                conflicts,
                ClassicConflictCode::DuplicateGalleryFolder,
                ClassicConflictSeverity::Blocking,
                Some(gallery.gallery_id),
                id,
                "Multiple Classic folders claim the same gallery ID; none was merged automatically",
                false,
            );
        }
    }
    Ok(plans)
}

fn discover_gallery_folders(
    root: &Path,
    fingerprint: &mut Sha256,
    conflicts: &mut Vec<ClassicImportConflict>,
) -> Result<Vec<ScannedFolder>, ApplicationError> {
    let mut result = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut entries_seen = 0usize;
    while let Some((directory, depth)) = stack.pop() {
        if entries_seen >= MAX_SCAN_ENTRIES || result.len() >= MAX_GALLERIES {
            push_conflict(
                conflicts,
                ClassicConflictCode::InventoryLimitReached,
                ClassicConflictSeverity::Blocking,
                None,
                "inventory-limit".into(),
                "Classic folder inventory reached the safe scan limit",
                false,
            );
            break;
        }
        let mut image_paths = Vec::new();
        let mut child_directories = Vec::new();
        let read_dir = fs::read_dir(&directory).map_err(|_| {
            ApplicationError::ClassicImportInvalid(
                "a selected Classic download folder cannot be read".into(),
            )
        })?;
        for entry in read_dir.flatten() {
            entries_seen += 1;
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if depth < 3 && !is_ignored_directory(&path) {
                    child_directories.push(path);
                }
            } else if file_type.is_file() && page_number_from_path(&path).is_some() {
                image_paths.push(path);
            }
        }
        let manifest_path = directory.join(".atsumi-download.json");
        if manifest_path.is_file() || !image_paths.is_empty() {
            let (manifest, manifest_error) = if manifest_path.is_file() {
                let bytes = read_bounded_file(&manifest_path, MAX_STATE_BYTES, "Classic manifest")?;
                fingerprint.update(relative_text(root, &manifest_path)?.as_bytes());
                fingerprint.update(Sha256::digest(&bytes));
                match serde_json::from_slice::<Value>(&bytes) {
                    Ok(value @ Value::Object(_)) => (Some(value), false),
                    _ => (None, true),
                }
            } else {
                (None, false)
            };
            let mut images = BTreeMap::new();
            for path in image_paths {
                let source_page = page_number_from_path(&path).unwrap_or_default();
                if source_page == 0 || images.contains_key(&source_page) {
                    continue;
                }
                let page = inspect_page_file(root, &path, fingerprint)?;
                images.insert(source_page, page);
            }
            result.push(ScannedFolder {
                canonical_path: directory.canonicalize().unwrap_or(directory.clone()),
                relative_path: relative_text(root, &directory)?,
                manifest,
                manifest_error,
                images,
            });
            continue;
        }
        stack.extend(child_directories.into_iter().map(|path| (path, depth + 1)));
    }
    Ok(result)
}

fn find_quarantine_page(
    data_root: &Path,
    gallery_id: i64,
    source_page: u32,
    fingerprint: &mut Sha256,
) -> Result<Option<ScannedPage>, ApplicationError> {
    let root = data_root
        .join("quarantine")
        .join("internal-pages")
        .join(gallery_id.to_string());
    if !root.is_dir() {
        return Ok(None);
    }
    let mut matches = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).into_iter().flatten().flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file()
                && page_number_from_path(&entry.path()) == Some(source_page)
            {
                matches.push(entry.path());
            }
        }
    }
    if matches.len() != 1 {
        return Ok(None);
    }
    inspect_page_file(data_root, &matches[0], fingerprint).map(Some)
}

fn inspect_page_file(
    root: &Path,
    path: &Path,
    fingerprint: &mut Sha256,
) -> Result<ScannedPage, ApplicationError> {
    let bytes = read_bounded_file(path, MAX_IMAGE_BYTES, "Classic image")?;
    let (_, format) = source_image_format(&bytes)?;
    decode_image(&bytes, format)?;
    let sha256 = format!("{:x}", Sha256::digest(&bytes));
    let relative_path = relative_text(root, path)?;
    fingerprint.update(relative_path.as_bytes());
    fingerprint.update(sha256.as_bytes());
    Ok(ScannedPage {
        relative_path,
        byte_length: bytes.len() as u64,
        sha256,
    })
}

fn read_bounded_file(
    path: &Path,
    maximum: u64,
    label: &'static str,
) -> Result<Vec<u8>, ApplicationError> {
    let metadata = path.metadata().map_err(|_| {
        ApplicationError::ClassicImportInvalid(format!("{label} metadata cannot be read"))
    })?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(ApplicationError::ClassicImportInvalid(format!(
            "{label} is empty or exceeds the safe size limit"
        )));
    }
    let mut file = File::open(path)
        .map_err(|_| ApplicationError::ClassicImportInvalid(format!("{label} cannot be opened")))?;
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| ApplicationError::ClassicImportInvalid(format!("{label} cannot be read")))?;
    Ok(bytes)
}

fn source_image_format(
    bytes: &[u8],
) -> Result<(DownloadSourceImageFormat, ImageFormat), ApplicationError> {
    match image::guess_format(bytes).map_err(|_| {
        ApplicationError::ClassicImportInvalid(
            "a Classic page has an unsupported image signature".into(),
        )
    })? {
        ImageFormat::WebP => Ok((DownloadSourceImageFormat::Webp, ImageFormat::WebP)),
        ImageFormat::Jpeg => Ok((DownloadSourceImageFormat::Jpeg, ImageFormat::Jpeg)),
        ImageFormat::Png => Ok((DownloadSourceImageFormat::Png, ImageFormat::Png)),
        _ => Err(ApplicationError::ClassicImportInvalid(
            "a Classic page format cannot be imported safely".into(),
        )),
    }
}

fn decode_image(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<image::DynamicImage, ApplicationError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_IMAGE_DECODE_ALLOC);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| {
        ApplicationError::ClassicImportInvalid(
            "a Classic page could not be decoded within the safety limits".into(),
        )
    })?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(ApplicationError::ClassicImportInvalid(
            "a Classic page has invalid dimensions".into(),
        ));
    }
    Ok(image)
}

fn resolve_read_only_path(root: &Path, relative: &str) -> Result<PathBuf, ApplicationError> {
    let relative = Path::new(relative);
    if relative.is_absolute()
        || relative.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err(ApplicationError::ClassicImportInvalid(
            "a Classic inventory path escapes its selected root".into(),
        ));
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|_| ApplicationError::ClassicImportSourceChanged)?;
    if path == root || !path.starts_with(root) || !path.is_file() {
        return Err(ApplicationError::ClassicImportInvalid(
            "a Classic inventory path is outside its selected root".into(),
        ));
    }
    Ok(path)
}

fn relative_text(root: &Path, path: &Path) -> Result<String, ApplicationError> {
    let canonical = path.canonicalize().map_err(|_| {
        ApplicationError::ClassicImportInvalid("a Classic inventory path cannot be resolved".into())
    })?;
    if canonical == root || !canonical.starts_with(root) {
        return Err(ApplicationError::ClassicImportInvalid(
            "a Classic inventory path escapes its selected root".into(),
        ));
    }
    canonical
        .strip_prefix(root)
        .ok()
        .and_then(|relative| relative.to_str())
        .map(|relative| relative.replace('\\', "/"))
        .ok_or_else(|| {
            ApplicationError::ClassicImportInvalid(
                "a Classic inventory path is not valid Unicode".into(),
            )
        })
}

fn page_number_from_path(path: &Path) -> Option<u32> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    if !matches!(extension.as_str(), "webp" | "jpg" | "jpeg" | "png") {
        return None;
    }
    let stem = path.file_stem()?.to_str()?.trim();
    stem.parse::<u32>().ok().filter(|page| *page > 0)
}

fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with('.')
                || matches!(
                    name.to_ascii_lowercase().as_str(),
                    "node_modules" | "target" | "cache" | "quarantine"
                )
        })
}

fn numeric_id(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.trim().parse().ok()))
        .filter(|value| *value > 0)
}

fn numeric_u32_array(value: Option<&Value>) -> Vec<u32> {
    let mut result = value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
        .filter(|value| *value > 0)
        .collect::<Vec<_>>();
    result.sort_unstable();
    result.dedup();
    result
}

fn optional_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn stable_short_hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))[..16].to_owned()
}

fn push_conflict(
    conflicts: &mut Vec<ClassicImportConflict>,
    code: ClassicConflictCode,
    severity: ClassicConflictSeverity,
    gallery_id: Option<i64>,
    conflict_id: String,
    message: &str,
    requires_acknowledgement: bool,
) {
    if conflicts
        .iter()
        .any(|conflict| conflict.conflict_id == conflict_id)
    {
        return;
    }
    conflicts.push(ClassicImportConflict {
        conflict_id,
        code,
        severity,
        gallery_id,
        message: message.to_owned(),
        requires_acknowledgement,
    });
}
