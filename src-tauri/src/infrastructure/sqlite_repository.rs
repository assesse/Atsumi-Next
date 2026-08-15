use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{
    params, Connection, ErrorCode, OptionalExtension, Row, Transaction, TransactionBehavior,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{
        ArtifactRepository, AutomationRepository, DownloadArtifactPlan, DownloadCheckpoint,
        DownloadMutationOutcome, DownloadPageAttempt, DownloadPageAttemptResult,
        DownloadPipelineRepository, DownloadPrepared, DownloadQueueAddOutcome, DownloadQueueRecord,
        DownloadRepository, QuarantineSaga, QuarantineSagaState, RepositoryError, StateRepository,
        StoredPage,
    },
    domain::{
        ArtifactBundle, ArtifactManifest, ArtifactRelativePath, ArtifactSha256,
        ArtifactStorageFormat, AutoFindCandidate, AutoFindCandidateRecord, AutoFindExclusionResult,
        AutoFindRun, AutoFindRunState, AutoFindSnapshot, DownloadArtifact, DownloadArtifactState,
        DownloadChangedEvent, DownloadEntry, DownloadEntryId, DownloadJobDescriptor,
        DownloadJobProjection, DownloadListRequest, DownloadPage, DownloadReviewKind, FavoriteKey,
        FavoriteMutationResult, FavoriteNamespace, FavoriteRecord, FixtureDownloadJobStep, Gallery,
        GalleryId, GalleryMetadata, GallerySummary, JobEvent, JobRef, JobState, Language,
        PageArtifact, PageArtifactState, SearchHistoryEntry, SearchRequest, SearchSort,
        SettingsSnapshot, SourcePageNumber, WindowPlacementSnapshot,
    },
};

use super::migrations::{MigrationError, MigrationRunner};

pub struct SqliteRepository {
    connection: Mutex<Connection>,
}

impl SqliteRepository {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RepositoryError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                RepositoryError::Other(format!("could not create the database directory: {error}"))
            })?;
        }
        let existing_database = match std::fs::metadata(path) {
            Ok(metadata) => metadata.is_file() && metadata.len() > 0,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => {
                return Err(RepositoryError::Other(format!(
                    "could not inspect the database file: {error}"
                )))
            }
        };
        let connection = Connection::open(path).map_err(map_sqlite_error)?;
        Self::from_connection(
            connection,
            Some(FileDatabase {
                path,
                existing_database,
            }),
        )
    }

    pub fn open_in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(map_sqlite_error)?;
        Self::from_connection(connection, None)
    }

    fn from_connection(
        mut connection: Connection,
        file_database: Option<FileDatabase<'_>>,
    ) -> Result<Self, RepositoryError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(map_sqlite_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite_error)?;
        let backup_path = file_database
            .filter(|database| database.existing_database)
            .map(|database| database.path);
        let report = run_migrations_with_backup(&mut connection, backup_path)?;
        if file_database.is_some() {
            let journal_mode: String = connection
                .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
                .map_err(map_sqlite_error)?;
            if !journal_mode.eq_ignore_ascii_case("wal") {
                return Err(RepositoryError::Other(format!(
                    "SQLite refused WAL journal mode and returned {journal_mode:?}"
                )));
            }
            connection
                .execute_batch("PRAGMA synchronous = NORMAL;")
                .map_err(map_sqlite_error)?;
        }
        tracing::info!(
            schema_version = report.current_version,
            migrations_applied = ?report.applied_versions,
            "SQLite schema is ready"
        );
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    fn connection(&self) -> Result<MutexGuard<'_, Connection>, RepositoryError> {
        self.connection
            .lock()
            .map_err(|_| RepositoryError::Other("database mutex was poisoned".into()))
    }
}

#[derive(Clone, Copy)]
struct FileDatabase<'a> {
    path: &'a Path,
    existing_database: bool,
}

fn run_migrations_with_backup(
    connection: &mut Connection,
    existing_database_path: Option<&Path>,
) -> Result<super::migrations::MigrationReport, RepositoryError> {
    if let Some(database_path) = existing_database_path {
        let pending_versions =
            MigrationRunner::pending_versions(connection).map_err(map_migration_error)?;
        if let (Some(first_pending), Some(target_version)) =
            (pending_versions.first(), pending_versions.last())
        {
            let backup_path = create_pre_migration_backup(
                connection,
                database_path,
                first_pending - 1,
                *target_version,
            )?;
            tracing::info!(
                backup_file = %backup_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("pre-migration-backup.bak"),
                from_version = first_pending - 1,
                to_version = target_version,
                "Created a recoverable pre-migration database backup"
            );
        }
    }

    MigrationRunner::run(connection).map_err(map_migration_error)
}

fn create_pre_migration_backup(
    connection: &Connection,
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> Result<PathBuf, RepositoryError> {
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| {
            RepositoryError::MigrationBackup(format!(
                "the system clock is before the Unix epoch: {error}"
            ))
        })?
        .as_secs();
    let backup_path =
        next_pre_migration_backup_path(database_path, from_version, to_version, created_at)?;
    let backup_path_text = backup_path.to_str().ok_or_else(|| {
        RepositoryError::MigrationBackup(
            "the pre-migration backup path is not valid Unicode".into(),
        )
    })?;
    connection
        .execute("VACUUM main INTO ?1", [backup_path_text])
        .map_err(|error| {
            RepositoryError::MigrationBackup(format!(
                "could not create pre-migration backup {}: {error}",
                backup_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("snapshot.bak")
            ))
        })?;
    Ok(backup_path)
}

fn next_pre_migration_backup_path(
    database_path: &Path,
    from_version: i64,
    to_version: i64,
    created_at: u64,
) -> Result<PathBuf, RepositoryError> {
    let parent = database_path.parent().ok_or_else(|| {
        RepositoryError::MigrationBackup(
            "the database has no directory for a pre-migration backup".into(),
        )
    })?;
    let file_name = database_path.file_name().ok_or_else(|| {
        RepositoryError::MigrationBackup(
            "the database has no file name for a pre-migration backup".into(),
        )
    })?;

    for sequence in 0..10_000_u32 {
        let mut backup_name = file_name.to_os_string();
        backup_name.push(format!(
            ".pre-migration-v{from_version}-to-v{to_version}-{created_at}"
        ));
        if sequence > 0 {
            backup_name.push(format!("-{sequence}"));
        }
        backup_name.push(".bak");
        let candidate = parent.join(backup_name);
        match std::fs::symlink_metadata(&candidate) {
            Ok(_) => continue,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(candidate),
            Err(error) => {
                return Err(RepositoryError::MigrationBackup(format!(
                    "could not inspect a pre-migration backup path: {error}"
                )))
            }
        }
    }

    Err(RepositoryError::MigrationBackup(
        "could not reserve a non-overwriting pre-migration backup name".into(),
    ))
}

impl StateRepository for SqliteRepository {
    fn settings_get(&self) -> Result<SettingsSnapshot, RepositoryError> {
        let connection = self.connection()?;
        read_settings(&connection)
    }

    fn settings_compare_and_set(
        &self,
        next: &SettingsSnapshot,
        expected_revision: u64,
    ) -> Result<bool, RepositoryError> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                r#"
                    UPDATE settings
                    SET revision = ?1,
                        download_root = ?2,
                        max_columns = ?3,
                        preview_width = ?4,
                        cache_limit_gb = ?5,
                        concurrent_image_requests = ?6,
                        request_start_interval_ms = ?7
                    WHERE singleton = 1 AND revision = ?8
                "#,
                params![
                    to_sql_integer(next.revision, "settings revision")?,
                    next.download_root,
                    i64::from(next.max_columns),
                    i64::from(next.preview_width),
                    i64::from(next.cache_limit_gb),
                    i64::from(next.concurrent_image_requests),
                    to_sql_integer(next.request_start_interval_ms, "request start interval")?,
                    to_sql_integer(expected_revision, "expected settings revision")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(changed == 1)
    }

    fn window_placement_get(&self) -> Result<WindowPlacementSnapshot, RepositoryError> {
        let connection = self.connection()?;
        read_window_placement(&connection)
    }

    fn window_placement_compare_and_set(
        &self,
        next: &WindowPlacementSnapshot,
        expected_revision: u64,
    ) -> Result<bool, RepositoryError> {
        let connection = self.connection()?;
        let changed = connection
            .execute(
                r#"
                    UPDATE window_placement
                    SET revision = ?1,
                        x = ?2,
                        y = ?3,
                        width = ?4,
                        height = ?5,
                        maximized = ?6
                    WHERE singleton = 1 AND revision = ?7
                "#,
                params![
                    to_sql_integer(next.revision, "window placement revision")?,
                    next.x,
                    next.y,
                    i64::from(next.width),
                    i64::from(next.height),
                    next.maximized,
                    to_sql_integer(expected_revision, "expected window placement revision")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(changed == 1)
    }
}

impl AutomationRepository for SqliteRepository {
    fn favorites_list(&self) -> Result<Vec<FavoriteRecord>, RepositoryError> {
        let connection = self.connection()?;
        read_favorites(&connection)
    }

    fn favorite_set(
        &self,
        key: &FavoriteKey,
        enabled: bool,
    ) -> Result<FavoriteMutationResult, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        if enabled {
            transaction
                .execute(
                    r#"
                        INSERT INTO favorites (
                            namespace, value, revision, created_at, updated_at
                        ) VALUES (
                            ?1, ?2, 0,
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                        ON CONFLICT(namespace, value) DO UPDATE SET
                            revision = favorites.revision + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    "#,
                    params![key.namespace.as_str(), key.value],
                )
                .map_err(map_sqlite_error)?;
            let favorite = read_favorite(&transaction, key)?.ok_or_else(|| {
                RepositoryError::Corrupt("favorite upsert did not produce a row".into())
            })?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(FavoriteMutationResult {
                enabled: true,
                favorite: Some(favorite),
            })
        } else {
            transaction
                .execute(
                    "DELETE FROM favorites WHERE namespace = ?1 AND value = ?2",
                    params![key.namespace.as_str(), key.value],
                )
                .map_err(map_sqlite_error)?;
            transaction.commit().map_err(map_sqlite_error)?;
            Ok(FavoriteMutationResult {
                enabled: false,
                favorite: None,
            })
        }
    }

    fn search_history_record(
        &self,
        request: &SearchRequest,
    ) -> Result<SearchHistoryEntry, RepositoryError> {
        let canonical = serde_json::to_vec(request)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let fingerprint = format!("{:x}", Sha256::digest(canonical));
        let include_tags = serde_json::to_string(&request.include_tags)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let exclude_tags = serde_json::to_string(&request.exclude_tags)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let languages = serde_json::to_string(&request.languages)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let connection = self.connection()?;
        connection
            .execute(
                r#"
                    INSERT INTO search_history (
                        fingerprint, text, include_tags_json, exclude_tags_json,
                        languages_json, sort, page_size, use_count, last_used_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, 1,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                    ON CONFLICT(fingerprint) DO UPDATE SET
                        use_count = search_history.use_count + 1,
                        last_used_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                "#,
                params![
                    fingerprint,
                    request.text,
                    include_tags,
                    exclude_tags,
                    languages,
                    search_sort_text(request.sort),
                    i64::from(request.page_size),
                ],
            )
            .map_err(map_sqlite_error)?;
        read_search_history_by_fingerprint(&connection, &fingerprint)?.ok_or_else(|| {
            RepositoryError::Corrupt("search history upsert did not produce a row".into())
        })
    }

    fn search_history_list(&self, limit: u32) -> Result<Vec<SearchHistoryEntry>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                    SELECT history_id, text, include_tags_json, exclude_tags_json,
                           languages_json, sort, page_size, use_count, last_used_at
                    FROM search_history
                    ORDER BY last_used_at DESC, history_id DESC
                    LIMIT ?1
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([i64::from(limit)], stored_search_history)
            .map_err(map_sqlite_error)?;
        rows.map(|row| row.map_err(map_sqlite_error)?.try_into_domain())
            .collect()
    }

    fn auto_find_recover_interrupted(&self) -> Result<usize, RepositoryError> {
        let connection = self.connection()?;
        connection
            .execute(
                r#"
                    UPDATE auto_find_runs
                    SET revision = revision + 1,
                        state = 'failed',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        error_code = 'AUTO_FIND_INTERRUPTED',
                        error_message = 'The previous Auto Find refresh stopped before completion'
                    WHERE state = 'running'
                "#,
                [],
            )
            .map_err(map_sqlite_error)
    }

    fn auto_find_start(&self, total_favorites: u32) -> Result<AutoFindRun, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        if let Some(existing) = read_running_auto_find(&transaction)? {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(existing);
        }
        let run_id = format!("auto-find-{}", Uuid::new_v4());
        transaction
            .execute(
                r#"
                    INSERT INTO auto_find_runs (
                        run_id, revision, state, total_favorites,
                        completed_favorites, candidates_found,
                        started_at, updated_at
                    ) VALUES (
                        ?1, 0, 'running', ?2, 0, 0,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                "#,
                params![run_id, i64::from(total_favorites)],
            )
            .map_err(map_sqlite_error)?;
        let run = read_auto_find_run(&transaction, &run_id)?.ok_or_else(|| {
            RepositoryError::Corrupt("Auto Find start did not produce a run".into())
        })?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(run)
    }

    fn auto_find_candidate_add(
        &self,
        candidate: &AutoFindCandidateRecord,
    ) -> Result<Option<AutoFindRun>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        if !auto_find_run_is_running(&transaction, &candidate.run_id)? {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(None);
        }
        let excluded: bool = transaction
            .query_row(
                r#"
                    SELECT EXISTS (
                        SELECT 1 FROM auto_find_exclusions WHERE gallery_id = ?1
                        UNION ALL
                        SELECT 1 FROM download_entries WHERE gallery_id = ?1
                    )
                "#,
                [candidate.gallery.id.get()],
                |row| row.get(0),
            )
            .map_err(map_sqlite_error)?;
        if excluded {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(None);
        }
        let tags = serde_json::to_string(&candidate.gallery.tags)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let series = serde_json::to_string(&candidate.gallery.series)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let characters = serde_json::to_string(&candidate.gallery.characters)
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO auto_find_candidates (
                        run_id, gallery_id, title, artist, group_name, pages,
                        language, tags_json, series_json, characters_json,
                        published_rank, popularity,
                        thumbnail_key, thumbnail_width, thumbnail_height,
                        favorite_namespace, favorite_value, discovered_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                        ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                "#,
                params![
                    candidate.run_id,
                    candidate.gallery.id.get(),
                    candidate.gallery.title,
                    candidate.gallery.artist,
                    candidate.gallery.group,
                    i64::from(candidate.gallery.pages),
                    language_text(candidate.gallery.language),
                    tags,
                    series,
                    characters,
                    i64::from(candidate.gallery.published_rank),
                    i64::from(candidate.gallery.popularity),
                    candidate.gallery.thumbnail_key,
                    i64::from(candidate.gallery.thumbnail_width),
                    i64::from(candidate.gallery.thumbnail_height),
                    candidate.matched_favorite.namespace.as_str(),
                    candidate.matched_favorite.value,
                ],
            )
            .map_err(map_sqlite_error)?;
        if inserted == 1 {
            transaction
                .execute(
                    r#"
                        UPDATE auto_find_runs
                        SET revision = revision + 1,
                            candidates_found = candidates_found + 1,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        WHERE run_id = ?1 AND state = 'running'
                    "#,
                    [candidate.run_id.as_str()],
                )
                .map_err(map_sqlite_error)?;
        }
        let run = read_auto_find_run(&transaction, &candidate.run_id)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(run.filter(|_| inserted == 1))
    }

    fn auto_find_progress(
        &self,
        run_id: &str,
        completed_favorites: u32,
    ) -> Result<Option<AutoFindRun>, RepositoryError> {
        let connection = self.connection()?;
        connection
            .execute(
                r#"
                    UPDATE auto_find_runs
                    SET revision = revision + 1,
                        completed_favorites = MIN(total_favorites, MAX(completed_favorites, ?2)),
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE run_id = ?1 AND state = 'running'
                "#,
                params![run_id, i64::from(completed_favorites)],
            )
            .map_err(map_sqlite_error)?;
        read_auto_find_run(&connection, run_id)
    }

    fn auto_find_finish(
        &self,
        run_id: &str,
        state: AutoFindRunState,
        error_code: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<Option<AutoFindRun>, RepositoryError> {
        if state == AutoFindRunState::Running {
            return Err(RepositoryError::Other(
                "Auto Find finish cannot keep a run in running state".into(),
            ));
        }
        let connection = self.connection()?;
        connection
            .execute(
                r#"
                    UPDATE auto_find_runs
                    SET revision = revision + 1,
                        state = ?2,
                        completed_favorites = CASE
                            WHEN ?2 = 'completed' THEN total_favorites
                            ELSE completed_favorites
                        END,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        error_code = ?3,
                        error_message = ?4
                    WHERE run_id = ?1 AND state = 'running'
                "#,
                params![run_id, state.as_str(), error_code, error_message],
            )
            .map_err(map_sqlite_error)?;
        read_auto_find_run(&connection, run_id)
    }

    fn auto_find_is_running(&self, run_id: &str) -> Result<bool, RepositoryError> {
        let connection = self.connection()?;
        auto_find_run_is_running(&connection, run_id)
    }

    fn auto_find_snapshot(&self) -> Result<AutoFindSnapshot, RepositoryError> {
        let connection = self.connection()?;
        read_auto_find_snapshot(&connection)
    }

    fn auto_find_exclude(
        &self,
        gallery_ids: &[GalleryId],
        reason: &str,
    ) -> Result<AutoFindExclusionResult, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        for gallery_id in gallery_ids {
            transaction
                .execute(
                    r#"
                        INSERT INTO auto_find_exclusions (gallery_id, reason, created_at)
                        VALUES (
                            ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                        ON CONFLICT(gallery_id) DO UPDATE SET reason = excluded.reason
                    "#,
                    params![gallery_id.get(), reason],
                )
                .map_err(map_sqlite_error)?;
        }
        let snapshot = read_auto_find_snapshot(&transaction)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(AutoFindExclusionResult {
            excluded_gallery_ids: gallery_ids.to_vec(),
            snapshot,
        })
    }
}

impl DownloadRepository for SqliteRepository {
    fn download_recover_interrupted(&self) -> Result<usize, RepositoryError> {
        let mut connection = self.connection()?;
        recover_volatile_downloads(&mut connection)
    }

    fn download_queue_add(
        &self,
        request_id: &str,
        galleries: &[GalleryId],
    ) -> Result<DownloadQueueAddOutcome, RepositoryError> {
        let normalized_galleries = serde_json::to_string(
            &galleries
                .iter()
                .map(|gallery| gallery.get())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| RepositoryError::Other(error.to_string()))?;
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;

        let existing_batch = transaction
            .query_row(
                r#"
                    SELECT normalized_galleries
                    FROM download_queue_requests
                    WHERE request_id = ?1
                "#,
                [request_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;

        if let Some(existing_batch) = existing_batch {
            if existing_batch != normalized_galleries {
                transaction.commit().map_err(map_sqlite_error)?;
                return Ok(DownloadQueueAddOutcome::IdempotencyConflict);
            }

            let entries = read_request_entries(&transaction, request_id)?;
            if entries.len() != galleries.len() {
                return Err(RepositoryError::Corrupt(format!(
                    "download queue request {request_id:?} has an incomplete entry mapping"
                )));
            }
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(DownloadQueueAddOutcome::Added(DownloadQueueRecord {
                entries,
                jobs: Vec::new(),
            }));
        }

        transaction
            .execute(
                r#"
                    INSERT INTO download_queue_requests (
                        request_id, normalized_galleries
                    ) VALUES (?1, ?2)
                "#,
                params![request_id, normalized_galleries],
            )
            .map_err(map_sqlite_error)?;

        let mut jobs = Vec::new();
        for (position, gallery_id) in galleries.iter().enumerate() {
            let existing_entry_id = transaction
                .query_row(
                    r#"
                        SELECT entry_id
                        FROM download_entries
                        WHERE gallery_id = ?1
                          AND state IN (
                              'queued', 'resolving_metadata', 'downloading',
                              'hashing', 'verifying', 'retry_wait'
                          )
                        LIMIT 1
                    "#,
                    [gallery_id.get()],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;

            let entry_id = match existing_entry_id {
                Some(entry_id) => entry_id,
                None => {
                    let entry_id = format!("entry-{}", Uuid::new_v4());
                    let job_id = format!("job-{}", Uuid::new_v4());
                    let job_request_id = format!("download-{}", Uuid::new_v4());
                    transaction
                        .execute(
                            r#"
                                INSERT INTO download_entries (
                                    entry_id, gallery_id, revision, state, progress,
                                    created_at, updated_at
                                ) VALUES (
                                    ?1, ?2, 0, 'queued', 0.0,
                                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                                )
                            "#,
                            params![entry_id, gallery_id.get()],
                        )
                        .map_err(map_sqlite_error)?;
                    transaction
                        .execute(
                            r#"
                                INSERT INTO download_jobs (
                                    job_id, request_id, entry_id, gallery_id,
                                    revision, state, completed_units, total_units,
                                    attempt, created_at, updated_at
                                ) VALUES (
                                    ?1, ?2, ?3, ?4, 0, 'queued', 0, 1, 1,
                                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                                )
                            "#,
                            params![job_id, job_request_id, entry_id, gallery_id.get()],
                        )
                        .map_err(map_sqlite_error)?;
                    transaction
                        .execute(
                            r#"
                                INSERT INTO download_attempts (
                                    job_id, attempt, started_at
                                ) VALUES (
                                    ?1, 1,
                                    strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                                )
                            "#,
                            [&job_id],
                        )
                        .map_err(map_sqlite_error)?;
                    jobs.push(DownloadJobDescriptor {
                        job_id,
                        entry_id: entry_id.clone(),
                        gallery_id: *gallery_id,
                        worker_attempt: 1,
                    });
                    entry_id
                }
            };

            transaction
                .execute(
                    r#"
                        INSERT INTO download_queue_request_entries (
                            request_id, position, gallery_id, entry_id,
                            response_revision, response_state, response_progress,
                            response_review_kind, response_review_id
                        )
                        SELECT ?1, ?2, ?3, d.entry_id,
                               d.revision, d.state, d.progress, d.review_kind, d.review_id
                        FROM download_entries d
                        WHERE d.entry_id = ?4
                    "#,
                    params![
                        request_id,
                        to_sql_integer(position as u64, "queue position")?,
                        gallery_id.get(),
                        entry_id,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }

        let entries = read_request_entries(&transaction, request_id)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(DownloadQueueAddOutcome::Added(DownloadQueueRecord {
            entries,
            jobs,
        }))
    }

    fn download_entries_list(
        &self,
        request: &DownloadListRequest,
    ) -> Result<DownloadPage, RepositoryError> {
        let connection = self.connection()?;
        let state = request.state.map(|state| state.to_string());
        let query = request.query.as_deref();
        let total_items = connection
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_entries d
                    WHERE (?1 IS NULL OR d.state = ?1)
                      AND (
                          ?2 IS NULL
                          OR instr(lower(d.entry_id), ?2) > 0
                          OR instr(CAST(d.gallery_id AS TEXT), ?2) > 0
                      )
                "#,
                params![state, query],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let total_items = stored_u64(total_items, "download total items")?;
        let offset = u64::from(request.page - 1)
            .checked_mul(u64::from(request.page_size))
            .ok_or_else(|| RepositoryError::Other("download list offset overflowed".into()))?;

        let mut statement = connection
            .prepare(
                r#"
                    SELECT
                        d.entry_id, d.gallery_id, d.revision, d.state, d.progress,
                        d.review_kind, d.review_id,
                        j.attempt, j.last_error_code, j.last_error_message
                    FROM download_entries d
                    JOIN download_jobs j
                      ON j.entry_id = d.entry_id AND j.gallery_id = d.gallery_id
                    WHERE (?1 IS NULL OR d.state = ?1)
                      AND (
                          ?2 IS NULL
                          OR instr(lower(d.entry_id), ?2) > 0
                          OR instr(CAST(d.gallery_id AS TEXT), ?2) > 0
                      )
                    ORDER BY d.gallery_id ASC, d.entry_id ASC
                    LIMIT ?3 OFFSET ?4
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map(
                params![
                    state,
                    query,
                    i64::from(request.page_size),
                    to_sql_integer(offset, "download list offset")?,
                ],
                stored_download_entry,
            )
            .map_err(map_sqlite_error)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(map_sqlite_error)?.try_into_domain()?);
        }

        Ok(DownloadPage {
            page: request.page,
            total_items,
            entries,
        })
    }

    fn download_active_count(&self) -> Result<u64, RepositoryError> {
        let connection = self.connection()?;
        let count = connection
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_entries
                    WHERE state IN (
                        'queued', 'resolving_metadata', 'downloading',
                        'hashing', 'verifying', 'retry_wait'
                    )
                "#,
                [],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        stored_u64(count, "active download count")
    }

    fn download_retry(
        &self,
        entry_ids: &[DownloadEntryId],
    ) -> Result<DownloadMutationOutcome<Vec<JobRef>>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let mut job_refs = Vec::with_capacity(entry_ids.len());

        for entry_id in entry_ids {
            let Some(target) = read_download_target(&transaction, entry_id)? else {
                return Ok(DownloadMutationOutcome::EntryNotFound(entry_id.clone()));
            };

            if target.state.is_active() {
                job_refs.push(JobRef {
                    job_id: target.job_id,
                    reused: true,
                    worker_attempt: stored_u64(target.attempt, "download attempt")?,
                });
                continue;
            }
            if !target.state.is_retryable() || !target.state.allows_transition_to(JobState::Queued)
            {
                return Ok(DownloadMutationOutcome::InvalidState {
                    entry_id: entry_id.clone(),
                    state: target.state,
                });
            }

            if let Some(active_job_id) =
                active_job_for_gallery(&transaction, target.gallery_id, target.entry_id.as_str())?
            {
                job_refs.push(JobRef {
                    job_id: active_job_id,
                    reused: true,
                    worker_attempt: stored_u64(target.attempt, "download attempt")?,
                });
                continue;
            }

            let job_revision = next_stored_revision(target.job_revision, "job revision")?;
            let entry_revision = next_stored_revision(target.entry_revision, "download revision")?;
            let attempt = next_stored_revision(target.attempt, "download attempt")?;
            let changed_jobs = transaction
                .execute(
                    r#"
                        UPDATE download_jobs
                        SET revision = ?1,
                            state = 'queued',
                            attempt = ?2,
                            completed_units = 0,
                            total_units = 1,
                            last_error_code = NULL,
                            last_error_message = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                            started_at = NULL,
                            finished_at = NULL
                        WHERE job_id = ?3 AND revision = ?4 AND state = ?5
                    "#,
                    params![
                        to_sql_integer(job_revision, "job revision")?,
                        to_sql_integer(attempt, "download attempt")?,
                        target.job_id,
                        target.job_revision,
                        target.state.to_string(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed_entries = transaction
                .execute(
                    r#"
                        UPDATE download_entries
                        SET revision = ?1,
                            state = 'queued',
                            progress = 0,
                            review_kind = NULL,
                            review_id = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        WHERE entry_id = ?2 AND revision = ?3 AND state = ?4
                    "#,
                    params![
                        to_sql_integer(entry_revision, "download revision")?,
                        target.entry_id.as_str(),
                        target.entry_revision,
                        target.state.to_string(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed_jobs != 1 || changed_entries != 1 {
                return Err(RepositoryError::Other(format!(
                    "download entry {:?} changed concurrently while retrying",
                    target.entry_id.as_str()
                )));
            }
            transaction
                .execute(
                    r#"
                        INSERT INTO download_attempts (
                            job_id, attempt, started_at
                        ) VALUES (
                            ?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                    "#,
                    params![target.job_id, to_sql_integer(attempt, "download attempt")?],
                )
                .map_err(map_sqlite_error)?;
            job_refs.push(JobRef {
                job_id: target.job_id,
                reused: false,
                worker_attempt: attempt,
            });
        }

        transaction.commit().map_err(map_sqlite_error)?;
        Ok(DownloadMutationOutcome::Applied(job_refs))
    }

    fn download_cancel(
        &self,
        entry_ids: &[DownloadEntryId],
    ) -> Result<DownloadMutationOutcome<Vec<DownloadEntry>>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;

        for entry_id in entry_ids {
            let Some(target) = read_download_target(&transaction, entry_id)? else {
                return Ok(DownloadMutationOutcome::EntryNotFound(entry_id.clone()));
            };
            if target.state == JobState::Cancelled {
                continue;
            }
            if !target.state.allows_transition_to(JobState::Cancelled) {
                return Ok(DownloadMutationOutcome::InvalidState {
                    entry_id: entry_id.clone(),
                    state: target.state,
                });
            }

            let job_revision = next_stored_revision(target.job_revision, "job revision")?;
            let entry_revision = next_stored_revision(target.entry_revision, "download revision")?;
            let changed_jobs = transaction
                .execute(
                    r#"
                        UPDATE download_jobs
                        SET revision = ?1,
                            state = 'cancelled',
                            last_error_code = CASE
                                WHEN ?4 IN ('interrupted', 'failed') THEN last_error_code
                                ELSE NULL
                            END,
                            last_error_message = CASE
                                WHEN ?4 IN ('interrupted', 'failed') THEN last_error_message
                                ELSE NULL
                            END,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                            finished_at = COALESCE(
                                finished_at,
                                strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                            )
                        WHERE job_id = ?2 AND revision = ?3 AND state = ?4
                    "#,
                    params![
                        to_sql_integer(job_revision, "job revision")?,
                        target.job_id,
                        target.job_revision,
                        target.state.to_string(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            let changed_entries = transaction
                .execute(
                    r#"
                        UPDATE download_entries
                        SET revision = ?1,
                            state = 'cancelled',
                            review_kind = NULL,
                            review_id = NULL,
                            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        WHERE entry_id = ?2 AND revision = ?3 AND state = ?4
                    "#,
                    params![
                        to_sql_integer(entry_revision, "download revision")?,
                        target.entry_id.as_str(),
                        target.entry_revision,
                        target.state.to_string(),
                    ],
                )
                .map_err(map_sqlite_error)?;
            if changed_jobs != 1 || changed_entries != 1 {
                return Err(RepositoryError::Other(format!(
                    "download entry {:?} changed concurrently while cancelling",
                    target.entry_id.as_str()
                )));
            }
            transaction
                .execute(
                    r#"
                        UPDATE download_attempts
                        SET finished_at = COALESCE(
                                finished_at,
                                strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                            ),
                            outcome_state = 'cancelled',
                            error_code = NULL,
                            error_message = NULL
                        WHERE job_id = ?1 AND attempt = ?2 AND finished_at IS NULL
                    "#,
                    params![target.job_id, target.attempt],
                )
                .map_err(map_sqlite_error)?;
        }

        let mut entries = Vec::with_capacity(entry_ids.len());
        for entry_id in entry_ids {
            let target = read_download_target(&transaction, entry_id)?.ok_or_else(|| {
                RepositoryError::Corrupt(format!(
                    "cancelled download entry {:?} disappeared",
                    entry_id.as_str()
                ))
            })?;
            entries.push(target.into_download_entry()?);
        }
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(DownloadMutationOutcome::Applied(entries))
    }

    fn fixture_download_job_advance(
        &self,
        job_id: &str,
        worker_attempt: u64,
        step: FixtureDownloadJobStep,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;

        let (
            entry_id,
            gallery_id,
            job_revision,
            download_revision,
            stored_job_state,
            stored_download_state,
            stored_attempt,
        ) = transaction
            .query_row(
                r#"
                    SELECT
                        j.entry_id, j.gallery_id, j.revision, d.revision,
                        j.state, d.state, j.attempt
                    FROM download_jobs j
                    JOIN download_entries d
                      ON d.entry_id = j.entry_id AND d.gallery_id = j.gallery_id
                    WHERE j.job_id = ?1
                "#,
                [job_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, i64>(6)?,
                    ))
                },
            )
            .map_err(map_sqlite_error)?;
        let current_attempt = stored_u64(stored_attempt, "download attempt")?;
        if current_attempt != worker_attempt {
            return Err(RepositoryError::Other(format!(
                "fixture job {job_id:?} worker attempt {worker_attempt} is stale; current attempt is {current_attempt}"
            )));
        }
        let current_job_state = stored_job_state
            .parse::<JobState>()
            .map_err(domain_corruption)?;
        let current_download_state = stored_download_state
            .parse::<JobState>()
            .map_err(domain_corruption)?;
        if current_job_state != current_download_state {
            return Err(RepositoryError::Corrupt(format!(
                "fixture job {job_id:?} and its download entry disagree on state"
            )));
        }
        if !step.follows(current_job_state) {
            return Err(RepositoryError::Other(format!(
                "fixture job {job_id:?} cannot advance from {current_job_state} to {}",
                step.state()
            )));
        }

        let job_revision = next_stored_revision(job_revision, "job revision")?;
        let download_revision = next_stored_revision(download_revision, "download revision")?;
        let state = step.state();
        let stored_state = state.to_string();
        let completed_units = step.completed_units();
        let total_units = step.total_units();
        let progress = completed_units as f64 / total_units as f64 * 100.0;
        let (last_error_code, last_error_message) = match step {
            FixtureDownloadJobStep::ResolvingMetadata => (None, None),
            FixtureDownloadJobStep::FoundationUnavailable => (
                Some("DOWNLOAD_FOUNDATION_UNAVAILABLE"),
                Some(step.message()),
            ),
        };

        let changed_jobs = transaction
            .execute(
                r#"
                    UPDATE download_jobs
                    SET revision = ?1,
                        state = ?2,
                        completed_units = ?3,
                        total_units = ?4,
                        last_error_code = ?5,
                        last_error_message = ?6,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        started_at = CASE
                            WHEN ?2 = 'resolving_metadata'
                            THEN COALESCE(
                                started_at,
                                strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                            )
                            ELSE started_at
                        END,
                        finished_at = CASE
                            WHEN ?2 = 'interrupted'
                            THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                            ELSE NULL
                        END
                    WHERE job_id = ?7 AND revision = ?8 AND state = ?9 AND attempt = ?10
                "#,
                params![
                    to_sql_integer(job_revision, "job revision")?,
                    stored_state,
                    to_sql_integer(completed_units, "completed units")?,
                    to_sql_integer(total_units, "total units")?,
                    last_error_code,
                    last_error_message,
                    job_id,
                    to_sql_integer(job_revision - 1, "expected job revision")?,
                    current_job_state.to_string(),
                    to_sql_integer(worker_attempt, "worker attempt")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        let changed_downloads = transaction
            .execute(
                r#"
                    UPDATE download_entries
                    SET revision = ?1,
                        state = ?2,
                        progress = ?3,
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE entry_id = ?4 AND revision = ?5 AND state = ?6
                "#,
                params![
                    to_sql_integer(download_revision, "download revision")?,
                    stored_state,
                    progress,
                    entry_id,
                    to_sql_integer(download_revision - 1, "expected download revision")?,
                    current_download_state.to_string(),
                ],
            )
            .map_err(map_sqlite_error)?;
        if changed_jobs != 1 || changed_downloads != 1 {
            return Err(RepositoryError::Other(format!(
                "fixture job {job_id:?} changed concurrently"
            )));
        }
        if step == FixtureDownloadJobStep::FoundationUnavailable {
            transaction
                .execute(
                    r#"
                        UPDATE download_attempts
                        SET finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                            outcome_state = 'interrupted',
                            error_code = ?1,
                            error_message = ?2
                        WHERE job_id = ?3 AND attempt = ?4
                    "#,
                    params![
                        last_error_code,
                        last_error_message,
                        job_id,
                        to_sql_integer(worker_attempt, "worker attempt")?,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        transaction.commit().map_err(map_sqlite_error)?;

        Ok(DownloadJobProjection {
            job: JobEvent {
                job_id: job_id.to_owned(),
                gallery_id: Some(gallery_id),
                revision: job_revision,
                state,
                completed_units: Some(completed_units),
                total_units: Some(total_units),
                message: Some(step.message().to_owned()),
            },
            download: DownloadChangedEvent {
                entry_id,
                gallery_id,
                revision: download_revision,
                state,
                progress: Some(progress),
                attempt: Some(worker_attempt),
                error_code: last_error_code.map(str::to_owned),
                error_message: last_error_message.map(str::to_owned),
            },
        })
    }
}

impl ArtifactRepository for SqliteRepository {
    fn artifact_bundle_replace(&self, bundle: &ArtifactBundle) -> Result<(), RepositoryError> {
        bundle
            .validate()
            .map_err(|error| RepositoryError::Other(error.to_string()))?;

        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;

        transaction
            .execute(
                r#"
                    INSERT INTO galleries (
                        gallery_id, revision, title, primary_artist, primary_group,
                        source_page_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT (gallery_id) DO UPDATE SET
                        revision = excluded.revision,
                        title = excluded.title,
                        primary_artist = excluded.primary_artist,
                        primary_group = excluded.primary_group,
                        source_page_count = excluded.source_page_count
                "#,
                params![
                    bundle.gallery.id.get(),
                    to_sql_integer(bundle.gallery.revision, "gallery revision")?,
                    bundle.gallery.metadata.title,
                    bundle.gallery.metadata.primary_artist,
                    bundle.gallery.metadata.primary_group,
                    i64::from(bundle.gallery.metadata.source_page_count),
                ],
            )
            .map_err(map_sqlite_error)?;

        let entry_gallery_id = transaction
            .query_row(
                "SELECT gallery_id FROM download_entries WHERE entry_id = ?1",
                [bundle.artifact.entry_id.as_str()],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        match entry_gallery_id {
            Some(gallery_id) if gallery_id == bundle.gallery.id.get() => {}
            Some(_) => {
                return Err(RepositoryError::Other(
                    "download artifact gallery does not match its download entry".into(),
                ));
            }
            None => {
                return Err(RepositoryError::Other(
                    "download artifact requires an existing download entry".into(),
                ));
            }
        }

        transaction
            .execute(
                r#"
                    INSERT INTO download_artifacts (
                        entry_id, gallery_id, revision, relative_directory,
                        expected_page_count, state, manifest_relative_path,
                        manifest_schema_version, writer_version,
                        hash_profile_version, completed_at
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    ON CONFLICT (entry_id) DO UPDATE SET
                        gallery_id = excluded.gallery_id,
                        revision = excluded.revision,
                        relative_directory = excluded.relative_directory,
                        expected_page_count = excluded.expected_page_count,
                        state = excluded.state,
                        manifest_relative_path = excluded.manifest_relative_path,
                        manifest_schema_version = excluded.manifest_schema_version,
                        writer_version = excluded.writer_version,
                        hash_profile_version = excluded.hash_profile_version,
                        completed_at = excluded.completed_at
                "#,
                params![
                    bundle.artifact.entry_id.as_str(),
                    bundle.artifact.gallery_id.get(),
                    to_sql_integer(bundle.artifact.revision, "download artifact revision")?,
                    bundle.artifact.relative_directory.as_str(),
                    i64::from(bundle.artifact.expected_page_count),
                    bundle.artifact.state.as_str(),
                    bundle
                        .artifact
                        .manifest_relative_path
                        .as_ref()
                        .map(ArtifactRelativePath::as_str),
                    bundle.artifact.manifest_schema_version.map(i64::from),
                    bundle.artifact.writer_version,
                    i64::from(bundle.artifact.hash_profile_version),
                    bundle.artifact.completed_at,
                ],
            )
            .map_err(map_sqlite_error)?;

        transaction
            .execute(
                "DELETE FROM download_pages WHERE entry_id = ?1",
                [bundle.artifact.entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        for page in &bundle.pages {
            transaction
                .execute(
                    r#"
                        INSERT INTO download_pages (
                            entry_id, gallery_id, source_page_number,
                            relative_path, state, byte_length, sha256,
                            storage_format, source_revision, verified_at, excluded
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                    "#,
                    params![
                        page.entry_id.as_str(),
                        page.page_id.gallery_id.get(),
                        i64::from(page.page_id.source_page_number.get()),
                        page.relative_path.as_str(),
                        page.state.as_str(),
                        page.byte_length
                            .map(|value| to_sql_integer(value, "page byte length"))
                            .transpose()?,
                        page.sha256.as_ref().map(ArtifactSha256::as_str),
                        page.storage_format.map(ArtifactStorageFormat::as_str),
                        page.source_revision,
                        page.verified_at,
                        page.excluded,
                    ],
                )
                .map_err(map_sqlite_error)?;
        }

        transaction.commit().map_err(map_sqlite_error)
    }

    fn artifact_bundle_get(
        &self,
        entry_id: &DownloadEntryId,
    ) -> Result<Option<ArtifactBundle>, RepositoryError> {
        let connection = self.connection()?;
        let stored = connection
            .query_row(
                r#"
                    SELECT
                        g.gallery_id,
                        g.revision,
                        g.title,
                        g.primary_artist,
                        g.primary_group,
                        g.source_page_count,
                        a.revision,
                        a.relative_directory,
                        a.expected_page_count,
                        a.state,
                        a.manifest_relative_path,
                        a.manifest_schema_version,
                        a.writer_version,
                        a.hash_profile_version,
                        a.completed_at
                    FROM download_artifacts a
                    JOIN galleries g ON g.gallery_id = a.gallery_id
                    WHERE a.entry_id = ?1
                "#,
                [entry_id.as_str()],
                stored_artifact_bundle,
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let Some(stored) = stored else {
            return Ok(None);
        };

        let gallery_id = GalleryId::new(stored.gallery_id).map_err(domain_corruption)?;
        let metadata = GalleryMetadata::new(
            stored.title,
            stored.primary_artist,
            stored.primary_group,
            stored_u32(stored.source_page_count, "gallery source page count")?,
        )
        .map_err(domain_corruption)?;
        let gallery = Gallery::new(
            gallery_id,
            stored_u64(stored.gallery_revision, "gallery revision")?,
            metadata,
        );
        let mut artifact = DownloadArtifact::new(
            entry_id.clone(),
            gallery_id,
            stored_u64(stored.artifact_revision, "download artifact revision")?,
            ArtifactRelativePath::new(stored.relative_directory).map_err(domain_corruption)?,
            stored_u32(stored.expected_page_count, "expected page count")?,
            stored
                .artifact_state
                .parse::<DownloadArtifactState>()
                .map_err(domain_corruption)?,
        )
        .map_err(domain_corruption)?;
        artifact.hash_profile_version =
            stored_u32(stored.hash_profile_version, "artifact hash profile version")?;
        match (
            stored.manifest_relative_path,
            stored.manifest_schema_version,
            stored.writer_version,
            stored.completed_at,
        ) {
            (Some(path), Some(schema), Some(writer), Some(completed_at)) => {
                let hash_profile_version = artifact.hash_profile_version;
                artifact = artifact
                    .with_manifest(
                        ArtifactRelativePath::new(path).map_err(domain_corruption)?,
                        stored_u32(schema, "manifest schema version")?,
                        writer,
                        hash_profile_version,
                        completed_at,
                    )
                    .map_err(domain_corruption)?;
            }
            (Some(path), None, None, None) if artifact.state != DownloadArtifactState::Complete => {
                artifact.manifest_relative_path =
                    Some(ArtifactRelativePath::new(path).map_err(domain_corruption)?);
            }
            (None, None, None, None) => {}
            _ => {
                return Err(RepositoryError::Corrupt(
                    "download artifact has incomplete manifest metadata".into(),
                ));
            }
        }

        let mut statement = connection
            .prepare(
                r#"
                    SELECT gallery_id, source_page_number, relative_path, state, byte_length,
                           sha256, storage_format, source_revision, verified_at, excluded
                    FROM download_pages
                    WHERE entry_id = ?1
                    ORDER BY source_page_number ASC
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([entry_id.as_str()], stored_page_artifact)
            .map_err(map_sqlite_error)?;
        let mut pages = Vec::new();
        for row in rows {
            let stored = row.map_err(map_sqlite_error)?;
            let page_gallery_id = GalleryId::new(stored.gallery_id).map_err(domain_corruption)?;
            let source_page_number =
                SourcePageNumber::new(stored_u32(stored.source_page_number, "source page number")?)
                    .map_err(domain_corruption)?;
            let byte_length = stored
                .byte_length
                .map(|value| stored_u64(value, "page byte length"))
                .transpose()?;
            let mut page = PageArtifact::new(
                entry_id.clone(),
                page_gallery_id,
                source_page_number,
                ArtifactRelativePath::new(stored.relative_path).map_err(domain_corruption)?,
                stored
                    .page_state
                    .parse::<PageArtifactState>()
                    .map_err(domain_corruption)?,
                byte_length,
            )
            .map_err(domain_corruption)?;
            match (
                stored.sha256,
                stored.storage_format,
                stored.source_revision,
                stored.verified_at,
            ) {
                (Some(sha256), Some(format), Some(source_revision), Some(verified_at)) => {
                    page = page
                        .with_verification(
                            ArtifactSha256::new(sha256).map_err(domain_corruption)?,
                            format
                                .parse::<ArtifactStorageFormat>()
                                .map_err(domain_corruption)?,
                            source_revision,
                            verified_at,
                        )
                        .map_err(domain_corruption)?;
                }
                (None, None, None, None) => {}
                _ => {
                    return Err(RepositoryError::Corrupt(
                        "download page has incomplete verification metadata".into(),
                    ));
                }
            }
            pages.push(page.with_excluded(stored.excluded));
        }

        ArtifactBundle::new(gallery, artifact, pages)
            .map(Some)
            .map_err(domain_corruption)
    }
}

impl DownloadPipelineRepository for SqliteRepository {
    fn pipeline_begin(
        &self,
        descriptor: &DownloadJobDescriptor,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_pipeline_target(&transaction, descriptor)?;
        if target.state != JobState::Queued {
            if target.state == JobState::ResolvingMetadata {
                let projection =
                    target.into_projection(Some("Metadata resolution is already active"));
                transaction.commit().map_err(map_sqlite_error)?;
                return projection;
            }
            return Err(invalid_pipeline_state(&target, "begin"));
        }
        let projection = transition_pipeline_target(
            &transaction,
            target,
            JobState::ResolvingMetadata,
            None,
            None,
            None,
            None,
            "Resolving gallery metadata",
        )?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_prepare(
        &self,
        plan: &DownloadArtifactPlan,
    ) -> Result<DownloadPrepared, RepositoryError> {
        if plan.source_pages.is_empty() {
            return Err(RepositoryError::Other(
                "download artifact plan must contain at least one source page".into(),
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_pipeline_target(&transaction, &plan.descriptor)?;
        if target.state != JobState::ResolvingMetadata {
            return Err(invalid_pipeline_state(&target, "prepare artifact"));
        }

        transaction
            .execute(
                r#"
                    INSERT INTO galleries (
                        gallery_id, revision, title, primary_artist, primary_group,
                        source_page_count
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT (gallery_id) DO UPDATE SET
                        revision = excluded.revision,
                        title = excluded.title,
                        primary_artist = excluded.primary_artist,
                        primary_group = excluded.primary_group,
                        source_page_count = excluded.source_page_count
                "#,
                params![
                    plan.gallery.id.get(),
                    to_sql_integer(plan.gallery.revision, "gallery revision")?,
                    plan.gallery.metadata.title,
                    plan.gallery.metadata.primary_artist,
                    plan.gallery.metadata.primary_group,
                    i64::from(plan.gallery.metadata.source_page_count),
                ],
            )
            .map_err(map_sqlite_error)?;

        let previous_artifact_revision = transaction
            .query_row(
                "SELECT revision FROM download_artifacts WHERE entry_id = ?1",
                [&plan.descriptor.entry_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        let artifact_revision = previous_artifact_revision
            .map(|revision| next_stored_revision(revision, "artifact revision"))
            .transpose()?
            .unwrap_or(0);
        transaction
            .execute(
                r#"
                    INSERT INTO download_artifacts (
                        entry_id, gallery_id, revision, relative_directory,
                        expected_page_count, state, manifest_relative_path,
                        hash_profile_version
                    ) VALUES (?1, ?2, ?3, ?4, ?5, 'incomplete', ?6, 1)
                    ON CONFLICT (entry_id) DO UPDATE SET
                        gallery_id = excluded.gallery_id,
                        revision = excluded.revision,
                        relative_directory = excluded.relative_directory,
                        expected_page_count = excluded.expected_page_count,
                        state = 'incomplete',
                        manifest_relative_path = excluded.manifest_relative_path,
                        manifest_schema_version = NULL,
                        writer_version = NULL,
                        completed_at = NULL
                "#,
                params![
                    plan.descriptor.entry_id,
                    plan.gallery.id.get(),
                    to_sql_integer(artifact_revision, "artifact revision")?,
                    plan.relative_directory.as_str(),
                    i64::try_from(plan.source_pages.len()).map_err(|_| {
                        RepositoryError::Other("source page count exceeds SQLite range".into())
                    })?,
                    plan.manifest_relative_path.as_str(),
                ],
            )
            .map_err(map_sqlite_error)?;

        for source_page in &plan.source_pages {
            let relative_path = ArtifactRelativePath::new(format!(
                "{}/{:04}.webp",
                plan.relative_directory.as_str(),
                source_page.source_page_number.get()
            ))
            .map_err(|error| RepositoryError::Other(error.to_string()))?;
            transaction
                .execute(
                    r#"
                        INSERT INTO download_pages (
                            entry_id, gallery_id, source_page_number,
                            relative_path, state, excluded
                        ) VALUES (?1, ?2, ?3, ?4, 'pending', 0)
                        ON CONFLICT (entry_id, source_page_number) DO UPDATE SET
                            gallery_id = excluded.gallery_id,
                            relative_path = excluded.relative_path
                    "#,
                    params![
                        plan.descriptor.entry_id,
                        plan.gallery.id.get(),
                        i64::from(source_page.source_page_number.get()),
                        relative_path.as_str(),
                    ],
                )
                .map_err(map_sqlite_error)?;
        }

        let unexpected_pages = transaction
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_pages
                    WHERE entry_id = ?1
                      AND source_page_number > ?2
                "#,
                params![
                    plan.descriptor.entry_id,
                    i64::try_from(plan.source_pages.len()).unwrap_or(i64::MAX),
                ],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        if unexpected_pages != 0 {
            return Err(RepositoryError::Corrupt(
                "download artifact contains source pages beyond the current gallery metadata"
                    .into(),
            ));
        }

        let total_units = u64::try_from(plan.source_pages.len())
            .map_err(|_| RepositoryError::Other("download page count overflowed".into()))?;
        let verified_units = transaction
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_pages
                    WHERE entry_id = ?1
                      AND state = 'present'
                      AND byte_length IS NOT NULL
                      AND sha256 IS NOT NULL
                      AND storage_format = 'webp'
                      AND source_revision IS NOT NULL
                      AND verified_at IS NOT NULL
                      AND excluded = 0
                "#,
                [&plan.descriptor.entry_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let verified_units = stored_u64(verified_units, "verified page count")?;
        let projection = transition_pipeline_target(
            &transaction,
            target,
            JobState::Downloading,
            Some(verified_units),
            Some(total_units),
            None,
            None,
            "Downloading verified source pages",
        )?;

        let mut statement = transaction
            .prepare(
                r#"
                    SELECT source_page_number, relative_path, byte_length,
                           sha256, storage_format, source_revision, verified_at, excluded
                    FROM download_pages
                    WHERE entry_id = ?1
                      AND state = 'present'
                      AND byte_length IS NOT NULL
                      AND sha256 IS NOT NULL
                      AND storage_format IS NOT NULL
                      AND source_revision IS NOT NULL
                      AND verified_at IS NOT NULL
                    ORDER BY source_page_number ASC
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([&plan.descriptor.entry_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, bool>(7)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut checkpoints = Vec::new();
        for row in rows {
            let row = row.map_err(map_sqlite_error)?;
            checkpoints.push(DownloadCheckpoint {
                page: StoredPage {
                    source_page_number: SourcePageNumber::new(stored_u32(
                        row.0,
                        "checkpoint source page number",
                    )?)
                    .map_err(domain_corruption)?,
                    relative_path: ArtifactRelativePath::new(row.1).map_err(domain_corruption)?,
                    byte_length: stored_u64(row.2, "checkpoint byte length")?,
                    sha256: ArtifactSha256::new(row.3).map_err(domain_corruption)?,
                    storage_format: row
                        .4
                        .parse::<ArtifactStorageFormat>()
                        .map_err(domain_corruption)?,
                    source_revision: row.5,
                    verified_at: row.6,
                },
                excluded: row.7,
            });
        }
        drop(statement);
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(DownloadPrepared {
            projection,
            checkpoints,
        })
    }

    fn pipeline_page_attempt_start(
        &self,
        attempt: &DownloadPageAttempt,
    ) -> Result<(), RepositoryError> {
        let connection = self.connection()?;
        ensure_current_pipeline_attempt(&connection, &attempt.descriptor)?;
        connection
            .execute(
                r#"
                    INSERT INTO download_page_attempts (
                        job_id, job_attempt, source_page_number,
                        candidate_index, started_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                    ON CONFLICT (
                        job_id, job_attempt, source_page_number, candidate_index
                    ) DO NOTHING
                "#,
                params![
                    attempt.descriptor.job_id,
                    to_sql_integer(attempt.descriptor.worker_attempt, "download attempt")?,
                    i64::from(attempt.source_page_number.get()),
                    i64::from(attempt.candidate_index),
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    fn pipeline_page_attempt_finish(
        &self,
        result: &DownloadPageAttemptResult,
    ) -> Result<(), RepositoryError> {
        let connection = self.connection()?;
        ensure_current_pipeline_attempt(&connection, &result.attempt.descriptor)?;
        connection
            .execute(
                r#"
                    INSERT INTO download_page_attempts (
                        job_id, job_attempt, source_page_number,
                        candidate_index, started_at, finished_at,
                        outcome, error_code, error_message, bytes_received
                    ) VALUES (
                        ?1, ?2, ?3, ?4,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        ?5, ?6, ?7, ?8
                    )
                    ON CONFLICT (
                        job_id, job_attempt, source_page_number, candidate_index
                    ) DO UPDATE SET
                        finished_at = excluded.finished_at,
                        outcome = excluded.outcome,
                        error_code = excluded.error_code,
                        error_message = excluded.error_message,
                        bytes_received = excluded.bytes_received
                "#,
                params![
                    result.attempt.descriptor.job_id,
                    to_sql_integer(result.attempt.descriptor.worker_attempt, "download attempt")?,
                    i64::from(result.attempt.source_page_number.get()),
                    i64::from(result.attempt.candidate_index),
                    result.outcome.as_str(),
                    result.error_code,
                    result.error_message,
                    result
                        .bytes_received
                        .map(|bytes| to_sql_integer(bytes, "received page bytes"))
                        .transpose()?,
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    fn pipeline_page_verified(
        &self,
        descriptor: &DownloadJobDescriptor,
        page: &StoredPage,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_pipeline_target(&transaction, descriptor)?;
        if target.state != JobState::Downloading {
            return Err(invalid_pipeline_state(&target, "record a verified page"));
        }
        let changed = transaction
            .execute(
                r#"
                    UPDATE download_pages
                    SET state = 'present',
                        relative_path = ?1,
                        byte_length = ?2,
                        sha256 = ?3,
                        storage_format = ?4,
                        source_revision = ?5,
                        verified_at = ?6
                    WHERE entry_id = ?7 AND source_page_number = ?8
                "#,
                params![
                    page.relative_path.as_str(),
                    to_sql_integer(page.byte_length, "page byte length")?,
                    page.sha256.as_str(),
                    page.storage_format.as_str(),
                    page.source_revision,
                    page.verified_at,
                    descriptor.entry_id,
                    i64::from(page.source_page_number.get()),
                ],
            )
            .map_err(map_sqlite_error)?;
        if changed != 1 {
            return Err(RepositoryError::Corrupt(format!(
                "download page {} has no prepared checkpoint",
                page.source_page_number.get()
            )));
        }
        let completed_units = transaction
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_pages
                    WHERE entry_id = ?1 AND state = 'present' AND excluded = 0
                      AND byte_length IS NOT NULL AND sha256 IS NOT NULL
                      AND storage_format = 'webp' AND source_revision IS NOT NULL
                      AND verified_at IS NOT NULL
                "#,
                [&descriptor.entry_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let projection = update_pipeline_progress(
            &transaction,
            target,
            stored_u64(completed_units, "verified page count")?,
            "Verified a downloaded source page",
        )?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_stage(
        &self,
        descriptor: &DownloadJobDescriptor,
        state: JobState,
        message: &'static str,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        if !matches!(state, JobState::Hashing | JobState::Verifying) {
            return Err(RepositoryError::Other(
                "pipeline stage must be hashing or verifying".into(),
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_pipeline_target(&transaction, descriptor)?;
        if !target.state.allows_transition_to(state) {
            return Err(invalid_pipeline_state(&target, "advance the pipeline"));
        }
        let projection = transition_pipeline_target(
            &transaction,
            target,
            state,
            None,
            None,
            None,
            None,
            message,
        )?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_complete(
        &self,
        descriptor: &DownloadJobDescriptor,
        manifest: &ArtifactManifest,
        manifest_relative_path: &ArtifactRelativePath,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_pipeline_target(&transaction, descriptor)?;
        if target.state != JobState::Verifying {
            return Err(invalid_pipeline_state(&target, "complete the artifact"));
        }
        let expected = transaction
            .query_row(
                "SELECT expected_page_count FROM download_artifacts WHERE entry_id = ?1",
                [&descriptor.entry_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        let verified = transaction
            .query_row(
                r#"
                    SELECT COUNT(*)
                    FROM download_pages
                    WHERE entry_id = ?1 AND state = 'present' AND excluded = 0
                      AND byte_length IS NOT NULL AND sha256 IS NOT NULL
                      AND storage_format = 'webp' AND source_revision IS NOT NULL
                      AND verified_at IS NOT NULL
                "#,
                [&descriptor.entry_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(map_sqlite_error)?;
        if expected != verified
            || stored_u32(expected, "expected artifact page count")? != manifest.expected_page_count
            || manifest.pages.len() != manifest.expected_page_count as usize
        {
            return Err(RepositoryError::Other(
                "artifact cannot complete before every source page is verified".into(),
            ));
        }
        let artifact_changed = transaction
            .execute(
                r#"
                    UPDATE download_artifacts
                    SET revision = revision + 1,
                        state = 'complete',
                        manifest_relative_path = ?1,
                        manifest_schema_version = ?2,
                        writer_version = ?3,
                        hash_profile_version = ?4,
                        completed_at = ?5
                    WHERE entry_id = ?6 AND state = 'incomplete'
                "#,
                params![
                    manifest_relative_path.as_str(),
                    i64::from(manifest.schema_version),
                    manifest.writer_version,
                    i64::from(manifest.hash_profile_version),
                    manifest.completed_at,
                    descriptor.entry_id,
                ],
            )
            .map_err(map_sqlite_error)?;
        if artifact_changed != 1 {
            return Err(RepositoryError::Other(
                "artifact changed concurrently while completing".into(),
            ));
        }
        let projection = transition_pipeline_target(
            &transaction,
            target,
            JobState::Completed,
            Some(stored_u64(verified, "verified page count")?),
            Some(stored_u64(expected, "expected page count")?),
            None,
            None,
            "Download completed and artifact integrity was verified",
        )?;
        transaction
            .execute(
                r#"
                    UPDATE download_attempts
                    SET finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        outcome_state = 'completed',
                        error_code = NULL,
                        error_message = NULL
                    WHERE job_id = ?1 AND attempt = ?2
                "#,
                params![
                    descriptor.job_id,
                    to_sql_integer(descriptor.worker_attempt, "download attempt")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_fail(
        &self,
        descriptor: &DownloadJobDescriptor,
        code: &str,
        message: &str,
        _retryable: bool,
    ) -> Result<Option<DownloadJobProjection>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = match read_pipeline_target(&transaction, descriptor) {
            Ok(target) => target,
            Err(RepositoryError::Other(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        if !target.state.is_active() || !target.state.allows_transition_to(JobState::Failed) {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(None);
        }
        let projection = transition_pipeline_target(
            &transaction,
            target,
            JobState::Failed,
            None,
            None,
            Some(code),
            Some(message),
            "Download stopped before artifact verification completed",
        )?;
        transaction
            .execute(
                r#"
                    UPDATE download_attempts
                    SET finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        outcome_state = 'failed', error_code = ?1, error_message = ?2
                    WHERE job_id = ?3 AND attempt = ?4
                "#,
                params![
                    code,
                    message,
                    descriptor.job_id,
                    to_sql_integer(descriptor.worker_attempt, "download attempt")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(Some(projection))
    }

    fn pipeline_resume_interrupted(&self) -> Result<Vec<DownloadJobDescriptor>, RepositoryError> {
        let entry_ids = {
            let connection = self.connection()?;
            let mut statement = connection
                .prepare(
                    r#"
                        SELECT d.entry_id
                        FROM download_entries d
                        JOIN download_artifacts a ON a.entry_id = d.entry_id
                        WHERE d.state = 'interrupted'
                        ORDER BY d.created_at ASC, d.entry_id ASC
                    "#,
                )
                .map_err(map_sqlite_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_sqlite_error)?;
            let mut entry_ids = Vec::new();
            for row in rows {
                entry_ids.push(
                    DownloadEntryId::new(row.map_err(map_sqlite_error)?)
                        .map_err(domain_corruption)?,
                );
            }
            entry_ids
        };
        if entry_ids.is_empty() {
            return Ok(Vec::new());
        }
        let job_refs = match <Self as DownloadRepository>::download_retry(self, &entry_ids)? {
            DownloadMutationOutcome::Applied(job_refs) => job_refs,
            DownloadMutationOutcome::EntryNotFound(entry_id) => {
                return Err(RepositoryError::Corrupt(format!(
                    "interrupted download entry {entry_id} disappeared during resume"
                )))
            }
            DownloadMutationOutcome::InvalidState { entry_id, state } => {
                return Err(RepositoryError::Other(format!(
                    "interrupted download entry {entry_id} changed to {state} during resume"
                )))
            }
        };
        let connection = self.connection()?;
        let mut descriptors = Vec::new();
        for job_ref in job_refs.into_iter().filter(|job_ref| !job_ref.reused) {
            let descriptor = connection
                .query_row(
                    r#"
                        SELECT job_id, entry_id, gallery_id, attempt
                        FROM download_jobs WHERE job_id = ?1
                    "#,
                    [&job_ref.job_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .map_err(map_sqlite_error)?;
            descriptors.push(DownloadJobDescriptor {
                job_id: descriptor.0,
                entry_id: descriptor.1,
                gallery_id: GalleryId::new(descriptor.2).map_err(domain_corruption)?,
                worker_attempt: stored_u64(descriptor.3, "download attempt")?,
            });
        }
        Ok(descriptors)
    }

    fn pipeline_descriptors_for_jobs(
        &self,
        jobs: &[JobRef],
    ) -> Result<Vec<DownloadJobDescriptor>, RepositoryError> {
        let connection = self.connection()?;
        let mut descriptors = Vec::new();
        for job in jobs.iter().filter(|job| !job.reused) {
            let stored = connection
                .query_row(
                    r#"
                        SELECT job_id, entry_id, gallery_id, attempt
                        FROM download_jobs WHERE job_id = ?1
                    "#,
                    [&job.job_id],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()
                .map_err(map_sqlite_error)?
                .ok_or_else(|| {
                    RepositoryError::Corrupt(format!(
                        "download job {:?} disappeared before launch",
                        job.job_id
                    ))
                })?;
            let attempt = stored_u64(stored.3, "download attempt")?;
            if attempt != job.worker_attempt {
                return Err(RepositoryError::Other(format!(
                    "download job {:?} changed attempt before launch",
                    job.job_id
                )));
            }
            descriptors.push(DownloadJobDescriptor {
                job_id: stored.0,
                entry_id: stored.1,
                gallery_id: GalleryId::new(stored.2).map_err(domain_corruption)?,
                worker_attempt: attempt,
            });
        }
        Ok(descriptors)
    }

    fn pipeline_mark_artifact_issue(
        &self,
        entry_id: &DownloadEntryId,
        code: &str,
        message: &str,
    ) -> Result<Option<DownloadJobProjection>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let Some(target) = read_download_target(&transaction, entry_id)? else {
            return Err(RepositoryError::Corrupt(format!(
                "artifact references missing download entry {entry_id}"
            )));
        };
        transaction
            .execute(
                r#"
                    UPDATE download_artifacts
                    SET revision = revision + 1, state = 'missing_artifacts'
                    WHERE entry_id = ?1 AND state != 'quarantined'
                "#,
                [entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        if target.state != JobState::Completed {
            transaction.commit().map_err(map_sqlite_error)?;
            return Ok(None);
        }
        let descriptor = DownloadJobDescriptor {
            job_id: target.job_id,
            entry_id: target.entry_id.to_string(),
            gallery_id: GalleryId::new(target.gallery_id).map_err(domain_corruption)?,
            worker_attempt: stored_u64(target.attempt, "download attempt")?,
        };
        let pipeline_target = read_pipeline_target(&transaction, &descriptor)?;
        let projection = transition_pipeline_target(
            &transaction,
            pipeline_target,
            JobState::Failed,
            None,
            None,
            Some(code),
            Some(message),
            "Artifact integrity needs attention",
        )?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(Some(projection))
    }

    fn pipeline_artifact_bundle(
        &self,
        entry_id: &DownloadEntryId,
    ) -> Result<Option<ArtifactBundle>, RepositoryError> {
        <Self as ArtifactRepository>::artifact_bundle_get(self, entry_id)
    }

    fn pipeline_artifact_bundles(&self) -> Result<Vec<ArtifactBundle>, RepositoryError> {
        let entry_ids = {
            let connection = self.connection()?;
            let mut statement = connection
                .prepare("SELECT entry_id FROM download_artifacts ORDER BY entry_id ASC")
                .map_err(map_sqlite_error)?;
            let rows = statement
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(map_sqlite_error)?;
            let mut entry_ids = Vec::new();
            for row in rows {
                entry_ids.push(
                    DownloadEntryId::new(row.map_err(map_sqlite_error)?)
                        .map_err(domain_corruption)?,
                );
            }
            entry_ids
        };
        let mut bundles = Vec::with_capacity(entry_ids.len());
        for entry_id in entry_ids {
            if let Some(bundle) =
                <Self as ArtifactRepository>::artifact_bundle_get(self, &entry_id)?
            {
                bundles.push(bundle);
            }
        }
        Ok(bundles)
    }

    fn pipeline_quarantine_begin(&self, saga: &QuarantineSaga) -> Result<(), RepositoryError> {
        if saga.state != QuarantineSagaState::PendingQuarantine {
            return Err(RepositoryError::Other(
                "quarantine saga must begin in pending_quarantine".into(),
            ));
        }
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_download_target(&transaction, &saga.entry_id)?
            .ok_or_else(|| RepositoryError::Other("download entry no longer exists".into()))?;
        if target.state != JobState::Completed {
            return Err(RepositoryError::Other(format!(
                "download entry cannot be quarantined from {}",
                target.state
            )));
        }
        let artifact = transaction
            .query_row(
                "SELECT relative_directory, state FROM download_artifacts WHERE entry_id = ?1",
                [saga.entry_id.as_str()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| RepositoryError::Other("download artifact no longer exists".into()))?;
        if artifact.0 != saga.original_relative_path.as_str() || artifact.1 != "complete" {
            return Err(RepositoryError::Other(
                "download artifact is not a verified complete artifact".into(),
            ));
        }
        transaction
            .execute(
                r#"
                    INSERT INTO quarantine_records (
                        record_id, entry_id, original_relative_path,
                        quarantine_relative_path, reason, state,
                        original_entry_state, original_artifact_state,
                        created_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, 'pending_quarantine',
                        'completed', 'complete',
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                "#,
                params![
                    saga.record_id,
                    saga.entry_id.as_str(),
                    saga.original_relative_path.as_str(),
                    saga.quarantine_relative_path.as_str(),
                    saga.reason,
                ],
            )
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)
    }

    fn pipeline_quarantine_complete(
        &self,
        record_id: &str,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let entry_id = transaction
            .query_row(
                "SELECT entry_id FROM quarantine_records WHERE record_id = ?1 AND state = 'pending_quarantine'",
                [record_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| RepositoryError::Other("quarantine operation is no longer pending".into()))?;
        let entry_id = DownloadEntryId::new(entry_id).map_err(domain_corruption)?;
        let target = read_download_target(&transaction, &entry_id)?
            .ok_or_else(|| RepositoryError::Other("download entry no longer exists".into()))?;
        if target.state != JobState::Completed {
            return Err(RepositoryError::Other(
                "download entry changed while it was being quarantined".into(),
            ));
        }
        let descriptor = DownloadJobDescriptor {
            job_id: target.job_id.clone(),
            entry_id: target.entry_id.to_string(),
            gallery_id: GalleryId::new(target.gallery_id).map_err(domain_corruption)?,
            worker_attempt: stored_u64(target.attempt, "download attempt")?,
        };
        let pipeline_target = read_pipeline_target(&transaction, &descriptor)?;
        let artifact_changed = transaction
            .execute(
                "UPDATE download_artifacts SET revision = revision + 1, state = 'quarantined' WHERE entry_id = ?1 AND state = 'complete'",
                [entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        if artifact_changed != 1 {
            return Err(RepositoryError::Other(
                "artifact changed while it was being quarantined".into(),
            ));
        }
        transaction
            .execute(
                "UPDATE download_pages SET state = 'quarantined' WHERE entry_id = ?1 AND state = 'present'",
                [entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        let projection = transition_pipeline_target(
            &transaction,
            pipeline_target,
            JobState::Quarantined,
            None,
            None,
            None,
            None,
            "Artifact moved to recoverable quarantine",
        )?;
        transaction
            .execute(
                "UPDATE quarantine_records SET state = 'quarantined' WHERE record_id = ?1 AND state = 'pending_quarantine'",
                [record_id],
            )
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_restore_begin(
        &self,
        entry_id: &DownloadEntryId,
    ) -> Result<QuarantineSaga, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let target = read_download_target(&transaction, entry_id)?
            .ok_or_else(|| RepositoryError::Other("download entry no longer exists".into()))?;
        if target.state != JobState::Quarantined {
            return Err(RepositoryError::Other(format!(
                "download entry cannot be restored from {}",
                target.state
            )));
        }
        let stored = transaction
            .query_row(
                r#"
                    SELECT record_id, original_relative_path,
                           quarantine_relative_path, reason
                    FROM quarantine_records
                    WHERE entry_id = ?1 AND state = 'quarantined'
                    ORDER BY created_at DESC, record_id DESC
                    LIMIT 1
                "#,
                [entry_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| {
                RepositoryError::Corrupt("quarantined entry has no active record".into())
            })?;
        let changed = transaction
            .execute(
                "UPDATE quarantine_records SET state = 'pending_restore' WHERE record_id = ?1 AND state = 'quarantined'",
                [&stored.0],
            )
            .map_err(map_sqlite_error)?;
        if changed != 1 {
            return Err(RepositoryError::Other(
                "quarantine record changed before restore".into(),
            ));
        }
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(QuarantineSaga {
            record_id: stored.0,
            entry_id: entry_id.clone(),
            original_relative_path: ArtifactRelativePath::new(stored.1)
                .map_err(domain_corruption)?,
            quarantine_relative_path: ArtifactRelativePath::new(stored.2)
                .map_err(domain_corruption)?,
            reason: stored.3,
            state: QuarantineSagaState::PendingRestore,
        })
    }

    fn pipeline_restore_complete(
        &self,
        record_id: &str,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(map_sqlite_error)?;
        let entry_id = transaction
            .query_row(
                "SELECT entry_id FROM quarantine_records WHERE record_id = ?1 AND state = 'pending_restore'",
                [record_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?
            .ok_or_else(|| RepositoryError::Other("restore operation is no longer pending".into()))?;
        let entry_id = DownloadEntryId::new(entry_id).map_err(domain_corruption)?;
        let target = read_download_target(&transaction, &entry_id)?
            .ok_or_else(|| RepositoryError::Other("download entry no longer exists".into()))?;
        if target.state != JobState::Quarantined {
            return Err(RepositoryError::Other(
                "download entry changed while it was being restored".into(),
            ));
        }
        let descriptor = DownloadJobDescriptor {
            job_id: target.job_id.clone(),
            entry_id: target.entry_id.to_string(),
            gallery_id: GalleryId::new(target.gallery_id).map_err(domain_corruption)?,
            worker_attempt: stored_u64(target.attempt, "download attempt")?,
        };
        let pipeline_target = read_pipeline_target(&transaction, &descriptor)?;
        let artifact_changed = transaction
            .execute(
                "UPDATE download_artifacts SET revision = revision + 1, state = 'complete' WHERE entry_id = ?1 AND state = 'quarantined'",
                [entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        if artifact_changed != 1 {
            return Err(RepositoryError::Other(
                "artifact changed while it was being restored".into(),
            ));
        }
        transaction
            .execute(
                "UPDATE download_pages SET state = 'present' WHERE entry_id = ?1 AND state = 'quarantined'",
                [entry_id.as_str()],
            )
            .map_err(map_sqlite_error)?;
        let projection = transition_pipeline_target(
            &transaction,
            pipeline_target,
            JobState::Completed,
            None,
            None,
            None,
            None,
            "Artifact restored from quarantine",
        )?;
        transaction
            .execute(
                r#"
                    UPDATE quarantine_records
                    SET state = 'restored',
                        restored_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    WHERE record_id = ?1 AND state = 'pending_restore'
                "#,
                [record_id],
            )
            .map_err(map_sqlite_error)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(projection)
    }

    fn pipeline_pending_quarantine_sagas(&self) -> Result<Vec<QuarantineSaga>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                    SELECT record_id, entry_id, original_relative_path,
                           quarantine_relative_path, reason, state
                    FROM quarantine_records
                    WHERE state IN ('pending_quarantine', 'pending_restore')
                    ORDER BY created_at ASC, record_id ASC
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut sagas = Vec::new();
        for row in rows {
            let row = row.map_err(map_sqlite_error)?;
            let state = match row.5.as_str() {
                "pending_quarantine" => QuarantineSagaState::PendingQuarantine,
                "pending_restore" => QuarantineSagaState::PendingRestore,
                _ => {
                    return Err(RepositoryError::Corrupt(
                        "pending quarantine query returned an invalid state".into(),
                    ))
                }
            };
            sagas.push(QuarantineSaga {
                record_id: row.0,
                entry_id: DownloadEntryId::new(row.1).map_err(domain_corruption)?,
                original_relative_path: ArtifactRelativePath::new(row.2)
                    .map_err(domain_corruption)?,
                quarantine_relative_path: ArtifactRelativePath::new(row.3)
                    .map_err(domain_corruption)?,
                reason: row.4,
                state,
            });
        }
        Ok(sagas)
    }
}

struct StoredPipelineTarget {
    job_id: String,
    entry_id: String,
    gallery_id: i64,
    job_revision: i64,
    entry_revision: i64,
    state: JobState,
    completed_units: i64,
    total_units: i64,
    attempt: i64,
    progress: f64,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl StoredPipelineTarget {
    fn into_projection(
        self,
        message: Option<&str>,
    ) -> Result<DownloadJobProjection, RepositoryError> {
        Ok(DownloadJobProjection {
            job: JobEvent {
                job_id: self.job_id,
                gallery_id: Some(self.gallery_id),
                revision: stored_u64(self.job_revision, "job revision")?,
                state: self.state,
                completed_units: Some(stored_u64(self.completed_units, "completed units")?),
                total_units: Some(stored_u64(self.total_units, "total units")?),
                message: message.map(str::to_owned),
            },
            download: DownloadChangedEvent {
                entry_id: self.entry_id,
                gallery_id: self.gallery_id,
                revision: stored_u64(self.entry_revision, "download revision")?,
                state: self.state,
                progress: Some(self.progress),
                attempt: Some(stored_u64(self.attempt, "download attempt")?),
                error_code: self.error_code,
                error_message: self.error_message,
            },
        })
    }
}

fn read_pipeline_target(
    transaction: &Transaction<'_>,
    descriptor: &DownloadJobDescriptor,
) -> Result<StoredPipelineTarget, RepositoryError> {
    let stored = transaction
        .query_row(
            r#"
                SELECT j.job_id, j.entry_id, j.gallery_id, j.revision,
                       d.revision, j.state, j.completed_units, j.total_units,
                       j.attempt, d.progress, j.last_error_code, j.last_error_message
                FROM download_jobs j
                JOIN download_entries d
                  ON d.entry_id = j.entry_id AND d.gallery_id = j.gallery_id
                WHERE j.job_id = ?1
            "#,
            [&descriptor.job_id],
            |row| {
                Ok(StoredPipelineTarget {
                    job_id: row.get(0)?,
                    entry_id: row.get(1)?,
                    gallery_id: row.get(2)?,
                    job_revision: row.get(3)?,
                    entry_revision: row.get(4)?,
                    state: row
                        .get::<_, String>(5)?
                        .parse::<JobState>()
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                5,
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    completed_units: row.get(6)?,
                    total_units: row.get(7)?,
                    attempt: row.get(8)?,
                    progress: row.get(9)?,
                    error_code: row.get(10)?,
                    error_message: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(map_sqlite_error)?
        .ok_or_else(|| RepositoryError::Other("download job no longer exists".into()))?;
    if stored.entry_id != descriptor.entry_id
        || stored.gallery_id != descriptor.gallery_id.get()
        || stored_u64(stored.attempt, "download attempt")? != descriptor.worker_attempt
    {
        return Err(RepositoryError::Other(
            "download worker descriptor is stale".into(),
        ));
    }
    Ok(stored)
}

fn ensure_current_pipeline_attempt(
    connection: &Connection,
    descriptor: &DownloadJobDescriptor,
) -> Result<(), RepositoryError> {
    let current = connection
        .query_row(
            "SELECT entry_id, gallery_id, attempt FROM download_jobs WHERE job_id = ?1",
            [&descriptor.job_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    if current.is_none_or(|current| {
        current.0 != descriptor.entry_id
            || current.1 != descriptor.gallery_id.get()
            || u64::try_from(current.2).ok() != Some(descriptor.worker_attempt)
    }) {
        return Err(RepositoryError::Other(
            "download worker descriptor is stale".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn transition_pipeline_target(
    transaction: &Transaction<'_>,
    target: StoredPipelineTarget,
    next_state: JobState,
    completed_units: Option<u64>,
    total_units: Option<u64>,
    error_code: Option<&str>,
    error_message: Option<&str>,
    message: &'static str,
) -> Result<DownloadJobProjection, RepositoryError> {
    if !target.state.allows_transition_to(next_state) {
        return Err(invalid_pipeline_state(&target, "transition"));
    }
    let job_revision = next_stored_revision(target.job_revision, "job revision")?;
    let entry_revision = next_stored_revision(target.entry_revision, "download revision")?;
    let completed_units = completed_units
        .map(|value| to_sql_integer(value, "completed units"))
        .transpose()?
        .unwrap_or(target.completed_units);
    let total_units = total_units
        .map(|value| to_sql_integer(value, "total units"))
        .transpose()?
        .unwrap_or(target.total_units);
    if total_units <= 0 || completed_units < 0 || completed_units > total_units {
        return Err(RepositoryError::Corrupt(
            "download progress units are inconsistent".into(),
        ));
    }
    let progress = (completed_units as f64 / total_units as f64) * 100.0;
    let terminal = !next_state.is_active();
    let changed_jobs = transaction
        .execute(
            r#"
                UPDATE download_jobs
                SET revision = ?1, state = ?2,
                    completed_units = ?3, total_units = ?4,
                    last_error_code = ?5, last_error_message = ?6,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    started_at = CASE
                        WHEN ?2 != 'queued' THEN COALESCE(
                            started_at,
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        ) ELSE started_at END,
                    finished_at = CASE
                        WHEN ?7 THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        ELSE NULL END
                WHERE job_id = ?8 AND revision = ?9 AND attempt = ?10 AND state = ?11
            "#,
            params![
                to_sql_integer(job_revision, "job revision")?,
                next_state.to_string(),
                completed_units,
                total_units,
                error_code,
                error_message,
                terminal,
                target.job_id,
                target.job_revision,
                target.attempt,
                target.state.to_string(),
            ],
        )
        .map_err(map_sqlite_error)?;
    let changed_entries = transaction
        .execute(
            r#"
                UPDATE download_entries
                SET revision = ?1, state = ?2, progress = ?3,
                    review_kind = NULL, review_id = NULL,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE entry_id = ?4 AND revision = ?5 AND state = ?6
            "#,
            params![
                to_sql_integer(entry_revision, "download revision")?,
                next_state.to_string(),
                progress,
                target.entry_id,
                target.entry_revision,
                target.state.to_string(),
            ],
        )
        .map_err(map_sqlite_error)?;
    if changed_jobs != 1 || changed_entries != 1 {
        return Err(RepositoryError::Other(
            "download pipeline state changed concurrently".into(),
        ));
    }
    StoredPipelineTarget {
        job_revision: to_sql_integer(job_revision, "job revision")?,
        entry_revision: to_sql_integer(entry_revision, "download revision")?,
        state: next_state,
        completed_units,
        total_units,
        progress,
        error_code: error_code.map(str::to_owned),
        error_message: error_message.map(str::to_owned),
        ..target
    }
    .into_projection(Some(message))
}

fn update_pipeline_progress(
    transaction: &Transaction<'_>,
    target: StoredPipelineTarget,
    completed_units: u64,
    message: &'static str,
) -> Result<DownloadJobProjection, RepositoryError> {
    let total_units = stored_u64(target.total_units, "total units")?;
    if completed_units > total_units {
        return Err(RepositoryError::Corrupt(
            "verified page count exceeds the expected page count".into(),
        ));
    }
    let job_revision = next_stored_revision(target.job_revision, "job revision")?;
    let entry_revision = next_stored_revision(target.entry_revision, "download revision")?;
    let progress = if total_units == 0 {
        0.0
    } else {
        (completed_units as f64 / total_units as f64) * 100.0
    };
    let changed_jobs = transaction
        .execute(
            r#"
                UPDATE download_jobs
                SET revision = ?1, completed_units = ?2,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE job_id = ?3 AND revision = ?4 AND attempt = ?5 AND state = ?6
            "#,
            params![
                to_sql_integer(job_revision, "job revision")?,
                to_sql_integer(completed_units, "completed units")?,
                target.job_id,
                target.job_revision,
                target.attempt,
                target.state.to_string(),
            ],
        )
        .map_err(map_sqlite_error)?;
    let changed_entries = transaction
        .execute(
            r#"
                UPDATE download_entries
                SET revision = ?1, progress = ?2,
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE entry_id = ?3 AND revision = ?4 AND state = ?5
            "#,
            params![
                to_sql_integer(entry_revision, "download revision")?,
                progress,
                target.entry_id,
                target.entry_revision,
                target.state.to_string(),
            ],
        )
        .map_err(map_sqlite_error)?;
    if changed_jobs != 1 || changed_entries != 1 {
        return Err(RepositoryError::Other(
            "download pipeline progress changed concurrently".into(),
        ));
    }
    StoredPipelineTarget {
        job_revision: to_sql_integer(job_revision, "job revision")?,
        entry_revision: to_sql_integer(entry_revision, "download revision")?,
        completed_units: to_sql_integer(completed_units, "completed units")?,
        progress,
        ..target
    }
    .into_projection(Some(message))
}

fn invalid_pipeline_state(target: &StoredPipelineTarget, operation: &str) -> RepositoryError {
    RepositoryError::Other(format!(
        "download job {:?} cannot {operation} from {}",
        target.job_id, target.state
    ))
}

struct StoredDownloadTarget {
    job_id: String,
    entry_id: DownloadEntryId,
    gallery_id: i64,
    job_revision: i64,
    entry_revision: i64,
    state: JobState,
    progress: f64,
    review_kind: Option<String>,
    review_id: Option<String>,
    attempt: i64,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl StoredDownloadTarget {
    fn into_download_entry(self) -> Result<DownloadEntry, RepositoryError> {
        let review_kind = parse_download_review_kind(self.review_kind)?;
        if self.state == JobState::ReviewRequired
            && (review_kind.is_none() || self.review_id.as_deref().is_none_or(str::is_empty))
        {
            return Err(RepositoryError::Corrupt(
                "review_required download entry is missing its review target".into(),
            ));
        }
        Ok(DownloadEntry {
            entry_id: self.entry_id,
            gallery_id: GalleryId::new(self.gallery_id).map_err(domain_corruption)?,
            revision: stored_u64(self.entry_revision, "download revision")?,
            state: self.state,
            progress: Some(self.progress),
            attempt: Some(stored_u64(self.attempt, "download attempt")?),
            error_code: self.error_code,
            error_message: self.error_message,
            review_kind,
            review_id: self.review_id,
        })
    }
}

fn read_download_target(
    transaction: &Transaction<'_>,
    entry_id: &DownloadEntryId,
) -> Result<Option<StoredDownloadTarget>, RepositoryError> {
    let stored = transaction
        .query_row(
            r#"
                SELECT
                    j.job_id, j.entry_id, j.gallery_id,
                    j.revision, d.revision, j.state, d.state,
                    d.progress, d.review_kind, d.review_id, j.attempt,
                    j.last_error_code, j.last_error_message
                FROM download_jobs j
                JOIN download_entries d
                  ON d.entry_id = j.entry_id AND d.gallery_id = j.gallery_id
                WHERE d.entry_id = ?1
            "#,
            [entry_id.as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, f64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Option<String>>(11)?,
                    row.get::<_, Option<String>>(12)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((
        job_id,
        stored_entry_id,
        gallery_id,
        job_revision,
        entry_revision,
        stored_job_state,
        stored_entry_state,
        progress,
        review_kind,
        review_id,
        attempt,
        error_code,
        error_message,
    )) = stored
    else {
        return Ok(None);
    };
    let job_state = stored_job_state
        .parse::<JobState>()
        .map_err(domain_corruption)?;
    let entry_state = stored_entry_state
        .parse::<JobState>()
        .map_err(domain_corruption)?;
    if job_state != entry_state {
        return Err(RepositoryError::Corrupt(format!(
            "download job {job_id:?} and entry {stored_entry_id:?} disagree on state"
        )));
    }
    Ok(Some(StoredDownloadTarget {
        job_id,
        entry_id: DownloadEntryId::new(stored_entry_id).map_err(domain_corruption)?,
        gallery_id,
        job_revision,
        entry_revision,
        state: entry_state,
        progress,
        review_kind,
        review_id,
        attempt,
        error_code,
        error_message,
    }))
}

fn active_job_for_gallery(
    transaction: &Transaction<'_>,
    gallery_id: i64,
    excluded_entry_id: &str,
) -> Result<Option<String>, RepositoryError> {
    transaction
        .query_row(
            r#"
                SELECT j.job_id
                FROM download_entries d
                JOIN download_jobs j
                  ON j.entry_id = d.entry_id AND j.gallery_id = d.gallery_id
                WHERE d.gallery_id = ?1
                  AND d.entry_id != ?2
                  AND d.state = j.state
                  AND d.state IN (
                      'queued', 'resolving_metadata', 'downloading',
                      'hashing', 'verifying', 'retry_wait'
                  )
                LIMIT 1
            "#,
            params![gallery_id, excluded_entry_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_sqlite_error)
}

fn recover_volatile_downloads(connection: &mut Connection) -> Result<usize, RepositoryError> {
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            r#"
                UPDATE download_attempts
                SET finished_at = COALESCE(
                        finished_at,
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    ),
                    outcome_state = 'interrupted',
                    error_code = 'JOB_INTERRUPTED',
                    error_message =
                        'The application stopped before the job reached a terminal state'
                WHERE EXISTS (
                    SELECT 1
                    FROM download_jobs j
                    WHERE j.job_id = download_attempts.job_id
                      AND j.attempt = download_attempts.attempt
                      AND j.state IN (
                          'queued', 'resolving_metadata', 'downloading',
                          'hashing', 'verifying', 'retry_wait'
                      )
                )
            "#,
            [],
        )
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            r#"
                UPDATE download_jobs
                SET revision = revision + 1,
                    state = 'interrupted',
                    last_error_code = 'JOB_INTERRUPTED',
                    last_error_message =
                        'The application stopped before the job reached a terminal state',
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                    finished_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                )
            "#,
            [],
        )
        .map_err(map_sqlite_error)?;
    let recovered_entries = transaction
        .execute(
            r#"
                UPDATE download_entries
                SET revision = revision + 1,
                    state = 'interrupted',
                    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                WHERE state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                )
            "#,
            [],
        )
        .map_err(map_sqlite_error)?;
    transaction.commit().map_err(map_sqlite_error)?;
    Ok(recovered_entries)
}

fn read_request_entries(
    connection: &Connection,
    request_id: &str,
) -> Result<Vec<DownloadEntry>, RepositoryError> {
    let mut statement = connection
        .prepare(
            r#"
                SELECT
                    request_entry.entry_id,
                    request_entry.gallery_id,
                    request_entry.response_revision,
                    request_entry.response_state,
                    request_entry.response_progress,
                    request_entry.response_review_kind,
                    request_entry.response_review_id,
                    NULL AS attempt,
                    NULL AS error_code,
                    NULL AS error_message
                FROM download_queue_request_entries request_entry
                WHERE request_entry.request_id = ?1
                ORDER BY request_entry.position ASC
            "#,
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([request_id], stored_download_entry)
        .map_err(map_sqlite_error)?;
    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(map_sqlite_error)?.try_into_domain()?);
    }
    Ok(entries)
}

struct StoredDownloadEntry {
    entry_id: String,
    gallery_id: i64,
    revision: i64,
    state: String,
    progress: f64,
    review_kind: Option<String>,
    review_id: Option<String>,
    attempt: Option<i64>,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl StoredDownloadEntry {
    fn try_into_domain(self) -> Result<DownloadEntry, RepositoryError> {
        let state = self.state.parse::<JobState>().map_err(domain_corruption)?;
        let review_kind = parse_download_review_kind(self.review_kind)?;
        if state == JobState::ReviewRequired
            && (review_kind.is_none() || self.review_id.as_deref().is_none_or(str::is_empty))
        {
            return Err(RepositoryError::Corrupt(
                "review_required download entry is missing its review target".into(),
            ));
        }

        Ok(DownloadEntry {
            entry_id: DownloadEntryId::new(self.entry_id).map_err(domain_corruption)?,
            gallery_id: GalleryId::new(self.gallery_id).map_err(domain_corruption)?,
            revision: stored_u64(self.revision, "download revision")?,
            state,
            progress: Some(self.progress),
            attempt: self
                .attempt
                .map(|attempt| stored_u64(attempt, "download attempt"))
                .transpose()?,
            error_code: self.error_code,
            error_message: self.error_message,
            review_kind,
            review_id: self.review_id,
        })
    }
}

fn parse_download_review_kind(
    review_kind: Option<String>,
) -> Result<Option<DownloadReviewKind>, RepositoryError> {
    review_kind
        .map(|kind| match kind.as_str() {
            "gallery_duplicate" => Ok(DownloadReviewKind::GalleryDuplicate),
            "internal_pages" => Ok(DownloadReviewKind::InternalPages),
            _ => Err(RepositoryError::Corrupt(format!(
                "download review kind {kind:?} is unsupported"
            ))),
        })
        .transpose()
}

fn stored_download_entry(row: &Row<'_>) -> rusqlite::Result<StoredDownloadEntry> {
    Ok(StoredDownloadEntry {
        entry_id: row.get(0)?,
        gallery_id: row.get(1)?,
        revision: row.get(2)?,
        state: row.get(3)?,
        progress: row.get(4)?,
        review_kind: row.get(5)?,
        review_id: row.get(6)?,
        attempt: row.get(7)?,
        error_code: row.get(8)?,
        error_message: row.get(9)?,
    })
}

struct StoredArtifactBundle {
    gallery_id: i64,
    gallery_revision: i64,
    title: String,
    primary_artist: Option<String>,
    primary_group: Option<String>,
    source_page_count: i64,
    artifact_revision: i64,
    relative_directory: String,
    expected_page_count: i64,
    artifact_state: String,
    manifest_relative_path: Option<String>,
    manifest_schema_version: Option<i64>,
    writer_version: Option<String>,
    hash_profile_version: i64,
    completed_at: Option<String>,
}

fn stored_artifact_bundle(row: &Row<'_>) -> rusqlite::Result<StoredArtifactBundle> {
    Ok(StoredArtifactBundle {
        gallery_id: row.get(0)?,
        gallery_revision: row.get(1)?,
        title: row.get(2)?,
        primary_artist: row.get(3)?,
        primary_group: row.get(4)?,
        source_page_count: row.get(5)?,
        artifact_revision: row.get(6)?,
        relative_directory: row.get(7)?,
        expected_page_count: row.get(8)?,
        artifact_state: row.get(9)?,
        manifest_relative_path: row.get(10)?,
        manifest_schema_version: row.get(11)?,
        writer_version: row.get(12)?,
        hash_profile_version: row.get(13)?,
        completed_at: row.get(14)?,
    })
}

struct StoredPageArtifact {
    gallery_id: i64,
    source_page_number: i64,
    relative_path: String,
    page_state: String,
    byte_length: Option<i64>,
    sha256: Option<String>,
    storage_format: Option<String>,
    source_revision: Option<String>,
    verified_at: Option<String>,
    excluded: bool,
}

fn stored_page_artifact(row: &Row<'_>) -> rusqlite::Result<StoredPageArtifact> {
    Ok(StoredPageArtifact {
        gallery_id: row.get(0)?,
        source_page_number: row.get(1)?,
        relative_path: row.get(2)?,
        page_state: row.get(3)?,
        byte_length: row.get(4)?,
        sha256: row.get(5)?,
        storage_format: row.get(6)?,
        source_revision: row.get(7)?,
        verified_at: row.get(8)?,
        excluded: row.get(9)?,
    })
}

fn read_favorites(connection: &Connection) -> Result<Vec<FavoriteRecord>, RepositoryError> {
    let mut statement = connection
        .prepare(
            r#"
                SELECT namespace, value, revision, created_at, updated_at
                FROM favorites
                ORDER BY namespace ASC, value COLLATE NOCASE ASC
            "#,
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([], stored_favorite)
        .map_err(map_sqlite_error)?;
    rows.map(|row| row.map_err(map_sqlite_error)?.try_into_domain())
        .collect()
}

fn read_favorite(
    connection: &Connection,
    key: &FavoriteKey,
) -> Result<Option<FavoriteRecord>, RepositoryError> {
    connection
        .query_row(
            r#"
                SELECT namespace, value, revision, created_at, updated_at
                FROM favorites
                WHERE namespace = ?1 AND value = ?2
            "#,
            params![key.namespace.as_str(), key.value],
            stored_favorite,
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(StoredFavorite::try_into_domain)
        .transpose()
}

struct StoredFavorite {
    namespace: String,
    value: String,
    revision: i64,
    created_at: String,
    updated_at: String,
}

impl StoredFavorite {
    fn try_into_domain(self) -> Result<FavoriteRecord, RepositoryError> {
        Ok(FavoriteRecord {
            namespace: FavoriteNamespace::from_database(&self.namespace).ok_or_else(|| {
                RepositoryError::Corrupt(format!(
                    "favorite namespace {:?} is unsupported",
                    self.namespace
                ))
            })?,
            value: self.value,
            revision: stored_u64(self.revision, "favorite revision")?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn stored_favorite(row: &Row<'_>) -> rusqlite::Result<StoredFavorite> {
    Ok(StoredFavorite {
        namespace: row.get(0)?,
        value: row.get(1)?,
        revision: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

fn read_search_history_by_fingerprint(
    connection: &Connection,
    fingerprint: &str,
) -> Result<Option<SearchHistoryEntry>, RepositoryError> {
    connection
        .query_row(
            r#"
                SELECT history_id, text, include_tags_json, exclude_tags_json,
                       languages_json, sort, page_size, use_count, last_used_at
                FROM search_history
                WHERE fingerprint = ?1
            "#,
            [fingerprint],
            stored_search_history,
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(StoredSearchHistory::try_into_domain)
        .transpose()
}

struct StoredSearchHistory {
    history_id: i64,
    text: String,
    include_tags_json: String,
    exclude_tags_json: String,
    languages_json: String,
    sort: String,
    page_size: i64,
    use_count: i64,
    last_used_at: String,
}

impl StoredSearchHistory {
    fn try_into_domain(self) -> Result<SearchHistoryEntry, RepositoryError> {
        Ok(SearchHistoryEntry {
            history_id: self.history_id,
            text: self.text,
            include_tags: serde_json::from_str(&self.include_tags_json)
                .map_err(domain_corruption)?,
            exclude_tags: serde_json::from_str(&self.exclude_tags_json)
                .map_err(domain_corruption)?,
            languages: serde_json::from_str(&self.languages_json).map_err(domain_corruption)?,
            sort: parse_search_sort(&self.sort)?,
            page_size: stored_u32(self.page_size, "search history page size")?,
            use_count: stored_u64(self.use_count, "search history use count")?,
            last_used_at: self.last_used_at,
        })
    }
}

fn stored_search_history(row: &Row<'_>) -> rusqlite::Result<StoredSearchHistory> {
    Ok(StoredSearchHistory {
        history_id: row.get(0)?,
        text: row.get(1)?,
        include_tags_json: row.get(2)?,
        exclude_tags_json: row.get(3)?,
        languages_json: row.get(4)?,
        sort: row.get(5)?,
        page_size: row.get(6)?,
        use_count: row.get(7)?,
        last_used_at: row.get(8)?,
    })
}

fn search_sort_text(sort: SearchSort) -> &'static str {
    match sort {
        SearchSort::Recent => "recent",
        SearchSort::PopularToday => "popular_today",
        SearchSort::PopularWeek => "popular_week",
        SearchSort::PopularMonth => "popular_month",
        SearchSort::PopularYear => "popular_year",
        SearchSort::Random => "random",
    }
}

fn parse_search_sort(value: &str) -> Result<SearchSort, RepositoryError> {
    match value {
        "recent" => Ok(SearchSort::Recent),
        "popular_today" => Ok(SearchSort::PopularToday),
        "popular_week" => Ok(SearchSort::PopularWeek),
        "popular_month" => Ok(SearchSort::PopularMonth),
        "popular_year" => Ok(SearchSort::PopularYear),
        "random" => Ok(SearchSort::Random),
        _ => Err(RepositoryError::Corrupt(format!(
            "search sort {value:?} is unsupported"
        ))),
    }
}

fn language_text(language: Language) -> &'static str {
    match language {
        Language::Korean => "korean",
        Language::Japanese => "japanese",
        Language::Chinese => "chinese",
        Language::English => "english",
    }
}

fn parse_language(value: &str) -> Result<Language, RepositoryError> {
    match value {
        "korean" => Ok(Language::Korean),
        "japanese" => Ok(Language::Japanese),
        "chinese" => Ok(Language::Chinese),
        "english" => Ok(Language::English),
        _ => Err(RepositoryError::Corrupt(format!(
            "gallery language {value:?} is unsupported"
        ))),
    }
}

fn auto_find_run_is_running(
    connection: &Connection,
    run_id: &str,
) -> Result<bool, RepositoryError> {
    connection
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM auto_find_runs WHERE run_id = ?1 AND state = 'running')",
            [run_id],
            |row| row.get(0),
        )
        .map_err(map_sqlite_error)
}

fn read_running_auto_find(connection: &Connection) -> Result<Option<AutoFindRun>, RepositoryError> {
    connection
        .query_row(
            r#"
                SELECT run_id, revision, state, total_favorites,
                       completed_favorites, candidates_found,
                       started_at, updated_at, finished_at,
                       error_code, error_message
                FROM auto_find_runs
                WHERE state = 'running'
                LIMIT 1
            "#,
            [],
            stored_auto_find_run,
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(StoredAutoFindRun::try_into_domain)
        .transpose()
}

fn read_auto_find_run(
    connection: &Connection,
    run_id: &str,
) -> Result<Option<AutoFindRun>, RepositoryError> {
    connection
        .query_row(
            r#"
                SELECT run_id, revision, state, total_favorites,
                       completed_favorites, candidates_found,
                       started_at, updated_at, finished_at,
                       error_code, error_message
                FROM auto_find_runs
                WHERE run_id = ?1
            "#,
            [run_id],
            stored_auto_find_run,
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(StoredAutoFindRun::try_into_domain)
        .transpose()
}

struct StoredAutoFindRun {
    run_id: String,
    revision: i64,
    state: String,
    total_favorites: i64,
    completed_favorites: i64,
    candidates_found: i64,
    started_at: String,
    updated_at: String,
    finished_at: Option<String>,
    error_code: Option<String>,
    error_message: Option<String>,
}

impl StoredAutoFindRun {
    fn try_into_domain(self) -> Result<AutoFindRun, RepositoryError> {
        Ok(AutoFindRun {
            run_id: self.run_id,
            revision: stored_u64(self.revision, "Auto Find revision")?,
            state: AutoFindRunState::from_database(&self.state).ok_or_else(|| {
                RepositoryError::Corrupt(format!("Auto Find state {:?} is unsupported", self.state))
            })?,
            total_favorites: stored_u32(self.total_favorites, "Auto Find favorite count")?,
            completed_favorites: stored_u32(
                self.completed_favorites,
                "Auto Find completed favorite count",
            )?,
            candidates_found: stored_u32(self.candidates_found, "Auto Find candidate count")?,
            started_at: self.started_at,
            updated_at: self.updated_at,
            finished_at: self.finished_at,
            error_code: self.error_code,
            error_message: self.error_message,
        })
    }
}

fn stored_auto_find_run(row: &Row<'_>) -> rusqlite::Result<StoredAutoFindRun> {
    Ok(StoredAutoFindRun {
        run_id: row.get(0)?,
        revision: row.get(1)?,
        state: row.get(2)?,
        total_favorites: row.get(3)?,
        completed_favorites: row.get(4)?,
        candidates_found: row.get(5)?,
        started_at: row.get(6)?,
        updated_at: row.get(7)?,
        finished_at: row.get(8)?,
        error_code: row.get(9)?,
        error_message: row.get(10)?,
    })
}

fn read_auto_find_snapshot(connection: &Connection) -> Result<AutoFindSnapshot, RepositoryError> {
    let run = connection
        .query_row(
            r#"
                SELECT run_id, revision, state, total_favorites,
                       completed_favorites, candidates_found,
                       started_at, updated_at, finished_at,
                       error_code, error_message
                FROM auto_find_runs
                ORDER BY started_at DESC, run_id DESC
                LIMIT 1
            "#,
            [],
            stored_auto_find_run,
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(StoredAutoFindRun::try_into_domain)
        .transpose()?;
    let Some(run) = run else {
        return Ok(AutoFindSnapshot {
            run: None,
            candidates: Vec::new(),
        });
    };
    let mut statement = connection
        .prepare(
            r#"
                SELECT run_id, gallery_id, title, artist, group_name, pages,
                       language, tags_json, series_json, characters_json,
                       published_rank, popularity,
                       thumbnail_key, thumbnail_width, thumbnail_height,
                       favorite_namespace, favorite_value, discovered_at
                FROM auto_find_candidates candidate
                WHERE run_id = ?1
                  AND NOT EXISTS (
                      SELECT 1 FROM auto_find_exclusions exclusion
                      WHERE exclusion.gallery_id = candidate.gallery_id
                  )
                  AND NOT EXISTS (
                      SELECT 1 FROM download_entries download
                      WHERE download.gallery_id = candidate.gallery_id
                  )
                ORDER BY published_rank DESC, gallery_id DESC
            "#,
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([run.run_id.as_str()], stored_auto_find_candidate)
        .map_err(map_sqlite_error)?;
    let candidates = rows
        .map(|row| row.map_err(map_sqlite_error)?.try_into_domain())
        .collect::<Result<Vec<_>, _>>()?;
    Ok(AutoFindSnapshot {
        run: Some(run),
        candidates,
    })
}

struct StoredAutoFindCandidate {
    run_id: String,
    gallery_id: i64,
    title: String,
    artist: String,
    group: Option<String>,
    pages: i64,
    language: String,
    tags_json: String,
    series_json: String,
    characters_json: String,
    published_rank: i64,
    popularity: i64,
    thumbnail_key: Option<String>,
    thumbnail_width: i64,
    thumbnail_height: i64,
    favorite_namespace: String,
    favorite_value: String,
    discovered_at: String,
}

impl StoredAutoFindCandidate {
    fn try_into_domain(self) -> Result<AutoFindCandidate, RepositoryError> {
        Ok(AutoFindCandidate {
            run_id: self.run_id,
            gallery: GallerySummary {
                id: GalleryId::new(self.gallery_id).map_err(domain_corruption)?,
                title: self.title,
                artist: self.artist,
                group: self.group,
                pages: stored_u32(self.pages, "Auto Find page count")?,
                language: parse_language(&self.language)?,
                tags: serde_json::from_str(&self.tags_json).map_err(domain_corruption)?,
                series: serde_json::from_str(&self.series_json).map_err(domain_corruption)?,
                characters: serde_json::from_str(&self.characters_json)
                    .map_err(domain_corruption)?,
                published_rank: stored_u32(self.published_rank, "Auto Find published rank")?,
                popularity: stored_u32(self.popularity, "Auto Find popularity")?,
                thumbnail_key: self.thumbnail_key,
                thumbnail_width: stored_u32(self.thumbnail_width, "Auto Find thumbnail width")?,
                thumbnail_height: stored_u32(self.thumbnail_height, "Auto Find thumbnail height")?,
            },
            matched_favorite: FavoriteKey {
                namespace: FavoriteNamespace::from_database(&self.favorite_namespace).ok_or_else(
                    || {
                        RepositoryError::Corrupt(format!(
                            "Auto Find favorite namespace {:?} is unsupported",
                            self.favorite_namespace
                        ))
                    },
                )?,
                value: self.favorite_value,
            },
            discovered_at: self.discovered_at,
        })
    }
}

fn stored_auto_find_candidate(row: &Row<'_>) -> rusqlite::Result<StoredAutoFindCandidate> {
    Ok(StoredAutoFindCandidate {
        run_id: row.get(0)?,
        gallery_id: row.get(1)?,
        title: row.get(2)?,
        artist: row.get(3)?,
        group: row.get(4)?,
        pages: row.get(5)?,
        language: row.get(6)?,
        tags_json: row.get(7)?,
        series_json: row.get(8)?,
        characters_json: row.get(9)?,
        published_rank: row.get(10)?,
        popularity: row.get(11)?,
        thumbnail_key: row.get(12)?,
        thumbnail_width: row.get(13)?,
        thumbnail_height: row.get(14)?,
        favorite_namespace: row.get(15)?,
        favorite_value: row.get(16)?,
        discovered_at: row.get(17)?,
    })
}

fn domain_corruption(error: impl std::fmt::Display) -> RepositoryError {
    RepositoryError::Corrupt(error.to_string())
}

fn read_settings(connection: &Connection) -> Result<SettingsSnapshot, RepositoryError> {
    let values = connection
        .query_row(
            r#"
                SELECT revision, download_root, max_columns, preview_width,
                       cache_limit_gb, concurrent_image_requests,
                       request_start_interval_ms
                FROM settings
                WHERE singleton = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?;

    Ok(SettingsSnapshot {
        revision: stored_u64(values.0, "settings revision")?,
        download_root: values.1,
        max_columns: stored_u32(values.2, "max columns")?,
        preview_width: stored_u32(values.3, "preview width")?,
        cache_limit_gb: stored_u32(values.4, "cache limit")?,
        concurrent_image_requests: stored_u32(values.5, "concurrent image requests")?,
        request_start_interval_ms: stored_u64(values.6, "request start interval")?,
    })
}

fn read_window_placement(
    connection: &Connection,
) -> Result<WindowPlacementSnapshot, RepositoryError> {
    let values = connection
        .query_row(
            r#"
                SELECT revision, x, y, width, height, maximized
                FROM window_placement
                WHERE singleton = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i32>>(1)?,
                    row.get::<_, Option<i32>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            },
        )
        .map_err(map_sqlite_error)?;

    Ok(WindowPlacementSnapshot {
        revision: stored_u64(values.0, "window placement revision")?,
        x: values.1,
        y: values.2,
        width: stored_u32(values.3, "window width")?,
        height: stored_u32(values.4, "window height")?,
        maximized: values.5,
    })
}

fn next_stored_revision(value: i64, label: &str) -> Result<u64, RepositoryError> {
    stored_u64(value, label)?
        .checked_add(1)
        .ok_or_else(|| RepositoryError::Corrupt(format!("{label} cannot be incremented")))
}

fn stored_u64(value: i64, label: &str) -> Result<u64, RepositoryError> {
    value
        .try_into()
        .map_err(|_| RepositoryError::Corrupt(format!("{label} is negative")))
}

fn stored_u32(value: i64, label: &str) -> Result<u32, RepositoryError> {
    value
        .try_into()
        .map_err(|_| RepositoryError::Corrupt(format!("{label} is outside the supported range")))
}

fn to_sql_integer(value: u64, label: &str) -> Result<i64, RepositoryError> {
    value.try_into().map_err(|_| {
        RepositoryError::Other(format!("{label} exceeds SQLite's signed integer range"))
    })
}

fn map_sqlite_error(error: rusqlite::Error) -> RepositoryError {
    match &error {
        rusqlite::Error::SqliteFailure(sqlite, _)
            if matches!(
                sqlite.code,
                ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked
            ) =>
        {
            RepositoryError::Busy(error.to_string())
        }
        rusqlite::Error::SqliteFailure(sqlite, _)
            if matches!(
                sqlite.code,
                ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase
            ) =>
        {
            RepositoryError::Corrupt(error.to_string())
        }
        _ => RepositoryError::Other(error.to_string()),
    }
}

fn map_migration_error(error: MigrationError) -> RepositoryError {
    match error {
        MigrationError::Sqlite(error) => map_sqlite_error(error),
        MigrationError::FutureVersion {
            found,
            latest_supported,
        } => RepositoryError::UnsupportedSchema {
            found,
            latest_supported,
        },
        MigrationError::NonContiguousHistory { .. } | MigrationError::NameMismatch { .. } => {
            RepositoryError::Corrupt(error.to_string())
        }
    }
}

#[cfg(test)]
mod migration_backup_tests {
    use super::*;
    use crate::infrastructure::migrations::MIGRATIONS;

    fn create_database_before_latest_migration(path: &Path) -> i64 {
        let connection = Connection::open(path).expect("create pre-migration database");
        connection
            .execute_batch(
                r#"
                    PRAGMA foreign_keys = ON;
                    CREATE TABLE schema_migrations (
                        version INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        applied_at TEXT NOT NULL DEFAULT (
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                    ) STRICT;
                "#,
            )
            .expect("create migration history");
        let migrations_before_latest = &MIGRATIONS[..MIGRATIONS.len() - 1];
        for migration in migrations_before_latest {
            connection
                .execute_batch(migration.sql)
                .expect("apply pre-latest migration");
            connection
                .execute(
                    "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                    params![migration.version, migration.name],
                )
                .expect("record pre-latest migration");
        }
        migrations_before_latest
            .last()
            .expect("at least one pre-latest migration")
            .version
    }

    fn backup_files(directory: &Path) -> Vec<PathBuf> {
        let mut backups = std::fs::read_dir(directory)
            .expect("read database directory")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.contains(".pre-migration-") && name.ends_with(".bak"))
            })
            .collect::<Vec<_>>();
        backups.sort();
        backups
    }

    #[test]
    fn file_database_is_backed_up_once_before_pending_migrations() {
        let temporary = tempfile::tempdir().expect("create temporary directory");
        let database_path = temporary.path().join("atsumi-next.sqlite3");
        let previous_version = create_database_before_latest_migration(&database_path);
        let target_version = MIGRATIONS.last().expect("latest migration").version;

        drop(SqliteRepository::open(&database_path).expect("migrate persistent repository"));

        let backups = backup_files(temporary.path());
        assert_eq!(backups.len(), 1);
        let backup_name = backups[0]
            .file_name()
            .and_then(|name| name.to_str())
            .expect("backup file name is Unicode");
        assert!(backup_name.starts_with(&format!(
            "atsumi-next.sqlite3.pre-migration-v{previous_version}-to-v{target_version}-"
        )));
        assert!(backup_name.ends_with(".bak"));
        let backup = Connection::open(&backups[0]).expect("open recoverable backup");
        let backup_version: i64 = backup
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("read backup schema version");
        assert_eq!(backup_version, previous_version);
        let integrity: String = backup
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("check backup integrity");
        assert_eq!(integrity, "ok");
        drop(backup);

        let migrated = Connection::open(&database_path).expect("open migrated database");
        let migrated_version: i64 = migrated
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("read migrated schema version");
        assert_eq!(migrated_version, target_version);
        drop(migrated);

        drop(SqliteRepository::open(&database_path).expect("reopen current repository"));
        assert_eq!(backup_files(temporary.path()).len(), 1);
    }

    #[test]
    fn backup_names_never_overwrite_an_existing_snapshot() {
        let temporary = tempfile::tempdir().expect("create temporary directory");
        let database_path = temporary.path().join("atsumi-next.sqlite3");
        let created_at = 1_786_780_000;
        let first = next_pre_migration_backup_path(&database_path, 6, 7, created_at)
            .expect("reserve first backup path");
        std::fs::write(&first, b"keep this recovery snapshot")
            .expect("create an existing recovery snapshot");

        let second = next_pre_migration_backup_path(&database_path, 6, 7, created_at)
            .expect("reserve non-overwriting backup path");

        assert_ne!(second, first);
        assert_eq!(
            second.file_name().and_then(|name| name.to_str()),
            Some("atsumi-next.sqlite3.pre-migration-v6-to-v7-1786780000-1.bak")
        );
        assert_eq!(
            std::fs::read(&first).expect("existing snapshot remains readable"),
            b"keep this recovery snapshot"
        );
    }

    #[test]
    fn backup_failure_prevents_pending_migrations_from_running() {
        let temporary = tempfile::tempdir().expect("create temporary directory");
        let missing_database_path = temporary
            .path()
            .join("missing-directory")
            .join("atsumi-next.sqlite3");
        let mut connection = Connection::open_in_memory().expect("open migration test database");
        connection
            .execute_batch(
                r#"
                    CREATE TABLE schema_migrations (
                        version INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        applied_at TEXT NOT NULL DEFAULT (
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                    ) STRICT;
                    INSERT INTO schema_migrations (version, name)
                    VALUES (1, 'settings_and_window_placement');
                "#,
            )
            .expect("seed pending migration history");

        let error = run_migrations_with_backup(&mut connection, Some(&missing_database_path))
            .expect_err("backup failure must abort migration");

        assert!(matches!(
            error,
            RepositoryError::MigrationBackup(message)
                if message.contains("could not create pre-migration backup")
        ));
        let recorded_version: i64 = connection
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |row| {
                row.get(0)
            })
            .expect("read unchanged migration history");
        assert_eq!(recorded_version, 1);
        let migration_two_table_exists: bool = connection
            .query_row(
                r#"
                    SELECT EXISTS (
                        SELECT 1 FROM sqlite_schema
                        WHERE type = 'table' AND name = 'download_entries'
                    )
                "#,
                [],
                |row| row.get(0),
            )
            .expect("check that migration two did not run");
        assert!(!migration_two_table_exists);
    }

    #[test]
    fn persistent_repository_uses_wal_without_exclusive_locking() {
        let temporary = tempfile::tempdir().expect("create temporary directory");
        let database_path = temporary.path().join("atsumi-next.sqlite3");
        let first = SqliteRepository::open(&database_path).expect("open primary repository");
        let second = SqliteRepository::open(&database_path).expect("open concurrent repository");

        let observer = Connection::open(&database_path).expect("open independent observer");
        let journal_mode: String = observer
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal mode");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");

        drop(observer);
        drop(second);
        drop(first);
    }

    #[test]
    fn future_schema_is_rejected_without_modifying_the_database() {
        let temporary = tempfile::tempdir().expect("create temporary directory");
        let database_path = temporary.path().join("future.sqlite3");
        let future_version = MIGRATIONS.last().expect("latest migration").version + 1;
        let connection = Connection::open(&database_path).expect("create future database");
        connection
            .execute_batch(&format!(
                r#"
                    CREATE TABLE schema_migrations (
                        version INTEGER PRIMARY KEY,
                        name TEXT NOT NULL,
                        applied_at TEXT NOT NULL DEFAULT (
                            strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                        )
                    ) STRICT;
                    INSERT INTO schema_migrations (version, name)
                    VALUES ({future_version}, 'future_schema');
                    CREATE TABLE future_sentinel (
                        value TEXT NOT NULL
                    ) STRICT;
                    INSERT INTO future_sentinel (value) VALUES ('preserve-me');
                "#
            ))
            .expect("seed future schema");
        drop(connection);
        let before = std::fs::read(&database_path).expect("snapshot future database");

        let error = match SqliteRepository::open(&database_path) {
            Ok(_) => panic!("older application must reject a future schema"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            RepositoryError::UnsupportedSchema {
                found,
                latest_supported
            } if found == future_version
                && latest_supported == future_version - 1
        ));
        let after = std::fs::read(&database_path).expect("re-read future database");
        assert_eq!(after, before);
        assert!(backup_files(temporary.path()).is_empty());
    }
}
