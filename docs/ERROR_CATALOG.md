# 오류 분류

## 원칙

- 사용자 문구, 개발 로그와 프로그램 분기용 code를 분리한다.
- retry 가능 여부는 호출부가 문자열로 추측하지 않는다.
- 실패한 URL, HTTP status, attempt는 detail에만 둔다.
- 같은 근본 원인은 화면마다 같은 code를 사용한다.

| Code | 사용자 문구 | Retry | 기본 행동 |
|---|---|---:|---|
| `NETWORK_OFFLINE` | 네트워크 연결을 확인하세요 | 예 | 연결 후 재시도 |
| `SOURCE_TIMEOUT` | 원본 사이트 응답이 늦습니다 | 예 | backoff 재시도 |
| `SOURCE_RATE_LIMITED` | 잠시 후 다시 시도합니다 | 예 | host cooldown |
| `SOURCE_NOT_FOUND` | 갤러리 정보를 찾을 수 없습니다 | 아니오 | 상세 확인 |
| `IMAGE_CANDIDATES_EXHAUSTED` | 이미지 서버에서 파일을 찾지 못했습니다 | 조건부 | URL 진단 또는 재조회 |
| `IMAGE_RESPONSE_INVALID` | 이미지가 아닌 응답을 받았습니다 | 예 | 후보 변경 |
| `IMAGE_DECODE_FAILED` | 이미지 처리에 실패했습니다 | 조건부 | 원본 보존, 상세 확인 |
| `FILESYSTEM_PERMISSION` | 폴더에 파일을 쓸 수 없습니다 | 아니오 | 설정/경로 열기 |
| `FILESYSTEM_MISSING` | 다운로드 폴더를 찾을 수 없습니다 | 아니오 | 재연결 또는 재다운로드 |
| `FILESYSTEM_CONFLICT` | 같은 위치에 다른 파일이 있습니다 | 아니오 | 사용자 검토 |
| `DATABASE_BUSY` | 내부 기록이 사용 중입니다 | 예 | 짧은 재시도 |
| `DATABASE_CORRUPT` | 내부 데이터 복구가 필요합니다 | 아니오 | backup/repair flow |
| `DATABASE_SCHEMA_NEWER` | 더 새로운 Atsumi Next가 만든 데이터입니다 | 아니오 | 데이터 변경 없이 최신 앱 사용 |
| `DATABASE_BACKUP_FAILED` | 안전 백업을 만들 수 없어 업데이트를 중단했습니다 | 아니오 | 저장 공간·권한 확인 후 재시도 |
| `IDEMPOTENCY_CONFLICT` | 같은 요청 식별자가 다른 대상에 이미 사용되었습니다 | 아니오 | 호출 내용 검토 |
| `DOWNLOAD_ENTRY_NOT_FOUND` | 다운로드 항목을 다시 불러오세요 | 아니오 | 목록 새로고침 |
| `INVALID_DOWNLOAD_STATE` | 현재 상태에서는 요청한 작업을 수행할 수 없습니다 | 아니오 | 최신 상태 검토 |
| `JOB_INTERRUPTED` | 작업이 중단되었습니다 | 예 | 이어받기 |
| `THUMBNAIL_REQUEST_INVALID` | 미리보기 요청 정보가 올바르지 않습니다 | 아니오 | 요청 key 확인 |
| `THUMBNAIL_COORDINATOR_CLOSED` | 미리보기 작업기가 종료되었습니다 | 예 | 앱 상태 확인 후 재시도 |
| `THUMBNAIL_WORKER_UNAVAILABLE` | 미리보기 작업기를 시작하지 못했습니다 | 예 | 로그 확인 후 재시도 |
| `INTEGRITY_INCOMPLETE` | 일부 페이지가 없습니다 | 예 | 누락 페이지 plan |
| `DUPLICATE_EXACT` | 동일한 이미지가 발견되었습니다 | 아니오 | Review |
| `DUPLICATE_VISUAL` | 유사한 작품을 확인해야 합니다 | 아니오 | Review |
| `REVIEW_DECISION_CONFLICT` | 판정 대상이 이후 변경되었습니다 | 아니오 | 최신 상태 다시 로드 |
| `PATH_OUTSIDE_ROOT` | 허용된 폴더 밖의 작업은 실행할 수 없습니다 | 아니오 | 작업 거부 |

## 로그 필드

모든 job 오류 로그에는 가능한 범위에서 다음을 포함한다.

```text
timestamp, operation_id, job_id, gallery_id, page_number,
stage, error_code, attempt, retry_delay_ms, host, http_status,
elapsed_ms, app_version, schema_version
```

URL query, cookie, session token, 로컬 사용자 이름은 기본 로그에서 제거한다.
