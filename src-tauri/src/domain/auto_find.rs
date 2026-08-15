use serde::{Deserialize, Serialize};

use super::{GalleryId, GallerySummary, Language, SearchSort, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FavoriteNamespace {
    Artist,
    Group,
    Series,
    Character,
    Tag,
}

impl FavoriteNamespace {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Artist => "artist",
            Self::Group => "group",
            Self::Series => "series",
            Self::Character => "character",
            Self::Tag => "tag",
        }
    }

    pub fn from_database(value: &str) -> Option<Self> {
        match value {
            "artist" => Some(Self::Artist),
            "group" => Some(Self::Group),
            "series" => Some(Self::Series),
            "character" => Some(Self::Character),
            "tag" => Some(Self::Tag),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FavoriteKey {
    pub namespace: FavoriteNamespace,
    pub value: String,
}

impl FavoriteKey {
    pub fn normalized(mut self) -> Result<Self, ValidationError> {
        self.value = self
            .value
            .trim()
            .to_lowercase()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if self.value.is_empty() {
            return Err(ValidationError::new("favorite.value", "must not be empty"));
        }
        if self.value.len() > 200 {
            return Err(ValidationError::new(
                "favorite.value",
                "must be at most 200 bytes",
            ));
        }
        if self.value.chars().any(char::is_control) {
            return Err(ValidationError::new(
                "favorite.value",
                "must not contain control characters",
            ));
        }
        Ok(self)
    }

    pub fn search_token(&self) -> String {
        format!(
            "{}:{}",
            self.namespace.as_str(),
            self.value.replace(' ', "_")
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteRecord {
    pub namespace: FavoriteNamespace,
    pub value: String,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

impl FavoriteRecord {
    pub fn key(&self) -> FavoriteKey {
        FavoriteKey {
            namespace: self.namespace,
            value: self.value.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FavoriteMutationResult {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite: Option<FavoriteRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHistoryEntry {
    pub history_id: i64,
    pub text: String,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub languages: Vec<Language>,
    pub sort: SearchSort,
    pub page_size: u32,
    pub use_count: u64,
    pub last_used_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoFindRunState {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AutoFindRunState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    pub fn from_database(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "failed" => Some(Self::Failed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFindRun {
    pub run_id: String,
    pub revision: u64,
    pub state: AutoFindRunState,
    pub total_favorites: u32,
    pub completed_favorites: u32,
    pub candidates_found: u32,
    pub started_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFindCandidate {
    pub run_id: String,
    #[serde(flatten)]
    pub gallery: GallerySummary,
    pub matched_favorite: FavoriteKey,
    pub discovered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFindSnapshot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<AutoFindRun>,
    pub candidates: Vec<AutoFindCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoFindCandidateRecord {
    pub run_id: String,
    pub gallery: GallerySummary,
    pub matched_favorite: FavoriteKey,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFindExclusionResult {
    pub excluded_gallery_ids: Vec<GalleryId>,
    pub snapshot: AutoFindSnapshot,
}
