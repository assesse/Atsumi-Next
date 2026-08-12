# API Contract V2 초안

## 공통 규칙

- command 이름은 `domain_action` 형식을 사용한다.
- payload와 result는 명시적 Rust struct와 TypeScript type을 공유한다.
- gallery ID는 number가 아니라 validation된 `GalleryId`로 domain에 들어간다.
- 긴 작업은 즉시 `jobId`를 반환한다.
- UI는 오류 문자열을 parsing하지 않는다.
- 같은 idempotency key의 queue command는 결과를 중복 생성하지 않는다.

## 공통 envelope

```ts
type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: ApiError };

type ApiError = {
  code: string;
  message: string;
  retryable: boolean;
  action?: "retry" | "review" | "reconnect" | "reveal" | "none";
  details?: Record<string, unknown>;
};
```

## Phase 3 필수 command

| Command | Payload | Result | 멱등성 |
|---|---|---|---|
| `settings_get` | 없음 | `SettingsSnapshot` | 예 |
| `settings_update` | `{ patch, expectedRevision }` | `SettingsSnapshot` | revision 기반 |
| `search_submit` | `SearchRequest` | `{ queryId, firstPage }` | query key 기반 |
| `search_page_get` | `{ queryId, page }` | `GalleryPage` | 예 |
| `gallery_detail_get` | `{ galleryId }` | `GalleryDetail` | 예 |
| `download_queue_add` | `{ galleries, requestId }` | `DownloadEntry[]` | requestId 기반 |
| `download_entries_list` | `DownloadListRequest` | `DownloadPage` | 예 |
| `download_retry` | `{ entryIds }` | `JobRef[]` | 현재 active job 재사용 |
| `download_cancel` | `{ entryIds }` | `DownloadEntry[]` | 예 |
| `download_remove` | `{ entryIds, mode }` | `RemovalPlan` 또는 `JobRef` | plan 승인 기반 |
| `artifact_open_first` | `{ entryId }` | `OpenResult` | 예 |
| `activity_list` | filter | `ActivityItem[]` | 예 |
| `app_reconcile` | `{ scope }` | `JobRef` | active job 재사용 |

## Event

| Event | 내용 |
|---|---|
| `job.changed` | job state, progress와 revision |
| `download.changed` | download entry projection의 부분 변경 |
| `thumbnail.ready` | gallery/page의 cache key와 상태 |
| `activity.changed` | 전역 작업 요약 변경 |
| `settings.changed` | 다른 window에서 바뀐 설정 snapshot |

이벤트가 유실돼도 `list/get` command로 현재 상태를 다시 구성할 수 있어야 한다.

## SearchRequest

```ts
type SearchRequest = {
  text: string;
  includeTags: string[];
  excludeTags: string[];
  languages: Array<"korean" | "japanese" | "chinese" | "english">;
  sort: "recent" | "popular_today" | "popular_week" | "popular_month" | "popular_year" | "random";
  pageSize: number;
};
```

backend는 최종 serialized query와 각 clause가 server/client 중 어디에 적용됐는지 diagnostic에 남긴다.

## DownloadEntry 상태

```ts
type DownloadState =
  | "queued"
  | "resolving_metadata"
  | "downloading"
  | "hashing"
  | "verifying"
  | "retry_wait"
  | "review_required"
  | "interrupted"
  | "failed"
  | "completed"
  | "quarantined";
```

`review_required`는 `reviewKind`와 `reviewId`를 가진다. 오류 문자열은 Review target을 결정하지 않는다.

## 후속 계약

Phase 5 이전에 다음을 별도 확정한다.

- duplicate candidate와 evidence
- duplicate decision plan/apply
- E-Hentai relation
- full scan progress와 cancellation
- internal scene block과 page selection plan/apply
