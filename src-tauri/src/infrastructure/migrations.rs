use std::collections::BTreeMap;

use rusqlite::{params, Connection};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub version: i64,
    pub name: &'static str,
    pub sql: &'static str,
}

pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "settings_and_window_placement",
        sql: r#"
            CREATE TABLE settings (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                download_root TEXT NOT NULL,
                max_columns INTEGER NOT NULL CHECK (max_columns BETWEEN 1 AND 4),
                preview_width INTEGER NOT NULL CHECK (preview_width BETWEEN 160 AND 360),
                cache_limit_gb INTEGER NOT NULL CHECK (cache_limit_gb BETWEEN 1 AND 30),
                concurrent_image_requests INTEGER NOT NULL CHECK (concurrent_image_requests BETWEEN 1 AND 30),
                request_start_interval_ms INTEGER NOT NULL CHECK (request_start_interval_ms BETWEEN 0 AND 5000)
            ) STRICT;

            INSERT INTO settings (
                singleton,
                revision,
                download_root,
                max_columns,
                preview_width,
                cache_limit_gb,
                concurrent_image_requests,
                request_start_interval_ms
            ) VALUES (1, 0, '', 3, 220, 10, 5, 25);

            CREATE TABLE window_placement (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                x INTEGER,
                y INTEGER,
                width INTEGER NOT NULL CHECK (width BETWEEN 1 AND 32768),
                height INTEGER NOT NULL CHECK (height BETWEEN 1 AND 32768),
                maximized INTEGER NOT NULL CHECK (maximized IN (0, 1))
            ) STRICT;

            INSERT INTO window_placement (
                singleton, revision, x, y, width, height, maximized
            ) VALUES (1, 0, NULL, NULL, 1280, 820, 0);
        "#,
    },
    Migration {
        version: 2,
        name: "mock_job_event_foundation",
        sql: r#"
            CREATE TABLE download_entries (
                entry_id TEXT PRIMARY KEY,
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                state TEXT NOT NULL CHECK (state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined'
                )),
                progress REAL NOT NULL CHECK (progress BETWEEN 0.0 AND 100.0)
            ) STRICT;

            CREATE TABLE download_jobs (
                job_id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL UNIQUE,
                entry_id TEXT NOT NULL UNIQUE REFERENCES download_entries(entry_id) ON DELETE CASCADE,
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                state TEXT NOT NULL CHECK (state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined'
                )),
                completed_units INTEGER NOT NULL CHECK (completed_units >= 0),
                total_units INTEGER NOT NULL CHECK (total_units > 0)
            ) STRICT;

            CREATE INDEX download_jobs_gallery_id_idx ON download_jobs(gallery_id);
        "#,
    },
    Migration {
        version: 3,
        name: "gallery_and_artifact_foundation",
        sql: r#"
            CREATE TABLE settings_v3 (
                singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                download_root TEXT NOT NULL,
                max_columns INTEGER NOT NULL CHECK (max_columns BETWEEN 1 AND 4),
                preview_width INTEGER NOT NULL CHECK (preview_width BETWEEN 160 AND 360),
                cache_limit_gb INTEGER NOT NULL CHECK (cache_limit_gb BETWEEN 1 AND 30),
                concurrent_image_requests INTEGER NOT NULL CHECK (concurrent_image_requests BETWEEN 1 AND 30),
                request_start_interval_ms INTEGER NOT NULL CHECK (request_start_interval_ms BETWEEN 0 AND 5000)
            ) STRICT;

            INSERT INTO settings_v3 (
                singleton, revision, download_root, max_columns, preview_width,
                cache_limit_gb, concurrent_image_requests, request_start_interval_ms
            )
            SELECT
                singleton,
                revision,
                download_root,
                MAX(1, MIN(4, max_columns)),
                MAX(160, MIN(360, preview_width)),
                MAX(1, MIN(30, cache_limit_gb)),
                MAX(1, MIN(30, concurrent_image_requests)),
                MAX(0, MIN(5000, request_start_interval_ms))
            FROM settings;

            DROP TABLE settings;
            ALTER TABLE settings_v3 RENAME TO settings;

            CREATE TABLE galleries (
                gallery_id INTEGER PRIMARY KEY CHECK (gallery_id > 0),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                title TEXT NOT NULL CHECK (length(trim(title)) > 0),
                primary_artist TEXT,
                source_page_count INTEGER NOT NULL CHECK (source_page_count > 0)
            ) STRICT;

            CREATE UNIQUE INDEX download_entries_identity_idx
                ON download_entries(entry_id, gallery_id);

            CREATE TABLE download_artifacts (
                entry_id TEXT PRIMARY KEY,
                gallery_id INTEGER NOT NULL,
                revision INTEGER NOT NULL CHECK (revision >= 0),
                relative_directory TEXT NOT NULL UNIQUE
                    CHECK (length(trim(relative_directory)) > 0),
                expected_page_count INTEGER NOT NULL CHECK (expected_page_count > 0),
                state TEXT NOT NULL CHECK (state IN (
                    'incomplete', 'complete', 'missing_artifacts', 'quarantined'
                )),
                UNIQUE (entry_id, gallery_id),
                FOREIGN KEY (gallery_id)
                    REFERENCES galleries(gallery_id) ON DELETE RESTRICT,
                FOREIGN KEY (entry_id, gallery_id)
                    REFERENCES download_entries(entry_id, gallery_id) ON DELETE CASCADE
            ) STRICT;

            CREATE TABLE download_pages (
                entry_id TEXT NOT NULL,
                gallery_id INTEGER NOT NULL,
                source_page_number INTEGER NOT NULL CHECK (source_page_number > 0),
                relative_path TEXT NOT NULL CHECK (length(trim(relative_path)) > 0),
                state TEXT NOT NULL CHECK (state IN (
                    'pending', 'present', 'missing', 'quarantined'
                )),
                byte_length INTEGER CHECK (byte_length > 0),
                PRIMARY KEY (entry_id, source_page_number),
                UNIQUE (entry_id, relative_path),
                FOREIGN KEY (entry_id, gallery_id)
                    REFERENCES download_artifacts(entry_id, gallery_id) ON DELETE CASCADE
            ) STRICT;

            CREATE INDEX download_pages_gallery_page_idx
                ON download_pages(gallery_id, source_page_number);
        "#,
    },
    Migration {
        version: 4,
        name: "gallery_primary_group",
        sql: r#"
            ALTER TABLE galleries ADD COLUMN primary_group TEXT;
        "#,
    },
    Migration {
        version: 5,
        name: "download_queue_contract",
        sql: r#"
            ALTER TABLE download_entries ADD COLUMN review_kind TEXT
                CHECK (review_kind IS NULL OR review_kind IN (
                    'gallery_duplicate', 'internal_pages'
                ));
            ALTER TABLE download_entries ADD COLUMN review_id TEXT;

            CREATE TABLE download_queue_requests (
                request_id TEXT PRIMARY KEY CHECK (length(trim(request_id)) > 0),
                normalized_galleries TEXT NOT NULL
                    CHECK (length(normalized_galleries) > 0)
            ) STRICT;

            CREATE TABLE download_queue_request_entries (
                request_id TEXT NOT NULL,
                position INTEGER NOT NULL CHECK (position >= 0),
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                entry_id TEXT NOT NULL,
                response_state TEXT NOT NULL CHECK (response_state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined'
                )),
                response_progress REAL NOT NULL
                    CHECK (response_progress BETWEEN 0.0 AND 100.0),
                response_review_kind TEXT CHECK (
                    response_review_kind IS NULL OR response_review_kind IN (
                        'gallery_duplicate', 'internal_pages'
                    )
                ),
                response_review_id TEXT,
                PRIMARY KEY (request_id, position),
                UNIQUE (request_id, gallery_id),
                FOREIGN KEY (request_id)
                    REFERENCES download_queue_requests(request_id) ON DELETE CASCADE,
                FOREIGN KEY (entry_id)
                    REFERENCES download_entries(entry_id) ON DELETE RESTRICT
            ) STRICT;

            UPDATE download_jobs
            SET revision = revision + 1,
                state = 'interrupted'
            WHERE entry_id IN (
                SELECT duplicate.entry_id
                FROM download_entries duplicate
                WHERE duplicate.state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                )
                AND EXISTS (
                    SELECT 1
                    FROM download_entries keeper
                    WHERE keeper.gallery_id = duplicate.gallery_id
                      AND keeper.rowid < duplicate.rowid
                      AND keeper.state IN (
                          'queued', 'resolving_metadata', 'downloading', 'hashing',
                          'verifying', 'retry_wait'
                      )
                )
            );
            UPDATE download_entries
            SET revision = revision + 1,
                state = 'interrupted'
            WHERE state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                )
              AND EXISTS (
                    SELECT 1
                    FROM download_entries keeper
                    WHERE keeper.gallery_id = download_entries.gallery_id
                      AND keeper.rowid < download_entries.rowid
                      AND keeper.state IN (
                          'queued', 'resolving_metadata', 'downloading', 'hashing',
                          'verifying', 'retry_wait'
                      )
                );

            CREATE UNIQUE INDEX download_entries_active_gallery_idx
                ON download_entries(gallery_id)
                WHERE state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                );
            CREATE INDEX download_queue_request_entries_entry_idx
                ON download_queue_request_entries(entry_id);
        "#,
    },
    Migration {
        version: 6,
        name: "download_queue_response_revision",
        sql: r#"
            ALTER TABLE download_queue_request_entries
            ADD COLUMN response_revision INTEGER NOT NULL DEFAULT 0
                CHECK (response_revision >= 0);
        "#,
    },
    Migration {
        version: 7,
        name: "download_lifecycle_and_cancelled_state",
        sql: r#"
            PRAGMA defer_foreign_keys = ON;

            CREATE TABLE download_entries_v7 (
                entry_id TEXT PRIMARY KEY,
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                state TEXT NOT NULL CHECK (state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined', 'cancelled'
                )),
                progress REAL NOT NULL CHECK (progress BETWEEN 0.0 AND 100.0),
                review_kind TEXT CHECK (review_kind IS NULL OR review_kind IN (
                    'gallery_duplicate', 'internal_pages'
                )),
                review_id TEXT,
                created_at TEXT NOT NULL CHECK (length(created_at) > 0),
                updated_at TEXT NOT NULL CHECK (length(updated_at) > 0),
                UNIQUE (entry_id, gallery_id)
            ) STRICT;

            INSERT INTO download_entries_v7 (
                entry_id, gallery_id, revision, state, progress,
                review_kind, review_id, created_at, updated_at
            )
            SELECT
                entry_id, gallery_id, revision, state, progress,
                review_kind, review_id,
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            FROM download_entries;

            CREATE TABLE download_jobs_v7 (
                job_id TEXT PRIMARY KEY,
                request_id TEXT NOT NULL UNIQUE,
                entry_id TEXT NOT NULL UNIQUE
                    REFERENCES download_entries_v7(entry_id) ON DELETE CASCADE,
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                revision INTEGER NOT NULL CHECK (revision >= 0),
                state TEXT NOT NULL CHECK (state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined', 'cancelled'
                )),
                completed_units INTEGER NOT NULL CHECK (completed_units >= 0),
                total_units INTEGER NOT NULL CHECK (total_units > 0),
                attempt INTEGER NOT NULL CHECK (attempt > 0),
                last_error_code TEXT,
                last_error_message TEXT,
                created_at TEXT NOT NULL CHECK (length(created_at) > 0),
                updated_at TEXT NOT NULL CHECK (length(updated_at) > 0),
                started_at TEXT,
                finished_at TEXT
            ) STRICT;

            INSERT INTO download_jobs_v7 (
                job_id, request_id, entry_id, gallery_id, revision, state,
                completed_units, total_units, attempt,
                last_error_code, last_error_message,
                created_at, updated_at, started_at, finished_at
            )
            SELECT
                job_id, request_id, entry_id, gallery_id, revision, state,
                completed_units, total_units, 1,
                CASE WHEN state = 'interrupted' THEN 'JOB_INTERRUPTED' END,
                CASE WHEN state = 'interrupted'
                    THEN 'The application stopped before the job reached a terminal state'
                END,
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                CASE WHEN state != 'queued'
                    THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                END,
                CASE WHEN state IN (
                    'review_required', 'interrupted', 'failed', 'completed', 'quarantined'
                ) THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END
            FROM download_jobs;

            CREATE TABLE download_queue_request_entries_v7 (
                request_id TEXT NOT NULL,
                position INTEGER NOT NULL CHECK (position >= 0),
                gallery_id INTEGER NOT NULL CHECK (gallery_id > 0),
                entry_id TEXT NOT NULL,
                response_state TEXT NOT NULL CHECK (response_state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait', 'review_required', 'interrupted',
                    'failed', 'completed', 'quarantined', 'cancelled'
                )),
                response_progress REAL NOT NULL
                    CHECK (response_progress BETWEEN 0.0 AND 100.0),
                response_review_kind TEXT CHECK (
                    response_review_kind IS NULL OR response_review_kind IN (
                        'gallery_duplicate', 'internal_pages'
                    )
                ),
                response_review_id TEXT,
                response_revision INTEGER NOT NULL CHECK (response_revision >= 0),
                PRIMARY KEY (request_id, position),
                UNIQUE (request_id, gallery_id),
                FOREIGN KEY (request_id)
                    REFERENCES download_queue_requests(request_id) ON DELETE CASCADE,
                FOREIGN KEY (entry_id)
                    REFERENCES download_entries_v7(entry_id) ON DELETE RESTRICT
            ) STRICT;

            INSERT INTO download_queue_request_entries_v7 (
                request_id, position, gallery_id, entry_id,
                response_state, response_progress,
                response_review_kind, response_review_id, response_revision
            )
            SELECT
                request_id, position, gallery_id, entry_id,
                response_state, response_progress,
                response_review_kind, response_review_id, response_revision
            FROM download_queue_request_entries;

            CREATE TABLE download_artifacts_v7 (
                entry_id TEXT PRIMARY KEY,
                gallery_id INTEGER NOT NULL,
                revision INTEGER NOT NULL CHECK (revision >= 0),
                relative_directory TEXT NOT NULL UNIQUE
                    CHECK (length(trim(relative_directory)) > 0),
                expected_page_count INTEGER NOT NULL CHECK (expected_page_count > 0),
                state TEXT NOT NULL CHECK (state IN (
                    'incomplete', 'complete', 'missing_artifacts', 'quarantined'
                )),
                UNIQUE (entry_id, gallery_id),
                FOREIGN KEY (gallery_id)
                    REFERENCES galleries(gallery_id) ON DELETE RESTRICT,
                FOREIGN KEY (entry_id, gallery_id)
                    REFERENCES download_entries_v7(entry_id, gallery_id) ON DELETE CASCADE
            ) STRICT;

            INSERT INTO download_artifacts_v7 (
                entry_id, gallery_id, revision, relative_directory,
                expected_page_count, state
            )
            SELECT
                entry_id, gallery_id, revision, relative_directory,
                expected_page_count, state
            FROM download_artifacts;

            CREATE TABLE download_pages_v7 (
                entry_id TEXT NOT NULL,
                gallery_id INTEGER NOT NULL,
                source_page_number INTEGER NOT NULL CHECK (source_page_number > 0),
                relative_path TEXT NOT NULL CHECK (length(trim(relative_path)) > 0),
                state TEXT NOT NULL CHECK (state IN (
                    'pending', 'present', 'missing', 'quarantined'
                )),
                byte_length INTEGER CHECK (byte_length > 0),
                PRIMARY KEY (entry_id, source_page_number),
                UNIQUE (entry_id, relative_path),
                FOREIGN KEY (entry_id, gallery_id)
                    REFERENCES download_artifacts_v7(entry_id, gallery_id) ON DELETE CASCADE
            ) STRICT;

            INSERT INTO download_pages_v7 (
                entry_id, gallery_id, source_page_number,
                relative_path, state, byte_length
            )
            SELECT
                entry_id, gallery_id, source_page_number,
                relative_path, state, byte_length
            FROM download_pages;

            DROP TABLE download_pages;
            DROP TABLE download_artifacts;
            DROP TABLE download_queue_request_entries;
            DROP TABLE download_jobs;
            DROP TABLE download_entries;

            ALTER TABLE download_entries_v7 RENAME TO download_entries;
            ALTER TABLE download_jobs_v7 RENAME TO download_jobs;
            ALTER TABLE download_queue_request_entries_v7
                RENAME TO download_queue_request_entries;
            ALTER TABLE download_artifacts_v7 RENAME TO download_artifacts;
            ALTER TABLE download_pages_v7 RENAME TO download_pages;

            CREATE INDEX download_jobs_gallery_id_idx ON download_jobs(gallery_id);
            CREATE UNIQUE INDEX download_entries_active_gallery_idx
                ON download_entries(gallery_id)
                WHERE state IN (
                    'queued', 'resolving_metadata', 'downloading', 'hashing',
                    'verifying', 'retry_wait'
                );
            CREATE INDEX download_queue_request_entries_entry_idx
                ON download_queue_request_entries(entry_id);
            CREATE INDEX download_pages_gallery_page_idx
                ON download_pages(gallery_id, source_page_number);

            CREATE TABLE download_attempts (
                job_id TEXT NOT NULL
                    REFERENCES download_jobs(job_id) ON DELETE CASCADE,
                attempt INTEGER NOT NULL CHECK (attempt > 0),
                started_at TEXT NOT NULL CHECK (length(started_at) > 0),
                finished_at TEXT,
                outcome_state TEXT CHECK (
                    outcome_state IS NULL OR outcome_state IN (
                        'queued', 'resolving_metadata', 'downloading', 'hashing',
                        'verifying', 'retry_wait', 'review_required', 'interrupted',
                        'failed', 'completed', 'quarantined', 'cancelled'
                    )
                ),
                error_code TEXT,
                error_message TEXT,
                PRIMARY KEY (job_id, attempt)
            ) STRICT;

            INSERT INTO download_attempts (
                job_id, attempt, started_at, finished_at,
                outcome_state, error_code, error_message
            )
            SELECT
                job_id, attempt, created_at, finished_at,
                CASE WHEN finished_at IS NOT NULL THEN state END,
                last_error_code, last_error_message
            FROM download_jobs;
        "#,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub applied_versions: Vec<i64>,
    pub current_version: i64,
}

#[derive(Debug, Error)]
pub enum MigrationError {
    #[error("SQLite migration failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("migration {version} was recorded as {actual:?}, expected {expected:?}")]
    NameMismatch {
        version: i64,
        expected: &'static str,
        actual: String,
    },
}

pub struct MigrationRunner;

impl MigrationRunner {
    pub fn run(connection: &mut Connection) -> Result<MigrationReport, MigrationError> {
        connection.execute_batch(
            r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    applied_at TEXT NOT NULL DEFAULT (
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                ) STRICT;
            "#,
        )?;

        let applied = Self::applied_migrations(connection)?;
        for migration in MIGRATIONS {
            if let Some(actual_name) = applied.get(&migration.version) {
                if actual_name != migration.name {
                    return Err(MigrationError::NameMismatch {
                        version: migration.version,
                        expected: migration.name,
                        actual: actual_name.clone(),
                    });
                }
            }
        }

        let mut applied_versions = Vec::new();
        for migration in MIGRATIONS {
            if applied.contains_key(&migration.version) {
                continue;
            }

            let transaction = connection.transaction()?;
            transaction.execute_batch(migration.sql)?;
            transaction.execute(
                "INSERT INTO schema_migrations (version, name) VALUES (?1, ?2)",
                params![migration.version, migration.name],
            )?;
            transaction.commit()?;
            applied_versions.push(migration.version);
        }

        let current_version = connection.query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )?;

        Ok(MigrationReport {
            applied_versions,
            current_version,
        })
    }

    fn applied_migrations(
        connection: &Connection,
    ) -> Result<BTreeMap<i64, String>, rusqlite::Error> {
        let mut statement = connection
            .prepare("SELECT version, name FROM schema_migrations ORDER BY version ASC")?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect()
    }
}
