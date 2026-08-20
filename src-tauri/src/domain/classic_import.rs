use serde::{Deserialize, Serialize};

use super::{FavoriteKey, SearchRequest};

pub const CLASSIC_IMPORT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassicImportState {
    DryRun,
    Applying,
    Applied,
    RollingBack,
    RolledBack,
    Failed,
}

impl ClassicImportState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DryRun => "dry_run",
            Self::Applying => "applying",
            Self::Applied => "applied",
            Self::RollingBack => "rolling_back",
            Self::RolledBack => "rolled_back",
            Self::Failed => "failed",
        }
    }

    pub fn from_database(value: &str) -> Option<Self> {
        match value {
            "dry_run" => Some(Self::DryRun),
            "applying" => Some(Self::Applying),
            "applied" => Some(Self::Applied),
            "rolling_back" => Some(Self::RollingBack),
            "rolled_back" => Some(Self::RolledBack),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassicConflictSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassicConflictCode {
    StateMissing,
    StateInvalid,
    StateCompletedFolderMissing,
    FolderWithoutState,
    ManifestMissing,
    ManifestInvalid,
    ManifestGalleryMismatch,
    ExpectedPageCountMismatch,
    MissingPage,
    DuplicateGalleryFolder,
    HashOnly,
    HiddenGalleryHasFiles,
    ExistingNextGallery,
    ExistingDestination,
    ClassicSourceChanged,
    LegacyHashMismatch,
    SeriesMemberUnavailable,
    InventoryLimitReached,
}

impl ClassicConflictCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StateMissing => "state_missing",
            Self::StateInvalid => "state_invalid",
            Self::StateCompletedFolderMissing => "state_completed_folder_missing",
            Self::FolderWithoutState => "folder_without_state",
            Self::ManifestMissing => "manifest_missing",
            Self::ManifestInvalid => "manifest_invalid",
            Self::ManifestGalleryMismatch => "manifest_gallery_mismatch",
            Self::ExpectedPageCountMismatch => "expected_page_count_mismatch",
            Self::MissingPage => "missing_page",
            Self::DuplicateGalleryFolder => "duplicate_gallery_folder",
            Self::HashOnly => "hash_only",
            Self::HiddenGalleryHasFiles => "hidden_gallery_has_files",
            Self::ExistingNextGallery => "existing_next_gallery",
            Self::ExistingDestination => "existing_destination",
            Self::ClassicSourceChanged => "classic_source_changed",
            Self::LegacyHashMismatch => "legacy_hash_mismatch",
            Self::SeriesMemberUnavailable => "series_member_unavailable",
            Self::InventoryLimitReached => "inventory_limit_reached",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportConflict {
    pub conflict_id: String,
    pub code: ClassicConflictCode,
    pub severity: ClassicConflictSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gallery_id: Option<i64>,
    pub message: String,
    pub requires_acknowledgement: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassicSourceRootKind {
    Data,
    Downloads,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportPagePlan {
    pub source_page: u32,
    pub root_kind: ClassicSourceRootKind,
    pub relative_path: String,
    pub byte_length: u64,
    pub sha256: String,
    pub excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportGalleryPlan {
    pub gallery_id: i64,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub source_folder: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relative_directory: Option<String>,
    pub expected_pages: u32,
    pub pages: Vec<ClassicImportPagePlan>,
    pub planned_bytes: u64,
    pub eligible: bool,
    pub conflict_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicSeriesPlan {
    pub parent_gallery_id: i64,
    pub member_gallery_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicPairPlan {
    pub left_gallery_id: i64,
    pub right_gallery_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicLegacyHashSummary {
    pub gallery_id: i64,
    pub page_hashes: u32,
    pub file_hashes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportPlan {
    pub schema_version: u32,
    pub source_fingerprint: String,
    pub favorites: Vec<FavoriteKey>,
    pub search_history: Vec<SearchRequest>,
    pub auto_find_exclusions: Vec<i64>,
    pub hidden_galleries: Vec<i64>,
    pub pair_exclusions: Vec<ClassicPairPlan>,
    pub series: Vec<ClassicSeriesPlan>,
    pub legacy_hashes: Vec<ClassicLegacyHashSummary>,
    pub galleries: Vec<ClassicImportGalleryPlan>,
    pub conflicts: Vec<ClassicImportConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportCounts {
    pub favorites: u32,
    pub search_history: u32,
    pub exclusions: u32,
    pub hidden_galleries: u32,
    pub pair_exclusions: u32,
    pub series_groups: u32,
    pub galleries_discovered: u32,
    pub galleries_eligible: u32,
    pub page_files: u32,
    pub legacy_hash_rows: u32,
    pub planned_copy_bytes: u64,
    pub conflicts: u32,
}

impl ClassicImportCounts {
    pub fn from_plan(plan: &ClassicImportPlan) -> Self {
        let eligible = plan.galleries.iter().filter(|gallery| gallery.eligible);
        Self {
            favorites: plan.favorites.len() as u32,
            search_history: plan.search_history.len() as u32,
            exclusions: plan.auto_find_exclusions.len() as u32,
            hidden_galleries: plan.hidden_galleries.len() as u32,
            pair_exclusions: plan.pair_exclusions.len() as u32,
            series_groups: plan.series.len() as u32,
            galleries_discovered: plan.galleries.len() as u32,
            galleries_eligible: plan
                .galleries
                .iter()
                .filter(|gallery| gallery.eligible)
                .count() as u32,
            page_files: eligible
                .clone()
                .map(|gallery| gallery.pages.len() as u32)
                .sum(),
            legacy_hash_rows: plan
                .legacy_hashes
                .iter()
                .map(|item| item.page_hashes.saturating_add(item.file_hashes))
                .sum(),
            planned_copy_bytes: eligible.map(|gallery| gallery.planned_bytes).sum(),
            conflicts: plan.conflicts.len() as u32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportReport {
    pub import_id: String,
    pub revision: u64,
    pub state: ClassicImportState,
    pub data_root_label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download_root_label: Option<String>,
    pub source_fingerprint: String,
    pub counts: ClassicImportCounts,
    pub conflicts: Vec<ClassicImportConflict>,
    pub galleries: Vec<ClassicImportGalleryPlan>,
    pub can_apply: bool,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rolled_back_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicImportDryRunRequest {
    pub data_root: String,
    #[serde(default)]
    pub download_root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicImportApplyRequest {
    pub import_id: String,
    pub expected_revision: u64,
    pub accepted_conflict_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassicImportRollbackRequest {
    pub import_id: String,
    pub expected_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassicImportApplyResult {
    pub report: ClassicImportReport,
    pub imported_gallery_ids: Vec<i64>,
    pub copied_files: u32,
    pub copied_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClassicImport {
    pub report: ClassicImportReport,
    pub data_root: String,
    pub download_root: Option<String>,
    pub plan: ClassicImportPlan,
}
