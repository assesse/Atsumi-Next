# API Contract V2

Phase 3A에서 구현된 command와 event 형식을 이 문서의 현재 기준 revision으로 사용한다. 아직 구현되지 않은 후속 command는 표에서 별도로 구분한다.

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
| `download_queue_add` | `{ galleries: GalleryId[], requestId }` | `DownloadEntry[]` | requestId + active gallery 기반 |
| `download_entries_list` | `DownloadListRequest` | `DownloadPage` | 예 |
| `download_retry` | `{ entryIds }` | `JobRef[]` | 현재 active job 재사용 |
| `download_cancel` | `{ entryIds }` | `DownloadEntry[]` | 예 |
| `thumbnail_request` | `ThumbnailRequest` | `ThumbnailRequestToken` | 같은 key의 in-flight 작업 병합 |
| `thumbnail_cancel` | `{ requestId }` | `boolean` | 예 |
| `thumbnail_invalidate` | `{ key }` | cache removal flags | 예 |
| `thumbnail_reprioritize` | `{ requestId, priority }` | `boolean` | 우선순위 승격만 적용 |
| `thumbnail_stats` | 없음 | `ThumbnailWorkerStats` | 예 |
| `download_remove` | `{ entryIds, mode }` | `RemovalPlan` 또는 `JobRef` | plan 승인 기반 |
| `artifact_open_first` | `{ entryId }` | `OpenResult` | 예 |
| `activity_list` | filter | `ActivityItem[]` | 예 |
| `app_reconcile` | `{ scope }` | `JobRef` | active job 재사용 |

## Event

| Event | 내용 |
|---|---|
| `job:changed` | job state, progress와 revision |
| `download:changed` | download entry projection의 부분 변경 |
| `thumbnail:ready` | requestId, gallery/page key, delivery 또는 typed failure |
| `activity.changed` | 전역 작업 요약 변경 |
| `settings:changed` | 다른 window에서 바뀐 설정 snapshot |

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
type DownloadEntry = {
  entryId: string;
  galleryId: GalleryId;
  revision: number;
  state: DownloadState;
  progress?: number;
  attempt?: number;
  errorCode?: string;
  errorMessage?: string;
  reviewKind?: "gallery_duplicate" | "internal_pages";
  reviewId?: string;
};

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
  | "quarantined"
  | "cancelled";
```

`review_required`는 `reviewKind`와 `reviewId`를 가진다. 오류 문자열은 Review target을 결정하지 않는다.

### Queue 멱등성과 조회

- `galleries`는 양의 `GalleryId` 1~200개이며 backend가 ID 오름차순으로 중복을 제거한다.
- 같은 `requestId`와 같은 정규화 ID 집합은 최초 queue 응답 snapshot을 그대로 재생하며 새 job을 만들지 않는다. 최신 상태는 `download_entries_list`로 재구성한다.
- 같은 `requestId`를 다른 ID 집합에 재사용하면 `IDEMPOTENCY_CONFLICT`를 반환한다.
- 새 `requestId`라도 같은 gallery가 `queued`, `resolving_metadata`, `downloading`, `hashing`, `verifying`, `retry_wait` 중 하나이면 기존 active entry를 재사용한다.
- 앱 시작 시 위 active 상태로 남은 job과 entry는 한 transaction에서 `interrupted`로 전환한다. 자동 resume은 하지 않으며 후속 `download_retry`/cancel 정책으로 처리한다.
- `download_entries_list`의 `query`는 UTF-8 기준 최대 500 bytes이며 현재 `entryId`와 `galleryId`에만 적용한다. 결과는 `galleryId`, `entryId` 오름차순으로 고정한다.

### Retry와 cancel

- `download_retry`는 `interrupted`, `failed`, `cancelled` 항목을 같은 entry/job의 다음 attempt로 전환한다. 이미 active이면 기존 job을 재사용한다.
- `download_cancel`은 허용된 state의 job과 entry를 한 transaction에서 `cancelled`로 전환한다. 이미 취소된 항목의 반복 요청은 revision을 올리지 않는다.
- attempt, 마지막 오류와 시작·종료 시각은 SQLite에 남기고 list/event projection으로 UI에 전달한다. 이미 실패/중단으로 종료된 attempt를 취소해도 원래 오류 증거는 보존한다. UI는 새 queue를 만들어 retry를 흉내 내지 않는다.

## Thumbnail coordinator 계약

```ts
type ThumbnailKey =
  | { kind: "galleryCover"; galleryId: GalleryId }
  | { kind: "galleryPage"; galleryId: GalleryId; sourcePage: number };

type ThumbnailRequest = {
  key: ThumbnailKey;
  consumer: "explore" | "downloads" | "detail" | "review";
  priority: "critical" | "visible" | "prefetch";
};
```

- `sourcePage`는 UI index가 아니라 1부터 시작하는 원본 page number다.
- 동일 key의 동시 요청은 프로세스 전역에서 하나로 합친다. 마지막 구독자가 사라지면 queued/running resolver에 cancellation을 전달한다.
- 완료는 `thumbnail:ready` event로 전달한다. 메모리 cache hit에서는 event가 command 응답보다 먼저 올 수 있으므로 frontend transport는 requestId별 미매칭 event를 잠시 보관한다.
- WebView decode 실패는 `thumbnail_invalidate`로 해당 key의 success/negative cache를 비운 뒤 다시 해석할 수 있다.
- frontend는 원본 URL, retry, cache eviction을 직접 결정하지 않는다. 현재는 결정론적 fixture resolver이며 실제 HTTP·disk resolver는 같은 port를 교체해 연결한다.

## 후속 계약

Phase 5 이전에 다음을 별도 확정한다.

- duplicate candidate와 evidence
- duplicate decision plan/apply
- E-Hentai relation
- full scan progress와 cancellation
- internal scene block과 page selection plan/apply
