# 오류 분류

## 원칙

- 사용자 문구, 개발 로그와 프로그램 분기용 code를 분리한다.
- retry 가능 여부는 호출부가 문자열로 추측하지 않는다.
- raw URL과 query는 detail에도 넣지 않는다. 허용된 host, HTTP status, attempt처럼 비식별 진단만 로그에 둔다.
- 같은 근본 원인은 화면마다 같은 code를 사용한다.

| Code | 사용자 문구 | Retry | 기본 행동 |
|---|---|---:|---|
| `NETWORK_OFFLINE` | 네트워크 연결을 확인하세요 | 예 | 연결 후 재시도 |
| `SOURCE_TIMEOUT` | 원본 사이트 응답이 늦습니다 | 예 | backoff 재시도 |
| `SOURCE_RATE_LIMITED` | 잠시 후 다시 시도합니다 | 예 | host cooldown |
| `SOURCE_NOT_FOUND` | 갤러리 정보를 찾을 수 없습니다 | 아니오 | 상세 확인 |
| `SOURCE_TEMPORARILY_UNAVAILABLE` | 원본 사이트를 일시적으로 사용할 수 없습니다 | 예 | backoff 재시도 |
| `SOURCE_UNAUTHORIZED` | 원본 사이트가 연결을 거부했습니다 | 아니오 | 연결 정책 확인 |
| `SOURCE_PROTOCOL` | 원본 응답 형식이 지원 계약과 다릅니다 | 아니오 | parser/fixture 검토 |
| `SOURCE_INVALID_DATA` | 원본 응답을 안전하게 읽을 수 없습니다 | 아니오 | payload/fixture 검토 |
| `IMAGE_CANDIDATES_EXHAUSTED` | 이미지 서버에서 파일을 찾지 못했습니다 | 조건부 | URL 진단 또는 재조회 |
| `IMAGE_RESPONSE_INVALID` | 이미지가 아닌 응답을 받았습니다 | 아니오 | 다른 후보를 모두 시도한 뒤 상세 확인 |
| `IMAGE_DECODE_FAILED` | 이미지 처리에 실패했습니다 | 조건부 | 원본 보존, 상세 확인 |
| `DOWNLOAD_ROOT_REQUIRED` | 다운로드 폴더를 먼저 선택하세요 | 아니오 | 첫 다운로드에서 선택 |
| `DOWNLOAD_ROOT_SELECTION_CANCELLED` | 폴더 선택을 취소해 대기열을 만들지 않았습니다 | 아니오 | 원할 때 다시 실행 |
| `DOWNLOAD_ROOT_UNAVAILABLE` | 선택한 다운로드 폴더를 사용할 수 없습니다 | 아니오 | 권한·경로 확인 |
| `FILESYSTEM_ERROR` | 파일 작업을 안전하게 완료하지 못했습니다 | 조건부 | 원인 제거 후 재시도 |
| `FILESYSTEM_MISSING` | 다운로드 폴더를 찾을 수 없습니다 | 아니오 | 재연결 또는 재다운로드 |
| `FILESYSTEM_PATH_OUTSIDE_ROOT` | 허용된 다운로드 폴더 밖의 작업은 거부됩니다 | 아니오 | 경로 검토 |
| `IMAGE_ENCODE_FAILED` | 검증된 이미지를 WebP로 저장하지 못했습니다 | 아니오 | 원본·decoder 검토 |
| `ARTIFACT_MANIFEST_INVALID` | manifest와 파일·DB 기록이 일치하지 않습니다 | 아니오 | 무결성 검사 |
| `ARTIFACT_HASH_MISMATCH` | 파일의 SHA-256이 검증 기록과 다릅니다 | 아니오 | 무결성 검사·재다운로드 |
| `DOWNLOAD_WORKER_UNAVAILABLE` | 다운로드 작업기를 시작할 수 없습니다 | 예 | 앱 상태 확인 후 재시도 |
| `REQUEST_CANCELLED` | 요청이 취소되었습니다 | 아니오 | 필요하면 재시도 |
| `QUARANTINE_CONFLICT` | 원본과 격리 위치가 모호해 파일을 변경하지 않았습니다 | 아니오 | 두 위치를 사용자 검토 |
| `DATABASE_BUSY` | 내부 기록이 사용 중입니다 | 예 | 짧은 재시도 |
| `DATABASE_CORRUPT` | 내부 데이터 복구가 필요합니다 | 아니오 | backup/repair flow |
| `DATABASE_SCHEMA_NEWER` | 더 새로운 Atsumi Next가 만든 데이터입니다 | 아니오 | 데이터 변경 없이 최신 앱 사용 |
| `DATABASE_BACKUP_FAILED` | 안전 백업을 만들 수 없어 업데이트를 중단했습니다 | 아니오 | 저장 공간·권한 확인 후 재시도 |
| `IDEMPOTENCY_CONFLICT` | 같은 요청 식별자가 다른 대상에 이미 사용되었습니다 | 아니오 | 호출 내용 검토 |
| `DOWNLOAD_ENTRY_NOT_FOUND` | 다운로드 항목을 다시 불러오세요 | 아니오 | 목록 새로고침 |
| `INVALID_DOWNLOAD_STATE` | 현재 상태에서는 요청한 작업을 수행할 수 없습니다 | 아니오 | 최신 상태 검토 |
| `AUTO_FIND_NOT_RUNNING` | 취소할 Auto Find 갱신이 없습니다 | 아니오 | 최신 snapshot 확인 |
| `DUPLICATE_SCAN_NOT_RUNNING` | 취소할 작품 중복 검사가 없습니다 | 아니오 | 최신 snapshot 확인 |
| `DUPLICATE_CANDIDATE_NOT_FOUND` | 중복 후보가 더 이상 존재하지 않습니다 | 아니오 | 후보 목록 다시 로드 |
| `INTERNAL_DUPLICATE_SCAN_NOT_RUNNING` | 취소할 내부 중복 검사가 없습니다 | 아니오 | 최신 snapshot 확인 |
| `INTERNAL_DUPLICATE_ENTRY_NOT_FOUND` | 내부 중복 검토 대상 다운로드가 없습니다 | 아니오 | 완료 다운로드 하나 선택 후 다시 검사 |
| `INTERNAL_REMOVAL_PLAN_INVALID` | 격리 계획이나 파일 상태가 최신 snapshot과 다릅니다 | 아니오 | Review·무결성 검사 후 계획 다시 생성 |
| `JOB_INTERRUPTED` | 작업이 중단되었습니다 | 예 | 이어받기 |
| `THUMBNAIL_REQUEST_INVALID` | 미리보기 요청 정보가 올바르지 않습니다 | 아니오 | 요청 key 확인 |
| `THUMBNAIL_COORDINATOR_CLOSED` | 미리보기 작업기가 종료되었습니다 | 예 | 앱 상태 확인 후 재시도 |
| `THUMBNAIL_WORKER_UNAVAILABLE` | 미리보기 작업기를 시작하지 못했습니다 | 예 | 로그 확인 후 재시도 |
| `INTEGRITY_INCOMPLETE` | 일부 페이지가 없습니다 | 예 | 누락 페이지 plan |
| `DUPLICATE_EXACT` | 동일한 이미지가 발견되었습니다 | 아니오 | Review |
| `DUPLICATE_VISUAL` | 유사한 작품을 확인해야 합니다 | 아니오 | Review |
| `REVISION_CONFLICT` | 판정 대상이 이후 변경되었습니다 | 아니오 | 최신 상태 다시 로드 |

## 로그 필드

모든 job 오류 로그에는 가능한 범위에서 다음을 포함한다.

```text
timestamp, operation_id, job_id, gallery_id, page_number,
stage, error_code, attempt, retry_delay_ms, host, http_status,
elapsed_ms, app_version, schema_version
```

URL query, cookie, session token, 로컬 사용자 이름은 기본 로그에서 제거한다.

Thumbnail event는 같은 원인을 camelCase code(`candidatesExhausted`, `responseInvalid`, `decodeFailed`)와 명시적 `retryable`로 전달한다. WebView는 이 값을 보존해 사용자 문구와 재시도를 결정하며 raw source 오류 문자열을 분기 조건으로 사용하지 않는다.

## Auto Find run 오류

아래 code는 command의 `ApiError`가 아니라 영속 `AutoFindRun.errorCode`다. UI는 `state`, `revision`, `errorCode`, `errorMessage`를 함께 표시하고 최신 상태는 `auto_find_snapshot`으로 다시 읽는다.

| Run error code | 발생 조건 | 사용자 행동 |
|---|---|---|
| `AUTO_FIND_SOURCE_FAILED` | 명시적 갱신 중 source 조회 실패 | 연결·source 상태 확인 후 다시 갱신 |
| `AUTO_FIND_WORKER_UNAVAILABLE` | background worker 시작 실패 | 앱 상태 확인 후 다시 갱신 |
| `AUTO_FIND_INTERRUPTED` | 이전 프로세스가 `running` 상태로 종료됨 | 보존된 부분 결과 확인 후 다시 갱신 |
| `AUTO_FIND_CANCELLED` | 사용자가 갱신을 취소함 | 필요할 때 다시 갱신 |
| `AUTO_FIND_APP_EXIT` | 앱 종료가 진행 중 갱신을 취소함 | 다음 실행에서 다시 갱신 |

worker 시작 실패 command는 현재 공통 `DATABASE_ERROR` envelope도 함께 반환할 수 있으므로, frontend는 반환 오류와 복원된 run snapshot 중 어느 하나도 성공으로 오인하지 않는다. source의 raw URL·검색어·transport detail은 `AutoFindRun.errorMessage`에 넣지 않는다.

## 작품 중복 scan 오류

아래 code는 영속 `DuplicateScanRun.errorCode`다. 최신 상태는 `duplicate_snapshot`으로 복원하며 이미 저장된 candidate·decision을 오류 때문에 삭제하지 않는다.

| Run error code | 발생 조건 | 사용자 행동 |
|---|---|---|
| `DUPLICATE_WORKER_UNAVAILABLE` | background worker 시작 실패 | 앱 상태 확인 후 다시 검사 |
| `DUPLICATE_SCAN_FAILED` | verified artifact가 검사 중 변경·손상되거나 repository 작업 실패 | Downloads 무결성 검사 후 다시 검사 |
| `DUPLICATE_SCAN_INTERRUPTED` | 이전 프로세스가 `running` 상태로 종료됨 | 보존된 후보 확인 후 다시 검사 |
| `DUPLICATE_SCAN_CANCELLED` | 사용자가 검사를 취소함 | 필요할 때 다시 검사 |
| `DUPLICATE_SCAN_APP_EXIT` | 앱 종료가 진행 중 검사를 취소함 | 다음 실행에서 다시 검사 |

오류 문구에는 download root, 파일명, session, raw source URL을 넣지 않는다. 검증 파일의 구체적 불일치는 기존 artifact reconcile에서 안정 code로 확인한다.

## 내부 페이지 중복 scan 오류

| Run error code | 발생 조건 | 사용자 행동 |
|---|---|---|
| `INTERNAL_SCAN_WORKER_UNAVAILABLE` | background worker 시작 실패 | 앱 상태 확인 후 다시 검사 |
| `INTERNAL_SCAN_FAILED` | verified page가 검사 중 변경·손상되거나 repository 작업 실패 | Downloads 무결성 검사 후 다시 검사 |
| `INTERNAL_SCAN_INTERRUPTED` | 이전 프로세스가 `running` 상태로 종료됨 | 보존된 group 확인 후 다시 검사 |
| `INTERNAL_SCAN_CANCELLED` | 사용자가 검사를 취소함 | 필요할 때 다시 검사 |
| `INTERNAL_SCAN_APP_EXIT` | 앱 종료가 진행 중 검사를 취소함 | 다음 실행에서 다시 검사 |

page quarantine에서 원본·격리 경로가 모두 있거나 모두 없으면 `INTERNAL_REMOVAL_PLAN_INVALID`와 Review action으로 중단한다. 경로·파일명을 WebView 오류나 기본 로그에 노출하지 않으며 자동 overwrite/delete하지 않는다.
