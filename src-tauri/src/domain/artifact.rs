use std::{collections::BTreeSet, fmt, path::Component, str::FromStr};

use serde::Serialize;

use super::{Gallery, GalleryId, GalleryPageId, SourcePageNumber, ValidationError};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct DownloadEntryId(String);

impl DownloadEntryId {
    pub fn new(value: impl Into<String>) -> Result<Self, ValidationError> {
        let value = value.into().trim().to_owned();
        if value.is_empty() {
            return Err(ValidationError::new("entryId", "must not be empty"));
        }
        if value.len() > 200 {
            return Err(ValidationError::new("entryId", "must be at most 200 bytes"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for DownloadEntryId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ArtifactRelativePath(String);

impl ArtifactRelativePath {
    pub fn new(value: impl AsRef<str>) -> Result<Self, ValidationError> {
        let path = std::path::Path::new(value.as_ref().trim());
        let mut parts = Vec::new();

        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(ValidationError::new(
                        "relativePath",
                        "must stay within the configured download root",
                    ));
                }
            }
        }

        if parts.is_empty() {
            return Err(ValidationError::new("relativePath", "must not be empty"));
        }
        Ok(Self(parts.join("/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn is_descendant_of(&self, directory: &Self) -> bool {
        self.0
            .strip_prefix(&directory.0)
            .is_some_and(|suffix| suffix.starts_with('/'))
    }
}

impl fmt::Display for ArtifactRelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadArtifactState {
    Incomplete,
    Complete,
    MissingArtifacts,
    Quarantined,
}

impl DownloadArtifactState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incomplete => "incomplete",
            Self::Complete => "complete",
            Self::MissingArtifacts => "missing_artifacts",
            Self::Quarantined => "quarantined",
        }
    }
}

impl FromStr for DownloadArtifactState {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "incomplete" => Ok(Self::Incomplete),
            "complete" => Ok(Self::Complete),
            "missing_artifacts" => Ok(Self::MissingArtifacts),
            "quarantined" => Ok(Self::Quarantined),
            _ => Err(ValidationError::new(
                "artifactState",
                "contains an unsupported value",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageArtifactState {
    Pending,
    Present,
    Missing,
    Quarantined,
}

impl PageArtifactState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Present => "present",
            Self::Missing => "missing",
            Self::Quarantined => "quarantined",
        }
    }
}

impl FromStr for PageArtifactState {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "present" => Ok(Self::Present),
            "missing" => Ok(Self::Missing),
            "quarantined" => Ok(Self::Quarantined),
            _ => Err(ValidationError::new(
                "pageArtifactState",
                "contains an unsupported value",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadArtifact {
    pub entry_id: DownloadEntryId,
    pub gallery_id: GalleryId,
    pub revision: u64,
    pub relative_directory: ArtifactRelativePath,
    pub expected_page_count: u32,
    pub state: DownloadArtifactState,
}

impl DownloadArtifact {
    pub fn new(
        entry_id: DownloadEntryId,
        gallery_id: GalleryId,
        revision: u64,
        relative_directory: ArtifactRelativePath,
        expected_page_count: u32,
        state: DownloadArtifactState,
    ) -> Result<Self, ValidationError> {
        if expected_page_count == 0 {
            return Err(ValidationError::new(
                "expectedPageCount",
                "must be greater than zero",
            ));
        }
        Ok(Self {
            entry_id,
            gallery_id,
            revision,
            relative_directory,
            expected_page_count,
            state,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageArtifact {
    pub entry_id: DownloadEntryId,
    pub page_id: GalleryPageId,
    pub relative_path: ArtifactRelativePath,
    pub state: PageArtifactState,
    pub byte_length: Option<u64>,
}

impl PageArtifact {
    pub fn new(
        entry_id: DownloadEntryId,
        gallery_id: GalleryId,
        source_page_number: SourcePageNumber,
        relative_path: ArtifactRelativePath,
        state: PageArtifactState,
        byte_length: Option<u64>,
    ) -> Result<Self, ValidationError> {
        if matches!(byte_length, Some(0)) {
            return Err(ValidationError::new(
                "byteLength",
                "must be greater than zero when known",
            ));
        }
        Ok(Self {
            entry_id,
            page_id: GalleryPageId {
                gallery_id,
                source_page_number,
            },
            relative_path,
            state,
            byte_length,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactBundle {
    pub gallery: Gallery,
    pub artifact: DownloadArtifact,
    pub pages: Vec<PageArtifact>,
}

impl ArtifactBundle {
    pub fn new(
        gallery: Gallery,
        artifact: DownloadArtifact,
        pages: Vec<PageArtifact>,
    ) -> Result<Self, ValidationError> {
        let bundle = Self {
            gallery,
            artifact,
            pages,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.gallery.id != self.artifact.gallery_id {
            return Err(ValidationError::new(
                "galleryId",
                "gallery and download artifact must match",
            ));
        }
        let mut page_numbers = BTreeSet::new();
        let mut paths = BTreeSet::new();
        for page in &self.pages {
            if page.entry_id != self.artifact.entry_id {
                return Err(ValidationError::new(
                    "entryId",
                    "page and download artifact must match",
                ));
            }
            if page.page_id.gallery_id != self.gallery.id {
                return Err(ValidationError::new(
                    "galleryId",
                    "page and gallery must match",
                ));
            }
            if page.page_id.source_page_number.get() > self.artifact.expected_page_count {
                return Err(ValidationError::new(
                    "sourcePageNumber",
                    "must not exceed the expected page count",
                ));
            }
            if !page_numbers.insert(page.page_id.source_page_number) {
                return Err(ValidationError::new(
                    "sourcePageNumber",
                    "must be unique within a download artifact",
                ));
            }
            if !paths.insert(&page.relative_path) {
                return Err(ValidationError::new(
                    "relativePath",
                    "must be unique within a download artifact",
                ));
            }
            if !page
                .relative_path
                .is_descendant_of(&self.artifact.relative_directory)
            {
                return Err(ValidationError::new(
                    "relativePath",
                    "page path must be inside the download artifact directory",
                ));
            }
        }

        if self.artifact.state == DownloadArtifactState::Complete
            && (self.pages.len() != self.artifact.expected_page_count as usize
                || self.pages.iter().any(|page| {
                    page.state != PageArtifactState::Present || page.byte_length.is_none()
                }))
        {
            return Err(ValidationError::new(
                "artifactState",
                "complete artifacts require every verified source page",
            ));
        }
        Ok(())
    }
}
