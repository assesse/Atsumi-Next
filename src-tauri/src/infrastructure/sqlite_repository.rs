use std::{
    path::Path,
    sync::{Mutex, MutexGuard},
    time::Duration,
};

use rusqlite::{
    params, Connection, ErrorCode, OptionalExtension, Row, Transaction, TransactionBehavior,
};
use uuid::Uuid;

use crate::{
    application::{
        ArtifactRepository, DownloadMutationOutcome, DownloadQueueAddOutcome, DownloadQueueRecord,
        DownloadRepository, RepositoryError, StateRepository,
    },
    domain::{
        ArtifactBundle, ArtifactRelativePath, DownloadArtifact, DownloadArtifactState,
        DownloadChangedEvent, DownloadEntry, DownloadEntryId, DownloadJobProjection,
        DownloadListRequest, DownloadPage, DownloadReviewKind, FixtureDownloadJobDescriptor,
        FixtureDownloadJobStep, Gallery, GalleryId, GalleryMetadata, JobEvent, JobRef, JobState,
        PageArtifact, PageArtifactState, SettingsSnapshot, SourcePageNumber,
        WindowPlacementSnapshot,
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
                RepositoryError::Other(format!(
                    "could not create database directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let connection = Connection::open(path).map_err(map_sqlite_error)?;
        connection
            .execute_batch("PRAGMA locking_mode = EXCLUSIVE;")
            .map_err(map_sqlite_error)?;
        Self::from_connection(connection)
    }

    pub fn open_in_memory() -> Result<Self, RepositoryError> {
        let connection = Connection::open_in_memory().map_err(map_sqlite_error)?;
        Self::from_connection(connection)
    }

    fn from_connection(mut connection: Connection) -> Result<Self, RepositoryError> {
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(map_sqlite_error)?;
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite_error)?;
        let report = MigrationRunner::run(&mut connection).map_err(map_migration_error)?;
        let recovered_entries = recover_volatile_downloads(&mut connection)?;
        tracing::info!(
            schema_version = report.current_version,
            migrations_applied = ?report.applied_versions,
            recovered_entries,
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

impl DownloadRepository for SqliteRepository {
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
                fixture_jobs: Vec::new(),
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

        let mut fixture_jobs = Vec::new();
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
                    fixture_jobs.push(FixtureDownloadJobDescriptor {
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
            fixture_jobs,
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
                        expected_page_count, state
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                    ON CONFLICT (entry_id) DO UPDATE SET
                        gallery_id = excluded.gallery_id,
                        revision = excluded.revision,
                        relative_directory = excluded.relative_directory,
                        expected_page_count = excluded.expected_page_count,
                        state = excluded.state
                "#,
                params![
                    bundle.artifact.entry_id.as_str(),
                    bundle.artifact.gallery_id.get(),
                    to_sql_integer(bundle.artifact.revision, "download artifact revision")?,
                    bundle.artifact.relative_directory.as_str(),
                    i64::from(bundle.artifact.expected_page_count),
                    bundle.artifact.state.as_str(),
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
                            relative_path, state, byte_length
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
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
                        a.state
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
        let artifact = DownloadArtifact::new(
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

        let mut statement = connection
            .prepare(
                r#"
                    SELECT gallery_id, source_page_number, relative_path, state, byte_length
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
            pages.push(
                PageArtifact::new(
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
                .map_err(domain_corruption)?,
            );
        }

        ArtifactBundle::new(gallery, artifact, pages)
            .map(Some)
            .map_err(domain_corruption)
    }
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
            && (review_kind.is_none() || self.review_id.as_deref().map_or(true, str::is_empty))
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
            && (review_kind.is_none() || self.review_id.as_deref().map_or(true, str::is_empty))
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
    })
}

struct StoredPageArtifact {
    gallery_id: i64,
    source_page_number: i64,
    relative_path: String,
    page_state: String,
    byte_length: Option<i64>,
}

fn stored_page_artifact(row: &Row<'_>) -> rusqlite::Result<StoredPageArtifact> {
    Ok(StoredPageArtifact {
        gallery_id: row.get(0)?,
        source_page_number: row.get(1)?,
        relative_path: row.get(2)?,
        page_state: row.get(3)?,
        byte_length: row.get(4)?,
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
        other => RepositoryError::Other(other.to_string()),
    }
}
