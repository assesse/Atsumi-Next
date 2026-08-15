use std::collections::BTreeMap;

use serde::{ser::SerializeMap, Serialize, Serializer};
use serde_json::{json, Value};

use crate::application::{ApplicationError, RepositoryError};
use crate::source::{SourceContractError, SourceErrorCode};

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
            ApplicationError::AutoFindNotRunning => Self {
                code: "AUTO_FIND_NOT_RUNNING".into(),
                message: "There is no active Auto Find refresh to cancel".into(),
                retryable: false,
                action: Some(ApiAction::None),
                details: None,
            },
            ApplicationError::DuplicateScanNotRunning => Self {
                code: "DUPLICATE_SCAN_NOT_RUNNING".into(),
                message: "There is no active duplicate scan to cancel".into(),
                retryable: false,
                action: Some(ApiAction::None),
                details: None,
            },
            ApplicationError::DuplicateCandidateNotFound(candidate_id) => Self {
                code: "DUPLICATE_CANDIDATE_NOT_FOUND".into(),
                message: "The duplicate candidate no longer exists; reload the review list".into(),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([(
                    "candidateId".into(),
                    json!(candidate_id),
                )])),
            },
            ApplicationError::DownloadPipeline(error) => Self {
                code: error.code.as_str().into(),
                message: error.message,
                retryable: error.retryable,
                action: Some(if error.retryable {
                    ApiAction::Retry
                } else if matches!(
                    error.code,
                    crate::application::DownloadPipelineErrorCode::ArtifactMissing
                        | crate::application::DownloadPipelineErrorCode::HashMismatch
                        | crate::application::DownloadPipelineErrorCode::ManifestInvalid
                        | crate::application::DownloadPipelineErrorCode::QuarantineConflict
                ) {
                    ApiAction::Review
                } else {
                    ApiAction::None
                }),
                details: None,
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
            RepositoryError::UnsupportedSchema {
                found,
                latest_supported,
            } => Self {
                code: "DATABASE_SCHEMA_NEWER".into(),
                message: "이 데이터는 더 새로운 Atsumi Next에서 만들어졌습니다. 앱을 업데이트하거나 백업본으로 복구하세요.".into(),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([
                    ("actualSchemaVersion".into(), json!(found)),
                    ("supportedSchemaVersion".into(), json!(latest_supported)),
                ])),
            },
            RepositoryError::MigrationBackup(message) => Self {
                code: "DATABASE_BACKUP_FAILED".into(),
                message: "안전 백업을 만들 수 없어 데이터 업데이트를 중단했습니다.".into(),
                retryable: false,
                action: Some(ApiAction::Review),
                details: Some(BTreeMap::from([("reason".into(), json!(message))])),
            },
            RepositoryError::Other(message) => Self {
                code: "DATABASE_ERROR".into(),
                message,
                retryable: false,
                action: Some(ApiAction::None),
                details: None,
            },
            RepositoryError::Source(error) => source_error(error),
        }
    }
}

fn source_error(error: SourceContractError) -> ApiError {
    let (code, message, action) = match error.code {
        SourceErrorCode::Cancelled => (
            "REQUEST_CANCELLED",
            "The source request was cancelled",
            ApiAction::None,
        ),
        SourceErrorCode::Validation => (
            "SOURCE_VALIDATION",
            "The source rejected the request",
            ApiAction::None,
        ),
        SourceErrorCode::NotFound => (
            "SOURCE_NOT_FOUND",
            "The requested item was not found in the current source",
            ApiAction::None,
        ),
        SourceErrorCode::Protocol => (
            "SOURCE_PROTOCOL",
            "The source response did not match the supported protocol",
            ApiAction::None,
        ),
        SourceErrorCode::InvalidData => (
            "SOURCE_INVALID_DATA",
            "The source returned data that could not be read safely",
            ApiAction::None,
        ),
        SourceErrorCode::RateLimited => (
            "SOURCE_RATE_LIMITED",
            "The source is rate-limiting requests; try again later",
            ApiAction::Retry,
        ),
        SourceErrorCode::TemporarilyUnavailable => (
            "SOURCE_TEMPORARILY_UNAVAILABLE",
            "The source is temporarily unavailable",
            ApiAction::Retry,
        ),
        SourceErrorCode::Timeout => (
            "SOURCE_TIMEOUT",
            "The source did not respond in time",
            ApiAction::Retry,
        ),
        SourceErrorCode::Unauthorized => (
            "SOURCE_UNAUTHORIZED",
            "The source rejected the connection",
            ApiAction::Reconnect,
        ),
        SourceErrorCode::Transport => (
            "NETWORK_OFFLINE",
            "A connection to the source could not be established",
            ApiAction::Reconnect,
        ),
        SourceErrorCode::ImageCandidatesExhausted => (
            "IMAGE_CANDIDATES_EXHAUSTED",
            "No supported image candidate could be loaded",
            ApiAction::None,
        ),
        SourceErrorCode::ImageResponseInvalid => (
            "IMAGE_RESPONSE_INVALID",
            "The source returned a response that is not a supported image",
            ApiAction::Retry,
        ),
        SourceErrorCode::ImageDecodeFailed => (
            "IMAGE_DECODE_FAILED",
            "The image could not be decoded safely",
            ApiAction::Review,
        ),
    };

    let mut details = BTreeMap::from([
        ("category".into(), json!(error.category.as_str())),
        ("sourceCode".into(), json!(error.code.as_str())),
    ]);
    if let Some(http_status) = error.http_status {
        details.insert("httpStatus".into(), json!(http_status));
    }
    if let Some(retry_after_seconds) = error.retry_after_seconds {
        details.insert("retryAfterSeconds".into(), json!(retry_after_seconds));
    }

    ApiError {
        code: code.into(),
        message: message.into(),
        retryable: error.retryable,
        action: Some(action),
        details: Some(details),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{
        map_http_status, map_transport_failure, SourceContractError, TransportFailureKind,
    };

    #[test]
    fn rate_limit_errors_keep_stable_retry_metadata() {
        let source = map_http_status(429, Some(17)).expect_err("429 must be an error");
        let api = ApiError::from(RepositoryError::from(source));

        assert_eq!(api.code, "SOURCE_RATE_LIMITED");
        assert!(api.retryable);
        assert_eq!(api.action, Some(ApiAction::Retry));
        assert_eq!(
            api.details,
            Some(BTreeMap::from([
                ("category".into(), json!("remote")),
                ("httpStatus".into(), json!(429)),
                ("retryAfterSeconds".into(), json!(17)),
                ("sourceCode".into(), json!("rate_limited")),
            ]))
        );
    }

    #[test]
    fn transport_error_message_does_not_expose_internal_detail() {
        let source = map_transport_failure(
            TransportFailureKind::Connection,
            "request containing private search terms failed",
        );
        let api = ApiError::from(RepositoryError::from(source));

        assert_eq!(api.code, "NETWORK_OFFLINE");
        assert!(api.retryable);
        assert_eq!(api.action, Some(ApiAction::Reconnect));
        assert_eq!(
            api.message,
            "A connection to the source could not be established"
        );
        assert!(!api.message.contains("private search terms"));
    }

    #[test]
    fn image_failures_use_stable_codes_without_exposing_source_details() {
        let failures = [
            (
                SourceContractError::image_candidates_exhausted(),
                "IMAGE_CANDIDATES_EXHAUSTED",
            ),
            (
                SourceContractError::image_response_invalid(
                    "https://private.example/image?token=secret returned HTML",
                ),
                "IMAGE_RESPONSE_INVALID",
            ),
            (
                SourceContractError::image_decode_failed(
                    "decoder failed for C:\\Users\\private\\download.webp",
                ),
                "IMAGE_DECODE_FAILED",
            ),
        ];

        for (source, expected_code) in failures {
            let api = ApiError::from(RepositoryError::from(source));
            assert_eq!(api.code, expected_code);
            assert!(!api.message.contains("private"));
            assert!(!api.message.contains("secret"));
            assert!(!api.message.contains("C:\\Users"));
        }
    }
}
