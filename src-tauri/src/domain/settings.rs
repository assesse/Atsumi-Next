use serde::{Deserialize, Serialize};

use super::{validate_folder_name_template, ValidationError, DEFAULT_FOLDER_NAME_TEMPLATE};

pub const DEFAULT_MAX_COLUMNS: u32 = 3;
pub const DEFAULT_PREVIEW_WIDTH: u32 = 220;
pub const DEFAULT_CACHE_LIMIT_GB: u32 = 10;
pub const DEFAULT_CONCURRENT_IMAGE_REQUESTS: u32 = 5;
pub const DEFAULT_REQUEST_START_INTERVAL_MS: u64 = 25;

/// Converts only well-formed Windows verbatim filesystem paths to the form a
/// user normally reads and edits. Device paths and malformed prefixes are
/// deliberately left unchanged.
pub fn windows_path_for_display(value: &str) -> String {
    const VERBATIM_PREFIX: &str = "\\\\?\\";
    const VERBATIM_UNC_PREFIX: &str = "\\\\?\\UNC\\";

    if let Some(rest) = value.strip_prefix(VERBATIM_UNC_PREFIX) {
        let mut components = rest.split('\\');
        if components.next().is_some_and(|server| !server.is_empty())
            && components.next().is_some_and(|share| !share.is_empty())
        {
            return format!(r"\\{rest}");
        }
        return value.to_owned();
    }

    if let Some(rest) = value.strip_prefix(VERBATIM_PREFIX) {
        let bytes = rest.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return rest.to_owned();
        }
    }

    value.to_owned()
}

pub fn download_root_for_display(value: &str) -> String {
    #[cfg(windows)]
    {
        windows_path_for_display(value)
    }
    #[cfg(not(windows))]
    {
        value.to_owned()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoFindHistoryMode {
    #[default]
    IncludeAllHistory,
    NewerThanOldestDownloaded,
}

impl AutoFindHistoryMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::IncludeAllHistory => "include_all_history",
            Self::NewerThanOldestDownloaded => "newer_than_oldest_downloaded",
        }
    }

    pub fn from_database(value: &str) -> Option<Self> {
        match value {
            "include_all_history" => Some(Self::IncludeAllHistory),
            "newer_than_oldest_downloaded" => Some(Self::NewerThanOldestDownloaded),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSnapshot {
    pub revision: u64,
    pub download_root: String,
    pub folder_name_template: String,
    pub max_columns: u32,
    pub preview_width: u32,
    pub cache_limit_gb: u32,
    pub concurrent_image_requests: u32,
    pub request_start_interval_ms: u64,
    pub auto_find_history_mode: AutoFindHistoryMode,
}

impl Default for SettingsSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            download_root: String::new(),
            folder_name_template: DEFAULT_FOLDER_NAME_TEMPLATE.to_owned(),
            max_columns: DEFAULT_MAX_COLUMNS,
            preview_width: DEFAULT_PREVIEW_WIDTH,
            cache_limit_gb: DEFAULT_CACHE_LIMIT_GB,
            concurrent_image_requests: DEFAULT_CONCURRENT_IMAGE_REQUESTS,
            request_start_interval_ms: DEFAULT_REQUEST_START_INTERVAL_MS,
            auto_find_history_mode: AutoFindHistoryMode::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SettingsPatch {
    pub download_root: Option<String>,
    pub folder_name_template: Option<String>,
    pub max_columns: Option<u32>,
    pub preview_width: Option<u32>,
    pub cache_limit_gb: Option<u32>,
    pub concurrent_image_requests: Option<u32>,
    pub request_start_interval_ms: Option<u64>,
    pub auto_find_history_mode: Option<AutoFindHistoryMode>,
}

impl SettingsSnapshot {
    pub fn apply_patch(&self, patch: SettingsPatch) -> Result<Self, ValidationError> {
        let mut next = self.clone();

        if let Some(value) = patch.download_root {
            next.download_root = value;
        }
        if let Some(value) = patch.folder_name_template {
            next.folder_name_template = value;
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
        if let Some(value) = patch.auto_find_history_mode {
            next.auto_find_history_mode = value;
        }

        // Windows canonicalization is still used at filesystem boundaries,
        // but settings remain human-readable and editable.
        next.download_root = download_root_for_display(&next.download_root);

        next.revision = self
            .revision
            .checked_add(1)
            .ok_or_else(|| ValidationError::new("revision", "cannot be incremented"))?;
        next.validate()?;
        Ok(next)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        validate_folder_name_template(&self.folder_name_template)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_display_paths_remove_only_safe_verbatim_prefixes() {
        assert_eq!(windows_path_for_display(r"\\?\D:\AD"), r"D:\AD");
        assert_eq!(
            windows_path_for_display(r"\\?\UNC\server\share\AD"),
            r"\\server\share\AD"
        );
        assert_eq!(windows_path_for_display(r"D:\AD"), r"D:\AD");
        assert_eq!(
            windows_path_for_display(r"\\server\share\AD"),
            r"\\server\share\AD"
        );
        assert_eq!(
            windows_path_for_display(r"\\.\PhysicalDrive0"),
            r"\\.\PhysicalDrive0"
        );
        assert_eq!(
            windows_path_for_display(r"\\?\UNC\server"),
            r"\\?\UNC\server"
        );
        assert_eq!(windows_path_for_display(r"\\?\D:AD"), r"\\?\D:AD");
        assert_eq!(
            windows_path_for_display(r"\\?\Volume{1234}\AD"),
            r"\\?\Volume{1234}\AD"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_download_roots_are_not_rewritten() {
        assert_eq!(download_root_for_display(r"\\?\D:\AD"), r"\\?\D:\AD");
    }
}
