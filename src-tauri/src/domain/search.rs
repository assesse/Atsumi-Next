use serde::{Deserialize, Serialize};

use super::{GalleryId, ValidationError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Korean,
    Japanese,
    Chinese,
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSort {
    Recent,
    PopularToday,
    PopularWeek,
    PopularMonth,
    PopularYear,
    Random,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchRequest {
    pub text: String,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub languages: Vec<Language>,
    pub sort: SearchSort,
    pub page_size: u32,
}

impl SearchRequest {
    pub fn normalized(mut self) -> Result<Self, ValidationError> {
        self.text = self.text.trim().to_lowercase();
        if self.text.len() > 500 {
            return Err(ValidationError::new("text", "must be at most 500 bytes"));
        }
        if !(1..=200).contains(&self.page_size) {
            return Err(ValidationError::new(
                "pageSize",
                "must be between 1 and 200",
            ));
        }

        self.include_tags = normalized_tags(self.include_tags, "includeTags")?;
        self.exclude_tags = normalized_tags(self.exclude_tags, "excludeTags")?;
        self.languages.sort_unstable();
        self.languages.dedup();
        Ok(self)
    }
}

fn normalized_tags(tags: Vec<String>, field: &'static str) -> Result<Vec<String>, ValidationError> {
    if tags.len() > 100 {
        return Err(ValidationError::new(field, "must contain at most 100 tags"));
    }

    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let tag = tag.trim().to_lowercase();
        if tag.is_empty() {
            return Err(ValidationError::new(field, "must not contain empty tags"));
        }
        if tag.len() > 200 {
            return Err(ValidationError::new(
                field,
                "each tag must be at most 200 bytes",
            ));
        }
        normalized.push(tag);
    }
    normalized.sort_unstable();
    normalized.dedup();
    Ok(normalized)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GallerySummary {
    pub id: GalleryId,
    pub title: String,
    pub artist: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    pub series: Vec<String>,
    pub characters: Vec<String>,
    pub pages: u32,
    pub language: Language,
    pub tags: Vec<String>,
    pub published_rank: u32,
    pub popularity: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_key: Option<String>,
    pub thumbnail_width: u32,
    pub thumbnail_height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryPage {
    pub page: u32,
    pub total_pages: u32,
    pub items: Vec<GallerySummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSubmission {
    pub query_id: String,
    pub first_page: GalleryPage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GalleryDetail {
    #[serde(flatten)]
    pub summary: GallerySummary,
    pub related: Vec<GallerySummary>,
}
