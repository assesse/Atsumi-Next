use std::collections::BTreeMap;

use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::{json, Value};

use crate::application::{ApplicationError, RepositoryError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiAction {
    Retry,
    Review,
    Reconnect,
    Reveal,
    None,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<ApiAction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<BTreeMap<String, Value>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiResult<T> {
    Success(T),
    Failure(ApiError),
}

impl<T> ApiResult<T> {
    pub fn success(data: T) -> Self {
        Self::Success(data)
    }

    pub fn failure(error: ApiError) -> Self {
        Self::Failure(error)
    }
}

impl<T, E> From<Result<T, E>> for ApiResult<T>
where
    E: Into<ApiError>,
{
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(data) => Self::Success(data),
            Err(error) => Self::Failure(error.into()),
        }
    }
}

impl<T> Serialize for ApiResult<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        match self {
            Self::Success(data) => {
                map.serialize_entry("ok", &true)?;
                map.serialize_entry("data", data)?;
            }
            Self::Failure(error) => {
                map.serialize_entry("ok", &false)?;
                map.serialize_entry("error", error)?;
            }
        }
        map.end()
    }
}

impl From<ApplicationError> for ApiError {
    fn from(error: ApplicationError) -> Self {
        match error {
            ApplicationError::Validation(error) => Self {
                code: "VALIDATION_ERROR".into(),
                message: error.to_string(),
                retryable: false,
                action: Some(ApiAction::None),
                details: Some(BTreeMap::from([
                    ("field".into(), json!(error.field)),
                    ("reason".into(), json!(error.message)),
                ])),
            },
            ApplicationError::RevisionConflict {
                resource,
                expected,
                actual,
            } => Self {
                code: "REVISION_CONFLICT".into(),
                message: format!(
                    "{resource} changed since it was loaded; reload the latest snapshot"
                ),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([
                    ("resource".into(), json!(resource)),
                    ("expectedRevision".into(), json!(expected)),
                    ("actualRevision".into(), json!(actual)),
                ])),
            },
            ApplicationError::QueryNotFound(query_id) => Self {
                code: "QUERY_NOT_FOUND".into(),
                message: "The search query is no longer available; submit it again".into(),
                retryable: false,
                action: Some(ApiAction::None),
                details: Some(BTreeMap::from([("queryId".into(), json!(query_id))])),
            },
            ApplicationError::GalleryNotFound(gallery_id) => Self {
                code: "SOURCE_NOT_FOUND".into(),
                message: "The gallery could not be found in the current source".into(),
                retryable: false,
                action: Some(ApiAction::None),
                details: Some(BTreeMap::from([(
                    "galleryId".into(),
                    json!(gallery_id.get()),
                )])),
            },
            ApplicationError::IdempotencyConflict { request_id } => Self {
                code: "IDEMPOTENCY_CONFLICT".into(),
                message: "The request ID was already used for a different gallery set".into(),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([("requestId".into(), json!(request_id))])),
            },
            ApplicationError::DownloadEntryNotFound(entry_id) => Self {
                code: "DOWNLOAD_ENTRY_NOT_FOUND".into(),
                message: "The download entry no longer exists; reload the download list".into(),
                retryable: false,
                action: Some(ApiAction::None),
                details: Some(BTreeMap::from([(
                    "entryId".into(),
                    json!(entry_id.as_str()),
                )])),
            },
            ApplicationError::InvalidDownloadState {
                entry_id,
                state,
                operation,
            } => Self {
                code: "INVALID_DOWNLOAD_STATE".into(),
                message: format!("The download cannot {operation} from its current state"),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([
                    ("entryId".into(), json!(entry_id.as_str())),
                    ("state".into(), json!(state.to_string())),
                    ("operation".into(), json!(operation)),
                ])),
            },
            ApplicationError::Repository(error) => error.into(),
        }
    }
}

impl From<RepositoryError> for ApiError {
    fn from(error: RepositoryError) -> Self {
        match error {
            RepositoryError::Busy(message) => Self {
                code: "DATABASE_BUSY".into(),
                message,
                retryable: true,
                action: Some(ApiAction::Retry),
                details: None,
            },
            RepositoryError::Corrupt(message) => Self {
                code: "DATABASE_CORRUPT".into(),
                message,
                retryable: false,
                action: Some(ApiAction::Review),
                details: None,
            },
            RepositoryError::Other(message) => Self {
                code: "DATABASE_ERROR".into(),
                message,
                retryable: false,
                action: Some(ApiAction::None),
                details: None,
            },
        }
    }
}
