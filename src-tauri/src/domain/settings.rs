use serde::{Deserialize, Serialize};

use super::ValidationError;

pub const DEFAULT_MAX_COLUMNS: u32 = 3;
pub const DEFAULT_PREVIEW_WIDTH: u32 = 220;
pub const DEFAULT_CACHE_LIMIT_GB: u32 = 10;
pub const DEFAULT_CONCURRENT_IMAGE_REQUESTS: u32 = 5;
pub const DEFAULT_REQUEST_START_INTERVAL_MS: u64 = 25;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub revision: u64,
    pub download_root: String,
    pub max_columns: u32,
    pub preview_width: u32,
    pub cache_limit_gb: u32,
    pub concurrent_image_requests: u32,
    pub request_start_interval_ms: u64,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            download_root: String::new(),
            max_columns: DEFAULT_MAX_COLUMNS,
            preview_width: DEFAULT_PREVIEW_WIDTH,
            cache_limit_gb: DEFAULT_CACHE_LIMIT_GB,
            concurrent_image_requests: DEFAULT_CONCURRENT_IMAGE_REQUESTS,
            request_start_interval_ms: DEFAULT_REQUEST_START_INTERVAL_MS,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub download_root: Option<String>,
    pub max_columns: Option<u32>,
    pub preview_width: Option<u32>,
    pub cache_limit_gb: Option<u32>,
    pub concurrent_image_requests: Option<u32>,
    pub request_start_interval_ms: Option<u64>,
}

impl SettingsSnapshot {
    pub fn apply_patch(&self, patch: SettingsPatch) -> Result<Self, ValidationError> {
        let mut next = self.clone();

        if let Some(value) = patch.download_root {
            next.download_root = value;
        }
        if let Some(value) = patch.max_columns {
            next.max_columns = value;
        }
        if let Some(value) = patch.preview_width {
            next.preview_width = value;
        }
        if let Some(value) = patch.cache_limit_gb {
            next.cache_limit_gb = value;
        }
        if let Some(value) = patch.concurrent_image_requests {
            next.concurrent_image_requests = value;
        }
        if let Some(value) = patch.request_start_interval_ms {
            next.request_start_interval_ms = value;
        }

        next.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| ValidationError::new("revision", "cannot be incremented"))?;
        next.validate()?;
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if !(1..=4).contains(&self.max_columns) {
            return Err(ValidationError::new(
                "maxColumns",
                "must be between 1 and 4",
            ));
        }
        if !(160..=360).contains(&self.preview_width) {
            return Err(ValidationError::new(
                "previewWidth",
                "must be between 160 and 360",
            ));
        }
        if !(1..=30).contains(&self.cache_limit_gb) {
            return Err(ValidationError::new(
                "cacheLimitGb",
                "must be between 1 and 30",
            ));
        }
        if !(1..=30).contains(&self.concurrent_image_requests) {
            return Err(ValidationError::new(
                "concurrentImageRequests",
                "must be between 1 and 30",
            ));
        }
        if self.request_start_interval_ms > 5_000 {
            return Err(ValidationError::new(
                "requestStartIntervalMs",
                "must be at most 5000",
            ));
        }
        Ok(())
    }
}
