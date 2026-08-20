use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, ErrorCode, OptionalExtension, Transaction};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    application::{
        ClassicArtifactCopy, ClassicImportRepository, ClassicImportTransitionOutcome,
        RepositoryError,
    },
    domain::{
        ArtifactBundle, ClassicImportCounts, ClassicImportPlan, ClassicImportReport,
        ClassicImportState, StoredClassicImport,
    },
};

use super::SqliteRepository;

impl ClassicImportRepository for SqliteRepository {
    fn classic_import_save_dry_run(
        &self,
        data_root: &str,
        download_root: Option<&str>,
        data_root_label: &str,
        download_root_label: Option<&str>,
        plan: &ClassicImportPlan,
    ) -> Result<StoredClassicImport, RepositoryError> {
        let connection = self.connection()?;
        let import_id = format!("classic-import-{}", Uuid::new_v4());
        let created_at = now_unix_ms();
        let counts = ClassicImportCounts::from_plan(plan);
        let can_apply = !plan.favorites.is_empty()
            || !plan.search_history.is_empty()
            || !plan.auto_find_exclusions.is_empty()
            || !plan.hidden_galleries.is_empty()
            || !plan.pair_exclusions.is_empty()
            || !plan.series.is_empty()
            || plan.galleries.iter().any(|gallery| gallery.eligible);
        let report = ClassicImportReport {
            import_id: import_id.clone(),
            revision: 0,
            state: ClassicImportState::DryRun,
            data_root_label: data_root_label.to_owned(),
            download_root_label: download_root_label.map(str::to_owned),
            source_fingerprint: plan.source_fingerprint.clone(),
            counts,
            conflicts: plan.conflicts.clone(),
            galleries: plan.galleries.clone(),
            can_apply,
            created_at: created_at.clone(),
            applied_at: None,
            rolled_back_at: None,
            error_code: None,
            error_message: None,
        };
        let plan_json = serde_json::to_string(plan).map_err(|_| {
            RepositoryError::Other("Classic import plan serialization failed".into())
        })?;
        let report_json = serde_json::to_string(&report).map_err(|_| {
            RepositoryError::Other("Classic import report serialization failed".into())
        })?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(map_sqlite_error)?;
        transaction
            .execute(
                r#"
                    INSERT INTO classic_import_runs (
                        import_id, revision, state, source_schema_version,
                        data_root, download_root, data_root_label, download_root_label,
                        source_fingerprint, plan_json, report_json,
                        created_at, updated_at
                    ) VALUES (?1, 0, 'dry_run', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?10)
                "#,
                params![
                    import_id,
                    i64::from(plan.schema_version),
                    data_root,
                    download_root,
                    data_root_label,
                    download_root_label,
                    plan.source_fingerprint,
                    plan_json,
                    report_json,
                    created_at,
                ],
            )
            .map_err(map_sqlite_error)?;
        for hash in &plan.legacy_hashes {
            transaction
                .execute(
                    r#"
                        INSERT INTO classic_import_legacy_hashes (
                            import_id, gallery_id, page_hashes, file_hashes,
                            trusted_for_duplicate_blocking
                        ) VALUES (?1, ?2, ?3, ?4, 0)
                    "#,
                    params![
                        report.import_id,
                        hash.gallery_id,
                        i64::from(hash.page_hashes),
                        i64::from(hash.file_hashes),
                    ],
                )
                .map_err(map_sqlite_error)?;
        }
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(StoredClassicImport {
            report,
            data_root: data_root.to_owned(),
            download_root: download_root.map(str::to_owned),
            plan: plan.clone(),
        })
    }

    fn classic_import_get(
        &self,
        import_id: &str,
    ) -> Result<Option<StoredClassicImport>, RepositoryError> {
        let connection = self.connection()?;
        load_stored_import(&connection, import_id)
    }

    fn classic_import_existing_gallery_ids(
        &self,
        gallery_ids: &[i64],
    ) -> Result<Vec<i64>, RepositoryError> {
        let connection = self.connection()?;
        let mut result = Vec::new();
        let mut statement = connection
            .prepare("SELECT 1 FROM download_entries WHERE gallery_id = ?1 LIMIT 1")
            .map_err(map_sqlite_error)?;
        for gallery_id in gallery_ids.iter().copied().filter(|id| *id > 0) {
            if statement
                .query_row([gallery_id], |_| Ok(()))
                .optional()
                .map_err(map_sqlite_error)?
                .is_some()
            {
                result.push(gallery_id);
            }
        }
        result.sort_unstable();
        result.dedup();
        Ok(result)
    }

    fn classic_import_begin_apply(
        &self,
        import_id: &str,
        expected_revision: u64,
    ) -> Result<ClassicImportTransitionOutcome, RepositoryError> {
        let mut connection = self.connection()?;
        transition_import(
            &mut connection,
            import_id,
            expected_revision,
            &[ClassicImportState::DryRun],
            ClassicImportState::Applying,
        )
    }

    fn classic_import_copy_mark(
        &self,
        import_id: &str,
        gallery_id: i64,
        entry_id: &str,
        relative_directory: &str,
        copied_files: u32,
        copied_bytes: u64,
    ) -> Result<(), RepositoryError> {
        let connection = self.connection()?;
        let stored_path = connection
            .query_row(
                "SELECT relative_directory FROM classic_import_artifact_copies WHERE import_id=?1 AND gallery_id=?2",
                params![import_id, gallery_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(map_sqlite_error)?;
        if stored_path
            .as_deref()
            .is_some_and(|path| path != relative_directory)
        {
            return Err(RepositoryError::Other(
                "Classic import copy relative directory is immutable".into(),
            ));
        }
        connection
            .execute(
                r#"
                    INSERT INTO classic_import_artifact_copies (
                        import_id, gallery_id, entry_id, relative_directory,
                        copied_files, copied_bytes, state, created_at, updated_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, 'copied',
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                    )
                    ON CONFLICT(import_id, gallery_id) DO UPDATE SET
                        entry_id = excluded.entry_id,
                        copied_files = excluded.copied_files,
                        copied_bytes = excluded.copied_bytes,
                        state = 'copied',
                        updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                "#,
                params![
                    import_id,
                    gallery_id,
                    entry_id,
                    relative_directory,
                    i64::from(copied_files),
                    to_sql_integer(copied_bytes, "copied bytes")?,
                ],
            )
            .map_err(map_sqlite_error)?;
        Ok(())
    }

    fn classic_import_commit_apply(
        &self,
        import_id: &str,
        expected_revision: u64,
        bundles: &[ArtifactBundle],
    ) -> Result<ClassicImportTransitionOutcome, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_sqlite_error)?;
        let Some(mut stored) = load_stored_import(&transaction, import_id)? else {
            return Ok(ClassicImportTransitionOutcome::NotFound);
        };
        if stored.report.revision != expected_revision {
            return Ok(ClassicImportTransitionOutcome::RevisionConflict {
                actual_revision: stored.report.revision,
            });
        }
        if stored.report.state != ClassicImportState::Applying {
            return Ok(ClassicImportTransitionOutcome::InvalidState(format!(
                "cannot commit from {}",
                stored.report.state.as_str()
            )));
        }
        let mut sequence = 0i64;
        apply_favorites(&transaction, import_id, &stored.plan, &mut sequence)?;
        apply_search_history(&transaction, import_id, &stored.plan, &mut sequence)?;
        apply_visibility(&transaction, import_id, &stored.plan, &mut sequence)?;
        for bundle in bundles {
            apply_artifact(&transaction, import_id, bundle, &mut sequence)?;
        }
        apply_series(&transaction, import_id, &stored.plan, &mut sequence)?;
        transaction
            .execute(
                "UPDATE classic_import_artifact_copies SET state='registered', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE import_id=?1",
                [import_id],
            )
            .map_err(map_sqlite_error)?;
        stored.report.revision = stored.report.revision.saturating_add(1);
        stored.report.state = ClassicImportState::Applied;
        stored.report.applied_at = Some(now_unix_ms());
        stored.report.error_code = None;
        stored.report.error_message = None;
        update_run_from_report(&transaction, &stored.report)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(ClassicImportTransitionOutcome::Applied(Box::new(stored)))
    }

    fn classic_import_begin_rollback(
        &self,
        import_id: &str,
        expected_revision: u64,
    ) -> Result<ClassicImportTransitionOutcome, RepositoryError> {
        let mut connection = self.connection()?;
        transition_import(
            &mut connection,
            import_id,
            expected_revision,
            &[ClassicImportState::Applied, ClassicImportState::Failed],
            ClassicImportState::RollingBack,
        )
    }

    fn classic_import_copied_artifacts(
        &self,
        import_id: &str,
    ) -> Result<Vec<ClassicArtifactCopy>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                r#"
                    SELECT gallery_id, entry_id, relative_directory,
                           copied_files, copied_bytes
                    FROM classic_import_artifact_copies
                    WHERE import_id = ?1
                    ORDER BY gallery_id ASC
                "#,
            )
            .map_err(map_sqlite_error)?;
        let rows = statement
            .query_map([import_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })
            .map_err(map_sqlite_error)?;
        let mut result = Vec::new();
        for row in rows {
            let (gallery_id, entry_id, relative_directory, files, bytes) =
                row.map_err(map_sqlite_error)?;
            result.push(ClassicArtifactCopy {
                gallery_id,
                entry_id,
                relative_directory,
                copied_files: u32::try_from(files).unwrap_or(u32::MAX),
                copied_bytes: u64::try_from(bytes).unwrap_or_default(),
            });
        }
        Ok(result)
    }

    fn classic_import_commit_rollback(
        &self,
        import_id: &str,
        expected_revision: u64,
    ) -> Result<ClassicImportTransitionOutcome, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_sqlite_error)?;
        let Some(mut stored) = load_stored_import(&transaction, import_id)? else {
            return Ok(ClassicImportTransitionOutcome::NotFound);
        };
        if stored.report.revision != expected_revision {
            return Ok(ClassicImportTransitionOutcome::RevisionConflict {
                actual_revision: stored.report.revision,
            });
        }
        if stored.report.state != ClassicImportState::RollingBack {
            return Ok(ClassicImportTransitionOutcome::InvalidState(format!(
                "cannot rollback from {}",
                stored.report.state.as_str()
            )));
        }
        rollback_changes(&transaction, import_id)?;
        transaction
            .execute(
                "UPDATE classic_import_artifact_copies SET state='quarantined', updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE import_id=?1",
                [import_id],
            )
            .map_err(map_sqlite_error)?;
        stored.report.revision = stored.report.revision.saturating_add(1);
        stored.report.state = ClassicImportState::RolledBack;
        stored.report.rolled_back_at = Some(now_unix_ms());
        stored.report.error_code = None;
        stored.report.error_message = None;
        update_run_from_report(&transaction, &stored.report)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(ClassicImportTransitionOutcome::Applied(Box::new(stored)))
    }

    fn classic_import_fail(
        &self,
        import_id: &str,
        error_code: &str,
        error_message: &str,
    ) -> Result<Option<StoredClassicImport>, RepositoryError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction().map_err(map_sqlite_error)?;
        let Some(mut stored) = load_stored_import(&transaction, import_id)? else {
            return Ok(None);
        };
        if matches!(
            stored.report.state,
            ClassicImportState::Applied | ClassicImportState::RolledBack
        ) {
            return Ok(Some(stored));
        }
        stored.report.revision = stored.report.revision.saturating_add(1);
        stored.report.state = ClassicImportState::Failed;
        stored.report.error_code = Some(error_code.to_owned());
        stored.report.error_message = Some(error_message.to_owned());
        update_run_from_report(&transaction, &stored.report)?;
        transaction.commit().map_err(map_sqlite_error)?;
        Ok(Some(stored))
    }

    fn classic_import_incomplete(&self) -> Result<Vec<StoredClassicImport>, RepositoryError> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare(
                "SELECT import_id FROM classic_import_runs WHERE state IN ('applying','rolling_back') ORDER BY created_at ASC",
            )
            .map_err(map_sqlite_error)?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(map_sqlite_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(map_sqlite_error)?;
        let mut imports = Vec::new();
        for import_id in ids {
            if let Some(stored) = load_stored_import(&connection, &import_id)? {
                imports.push(stored);
            }
        }
        Ok(imports)
    }
}

fn load_stored_import(
    connection: &Connection,
    import_id: &str,
) -> Result<Option<StoredClassicImport>, RepositoryError> {
    let row = connection
        .query_row(
            r#"
                SELECT report_json, data_root, download_root, plan_json
                FROM classic_import_runs WHERE import_id = ?1
            "#,
            [import_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(map_sqlite_error)?;
    let Some((report_json, data_root, download_root, plan_json)) = row else {
        return Ok(None);
    };
    let report = serde_json::from_str::<ClassicImportReport>(&report_json)
        .map_err(|_| RepositoryError::Corrupt("Classic import report is malformed".into()))?;
    let plan = serde_json::from_str::<ClassicImportPlan>(&plan_json)
        .map_err(|_| RepositoryError::Corrupt("Classic import plan is malformed".into()))?;
    Ok(Some(StoredClassicImport {
        report,
        data_root,
        download_root,
        plan,
    }))
}

fn transition_import(
    connection: &mut Connection,
    import_id: &str,
    expected_revision: u64,
    allowed: &[ClassicImportState],
    next: ClassicImportState,
) -> Result<ClassicImportTransitionOutcome, RepositoryError> {
    let transaction = connection.transaction().map_err(map_sqlite_error)?;
    let Some(mut stored) = load_stored_import(&transaction, import_id)? else {
        return Ok(ClassicImportTransitionOutcome::NotFound);
    };
    if stored.report.revision != expected_revision {
        return Ok(ClassicImportTransitionOutcome::RevisionConflict {
            actual_revision: stored.report.revision,
        });
    }
    if !allowed.contains(&stored.report.state) {
        return Ok(ClassicImportTransitionOutcome::InvalidState(format!(
            "cannot transition {} to {}",
            stored.report.state.as_str(),
            next.as_str()
        )));
    }
    stored.report.revision = stored.report.revision.saturating_add(1);
    stored.report.state = next;
    stored.report.error_code = None;
    stored.report.error_message = None;
    update_run_from_report(&transaction, &stored.report)?;
    transaction.commit().map_err(map_sqlite_error)?;
    Ok(ClassicImportTransitionOutcome::Applied(Box::new(stored)))
}

fn update_run_from_report(
    transaction: &Transaction<'_>,
    report: &ClassicImportReport,
) -> Result<(), RepositoryError> {
    let report_json = serde_json::to_string(report)
        .map_err(|_| RepositoryError::Other("Classic import report serialization failed".into()))?;
    transaction
        .execute(
            r#"
                UPDATE classic_import_runs
                SET revision=?2, state=?3, report_json=?4,
                    updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    applied_at=?5, rolled_back_at=?6,
                    error_code=?7, error_message=?8
                WHERE import_id=?1
            "#,
            params![
                report.import_id,
                to_sql_integer(report.revision, "import revision")?,
                report.state.as_str(),
                report_json,
                report.applied_at,
                report.rolled_back_at,
                report.error_code,
                report.error_message,
            ],
        )
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn apply_favorites(
    transaction: &Transaction<'_>,
    import_id: &str,
    plan: &ClassicImportPlan,
    sequence: &mut i64,
) -> Result<(), RepositoryError> {
    for favorite in &plan.favorites {
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO favorites (
                        namespace, value, revision, created_at, updated_at
                    ) VALUES (
                        ?1, ?2, 0,
                        strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ','now')
                    )
                "#,
                params![favorite.namespace.as_str(), favorite.value],
            )
            .map_err(map_sqlite_error)?;
        if inserted > 0 {
            record_change(
                transaction,
                import_id,
                sequence,
                "favorite",
                &format!("{}:{}", favorite.namespace.as_str(), favorite.value),
                Some(0),
            )?;
        }
    }
    Ok(())
}

fn apply_search_history(
    transaction: &Transaction<'_>,
    import_id: &str,
    plan: &ClassicImportPlan,
    sequence: &mut i64,
) -> Result<(), RepositoryError> {
    for request in &plan.search_history {
        let fingerprint = search_fingerprint(request)?;
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO search_history (
                        fingerprint, text, include_tags_json, exclude_tags_json,
                        languages_json, sort, page_size, use_count, last_used_at
                    ) VALUES (
                        ?1, ?2, ?3, ?4, ?5, ?6, ?7, 1,
                        strftime('%Y-%m-%dT%H:%M:%fZ','now')
                    )
                "#,
                params![
                    fingerprint,
                    request.text,
                    serde_json::to_string(&request.include_tags).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&request.exclude_tags).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_string(&request.languages).unwrap_or_else(|_| "[]".into()),
                    serde_json::to_value(request.sort)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_owned))
                        .unwrap_or_else(|| "recent".into()),
                    i64::from(request.page_size),
                ],
            )
            .map_err(map_sqlite_error)?;
        if inserted > 0 {
            record_change(
                transaction,
                import_id,
                sequence,
                "search_history",
                &fingerprint,
                Some(1),
            )?;
        }
    }
    Ok(())
}

fn apply_visibility(
    transaction: &Transaction<'_>,
    import_id: &str,
    plan: &ClassicImportPlan,
    sequence: &mut i64,
) -> Result<(), RepositoryError> {
    for gallery_id in &plan.auto_find_exclusions {
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO auto_find_exclusions (gallery_id, reason, created_at)
                    VALUES (?1, 'Classic read-only import', strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                "#,
                [gallery_id],
            )
            .map_err(map_sqlite_error)?;
        if inserted > 0 {
            record_change(
                transaction,
                import_id,
                sequence,
                "auto_find_exclusion",
                &gallery_id.to_string(),
                None,
            )?;
        }
    }
    for gallery_id in &plan.hidden_galleries {
        let decision_id = format!("classic-import-hidden-{import_id}-{gallery_id}");
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO duplicate_hidden_galleries (
                        gallery_id, decision_id, created_at
                    ) VALUES (?1, ?2, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                "#,
                params![gallery_id, decision_id],
            )
            .map_err(map_sqlite_error)?;
        if inserted > 0 {
            record_change(
                transaction,
                import_id,
                sequence,
                "hidden_gallery",
                &gallery_id.to_string(),
                None,
            )?;
        }
    }
    for pair in &plan.pair_exclusions {
        let decision_id = format!(
            "classic-import-pair-{import_id}-{}-{}",
            pair.left_gallery_id, pair.right_gallery_id
        );
        let inserted = transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO duplicate_pair_exclusions (
                        parent_gallery_id, candidate_gallery_id, decision_id, created_at
                    ) VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                "#,
                params![pair.left_gallery_id, pair.right_gallery_id, decision_id],
            )
            .map_err(map_sqlite_error)?;
        if inserted > 0 {
            record_change(
                transaction,
                import_id,
                sequence,
                "pair_exclusion",
                &format!("{}:{}", pair.left_gallery_id, pair.right_gallery_id),
                None,
            )?;
        }
    }
    Ok(())
}

fn apply_artifact(
    transaction: &Transaction<'_>,
    import_id: &str,
    bundle: &ArtifactBundle,
    sequence: &mut i64,
) -> Result<(), RepositoryError> {
    bundle
        .validate()
        .map_err(|error| RepositoryError::Other(error.to_string()))?;
    let entry_id = bundle.artifact.entry_id.as_str();
    let gallery_id = bundle.gallery.id.get();
    transaction
        .execute(
            r#"
                INSERT INTO galleries (
                    gallery_id, revision, title, primary_artist,
                    source_page_count, primary_group
                ) VALUES (?1, 0, ?2, ?3, ?4, ?5)
            "#,
            params![
                gallery_id,
                bundle.gallery.metadata.title,
                bundle.gallery.metadata.primary_artist,
                i64::from(bundle.gallery.metadata.source_page_count),
                bundle.gallery.metadata.primary_group,
            ],
        )
        .map_err(map_sqlite_error)?;
    for artist in &bundle.gallery.metadata.artists {
        transaction
            .execute(
                "INSERT OR IGNORE INTO owned_gallery_artists (gallery_id, artist) VALUES (?1, ?2)",
                params![gallery_id, artist],
            )
            .map_err(map_sqlite_error)?;
    }
    transaction
        .execute(
            r#"
                INSERT INTO download_entries (
                    entry_id, gallery_id, revision, state, progress,
                    created_at, updated_at
                ) VALUES (
                    ?1, ?2, 0, 'completed', 100.0,
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ','now')
                )
            "#,
            params![entry_id, gallery_id],
        )
        .map_err(map_sqlite_error)?;
    let job_id = format!("classic-job-{import_id}-{gallery_id}");
    let request_id = format!("classic-request-{import_id}-{gallery_id}");
    transaction
        .execute(
            r#"
                INSERT INTO download_jobs (
                    job_id, request_id, entry_id, gallery_id, revision, state,
                    completed_units, total_units, attempt,
                    created_at, updated_at, started_at, finished_at
                ) VALUES (
                    ?1, ?2, ?3, ?4, 0, 'completed', ?5, ?5, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ','now')
                )
            "#,
            params![
                job_id,
                request_id,
                entry_id,
                gallery_id,
                i64::from(bundle.artifact.expected_page_count)
            ],
        )
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            r#"
                INSERT INTO download_attempts (
                    job_id, attempt, started_at, finished_at, outcome_state
                ) VALUES (
                    ?1, 1,
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                    'completed'
                )
            "#,
            [job_id],
        )
        .map_err(map_sqlite_error)?;
    transaction
        .execute(
            r#"
                INSERT INTO download_artifacts (
                    entry_id, gallery_id, revision, relative_directory,
                    expected_page_count, state, manifest_relative_path,
                    manifest_schema_version, writer_version, hash_profile_version,
                    completed_at
                ) VALUES (?1, ?2, 0, ?3, ?4, 'complete', ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                entry_id,
                gallery_id,
                bundle.artifact.relative_directory.as_str(),
                i64::from(bundle.artifact.expected_page_count),
                bundle
                    .artifact
                    .manifest_relative_path
                    .as_ref()
                    .map(|path| path.as_str()),
                bundle.artifact.manifest_schema_version.map(i64::from),
                bundle.artifact.writer_version,
                i64::from(bundle.artifact.hash_profile_version),
                bundle.artifact.completed_at,
            ],
        )
        .map_err(map_sqlite_error)?;
    for page in &bundle.pages {
        transaction
            .execute(
                r#"
                    INSERT INTO download_pages (
                        entry_id, gallery_id, source_page_number, relative_path,
                        state, byte_length, sha256, storage_format,
                        source_revision, verified_at, excluded
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    entry_id,
                    gallery_id,
                    i64::from(page.page_id.source_page_number.get()),
                    page.relative_path.as_str(),
                    page.state.as_str(),
                    page.byte_length
                        .map(|value| to_sql_integer(value, "page bytes"))
                        .transpose()?,
                    page.sha256.as_ref().map(|sha| sha.as_str()),
                    page.storage_format.map(|format| format.as_str()),
                    page.source_revision,
                    page.verified_at,
                    i64::from(page.excluded),
                ],
            )
            .map_err(map_sqlite_error)?;
    }
    record_change(
        transaction,
        import_id,
        sequence,
        "download_artifact",
        entry_id,
        Some(0),
    )?;
    Ok(())
}

fn apply_series(
    transaction: &Transaction<'_>,
    import_id: &str,
    plan: &ClassicImportPlan,
    sequence: &mut i64,
) -> Result<(), RepositoryError> {
    for group in &plan.series {
        let mut members = vec![group.parent_gallery_id];
        members.extend(group.member_gallery_ids.iter().copied());
        members.sort_unstable();
        members.dedup();
        let mut resolved = Vec::new();
        for gallery_id in members {
            let entry_id = transaction
                .query_row(
                    "SELECT entry_id FROM download_entries WHERE gallery_id=?1 ORDER BY created_at DESC LIMIT 1",
                    [gallery_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(map_sqlite_error)?;
            if let Some(entry_id) = entry_id {
                resolved.push((gallery_id, entry_id));
            }
        }
        if resolved.len() < 2 {
            continue;
        }
        let series_group_id = format!("classic-series-{import_id}-{}", group.parent_gallery_id);
        transaction
            .execute(
                r#"
                    INSERT OR IGNORE INTO duplicate_series_groups (
                        series_group_id, name, revision, created_at, updated_at
                    ) VALUES (
                        ?1, ?2, 0,
                        strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                        strftime('%Y-%m-%dT%H:%M:%fZ','now')
                    )
                "#,
                params![
                    series_group_id,
                    format!("Classic series {}", group.parent_gallery_id)
                ],
            )
            .map_err(map_sqlite_error)?;
        for (gallery_id, entry_id) in resolved {
            transaction
                .execute(
                    r#"
                        INSERT OR IGNORE INTO duplicate_series_members (
                            series_group_id, gallery_id, entry_id, created_at
                        ) VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
                    "#,
                    params![series_group_id, gallery_id, entry_id],
                )
                .map_err(map_sqlite_error)?;
        }
        record_change(
            transaction,
            import_id,
            sequence,
            "series_group",
            &series_group_id,
            Some(0),
        )?;
    }
    Ok(())
}

fn record_change(
    transaction: &Transaction<'_>,
    import_id: &str,
    sequence: &mut i64,
    entity_kind: &str,
    entity_key: &str,
    after_revision: Option<u64>,
) -> Result<(), RepositoryError> {
    transaction
        .execute(
            r#"
                INSERT INTO classic_import_changes (
                    import_id, sequence, entity_kind, entity_key, after_revision
                ) VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                import_id,
                *sequence,
                entity_kind,
                entity_key,
                after_revision
                    .map(|value| to_sql_integer(value, "after revision"))
                    .transpose()?,
            ],
        )
        .map_err(map_sqlite_error)?;
    *sequence = sequence.saturating_add(1);
    Ok(())
}

fn rollback_changes(transaction: &Transaction<'_>, import_id: &str) -> Result<(), RepositoryError> {
    let mut statement = transaction
        .prepare(
            r#"
                SELECT entity_kind, entity_key, after_revision
                FROM classic_import_changes
                WHERE import_id=?1
                ORDER BY sequence DESC
            "#,
        )
        .map_err(map_sqlite_error)?;
    let rows = statement
        .query_map([import_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(map_sqlite_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(map_sqlite_error)?;
    drop(statement);
    for (kind, key, revision) in rows {
        match kind.as_str() {
            "download_artifact" => {
                let gallery_id = transaction
                    .query_row(
                        "SELECT gallery_id FROM download_entries WHERE entry_id=?1",
                        [&key],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(map_sqlite_error)?;
                transaction
                    .execute("DELETE FROM download_entries WHERE entry_id=?1", [&key])
                    .map_err(map_sqlite_error)?;
                if let Some(gallery_id) = gallery_id {
                    transaction
                        .execute(
                            "DELETE FROM galleries WHERE gallery_id=?1 AND NOT EXISTS (SELECT 1 FROM download_entries WHERE gallery_id=?1)",
                            [gallery_id],
                        )
                        .map_err(map_sqlite_error)?;
                }
            }
            "favorite" => {
                if let Some((namespace, value)) = key.split_once(':') {
                    transaction
                        .execute(
                            "DELETE FROM favorites WHERE namespace=?1 AND value=?2 AND revision=?3",
                            params![namespace, value, revision.unwrap_or_default()],
                        )
                        .map_err(map_sqlite_error)?;
                }
            }
            "search_history" => {
                transaction
                    .execute(
                        "DELETE FROM search_history WHERE fingerprint=?1 AND use_count=?2",
                        params![key, revision.unwrap_or(1)],
                    )
                    .map_err(map_sqlite_error)?;
            }
            "auto_find_exclusion" => {
                transaction
                    .execute(
                        "DELETE FROM auto_find_exclusions WHERE gallery_id=?1 AND reason='Classic read-only import'",
                        [key.parse::<i64>().unwrap_or_default()],
                    )
                    .map_err(map_sqlite_error)?;
            }
            "hidden_gallery" => {
                transaction
                    .execute(
                        "DELETE FROM duplicate_hidden_galleries WHERE gallery_id=?1 AND decision_id LIKE 'classic-import-hidden-%'",
                        [key.parse::<i64>().unwrap_or_default()],
                    )
                    .map_err(map_sqlite_error)?;
            }
            "pair_exclusion" => {
                if let Some((left, right)) = key.split_once(':') {
                    transaction
                        .execute(
                            "DELETE FROM duplicate_pair_exclusions WHERE parent_gallery_id=?1 AND candidate_gallery_id=?2 AND decision_id LIKE 'classic-import-pair-%'",
                            params![left.parse::<i64>().unwrap_or_default(), right.parse::<i64>().unwrap_or_default()],
                        )
                        .map_err(map_sqlite_error)?;
                }
            }
            "series_group" => {
                transaction
                    .execute(
                        "DELETE FROM duplicate_series_groups WHERE series_group_id=?1 AND revision=?2",
                        params![key, revision.unwrap_or_default()],
                    )
                    .map_err(map_sqlite_error)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn search_fingerprint(request: &crate::domain::SearchRequest) -> Result<String, RepositoryError> {
    let value = json!({
        "text": request.text,
        "includeTags": request.include_tags,
        "excludeTags": request.exclude_tags,
        "languages": request.languages,
        "sort": request.sort,
        "pageSize": request.page_size,
    });
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| RepositoryError::Other("search fingerprint serialization failed".into()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn now_unix_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn to_sql_integer(value: u64, field: &'static str) -> Result<i64, RepositoryError> {
    i64::try_from(value)
        .map_err(|_| RepositoryError::Other(format!("{field} exceeds SQLite range")))
}

fn map_sqlite_error(error: rusqlite::Error) -> RepositoryError {
    match &error {
        rusqlite::Error::SqliteFailure(details, _) => match details.code {
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => {
                RepositoryError::Busy("The Next database is busy; retry the import".into())
            }
            ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => {
                RepositoryError::Corrupt("The Next database is corrupt".into())
            }
            _ => RepositoryError::Other("Classic import database operation failed".into()),
        },
        _ => RepositoryError::Other("Classic import database operation failed".into()),
    }
}
