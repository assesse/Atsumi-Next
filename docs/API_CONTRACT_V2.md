# API Contract V2

Phase 5의 실제 artifact 기반 작품 중복 판정과 Review까지 구현된 command와 event 형식을 이 문서의 현재 기준 revision으로 사용한다. 내부 페이지 중복과 Classic import command는 후속 계약으로 구분한다.

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

## 현재 구현 command

| Command | Payload | Result | 멱등성 |
|---|---|---|---|
| `settings_get` | 없음 | `SettingsSnapshot` | 예 |
| `settings_update` | `{ patch, expectedRevision }` | `SettingsSnapshot` | revision 기반 |
| `search_submit` | `SearchRequest` | `{ queryId, firstPage }` | query key 기반 |
| `search_page_get` | `{ queryId, page }` | `GalleryPage` | 예 |
| `gallery_detail_get` | `{ galleryId }` | `GalleryDetail` | 예 |
| `favorites_list` | 없음 | `FavoriteRecord[]` | 예 |
| `favorite_set` | `{ key, enabled }` | `FavoriteMutationResult` | 상태 기준; enabled 반복 시 revision 증가 |
| `search_history_list` | `{ limit }` | `SearchHistoryEntry[]` | 예 |
| `auto_find_snapshot` | 없음 | `AutoFindSnapshot` | 예 |
| `auto_find_refresh` | 없음 | `AutoFindRun` | 실행 중인 run은 재사용, 완료 뒤에는 새 run |
| `auto_find_cancel` | 없음 | `AutoFindRun` | 실행 중 run에 한 번 적용 |
| `auto_find_exclude` | `{ galleryIds, reason }` | `AutoFindExclusionResult` | gallery ID 기준 upsert |
| `duplicate_snapshot` | 없음 | `DuplicateSnapshot` | 예 |
| `duplicate_scan_start` | 없음 | `DuplicateScanRun` | 실행 중인 run 재사용 |
| `duplicate_scan_cancel` | 없음 | `DuplicateScanRun` | 실행 중 run에 한 번 적용 |
| `duplicate_review_get` | `{ candidateId }` | `DuplicateReview` | 예 |
| `duplicate_decision_apply` | `{ request: DuplicateDecisionRequest }` | `DuplicateReview` | candidate revision CAS |
| `download_queue_add` | `{ galleries: GalleryId[], requestId }` | `DownloadEntry[]` | requestId + active gallery 기반 |
| `download_entries_list` | `DownloadListRequest` | `DownloadPage` | 예 |
| `download_retry` | `{ entryIds }` | `JobRef[]` | 현재 active job 재사용 |
| `download_cancel` | `{ entryIds }` | `DownloadEntry[]` | 예 |
| `download_quarantine` | `{ entryIds, reason }` | `DownloadEntry[]` | active quarantine record로 중복 방지 |
| `download_quarantine_undo` | `{ entryIds }` | `DownloadEntry[]` | active quarantine record 기반 |
| `thumbnail_request` | `ThumbnailRequest` | `ThumbnailRequestToken` | 같은 key의 in-flight 작업 병합 |
| `thumbnail_cancel` | `{ requestId }` | `boolean` | 예 |
| `thumbnail_invalidate` | `{ key }` | cache removal flags | 예 |
| `thumbnail_reprioritize` | `{ requestId, priority }` | `boolean` | 우선순위 승격만 적용 |
| `thumbnail_stats` | 없음 | `ThumbnailWorkerStats` | 예 |
| `artifact_open_first` | `{ entryId }` | `null` | 검증 snapshot 기반 |
| `app_reconcile` | 없음 | `ReconcileReport` | pending saga와 interrupted job 재사용 |

## Event

| Event | 내용 |
|---|---|
| `job:changed` | job state, progress와 revision |
| `download:changed` | download entry projection의 부분 변경 |
| `thumbnail:ready` | requestId, gallery/page key, delivery 또는 typed failure |
| `settings:changed` | 다른 window에서 바뀐 설정 snapshot |
| `auto-find:changed` | Auto Find run state, progress, candidate count와 revision |
| `duplicate:changed` | 작품 중복 scan state, hash/pair progress, candidate count와 revision |

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

## Gallery summary·detail metadata

```ts
type GallerySummary = {
  id: GalleryId;
  title: string;
  artist: string;
  group?: string;
  series: string[];
  characters: string[];
  pages: number;
  language: "korean" | "japanese" | "chinese" | "english";
  tags: string[];
  publishedRank: number;
  popularity: number;
  thumbnailKey?: string;
  thumbnailWidth: number;
  thumbnailHeight: number;
};

type GalleryDetail = GallerySummary & {
  related: GallerySummary[];
};
```

`series`와 `characters`는 검색·상세·Related·Auto Find restore에서 항상 존재하는 배열이며 값이 없으면 `[]`다. metadata 검색은 공백을 underscore로 바꾼 `series:rain_archives`, `character:mira_lane` 같은 token을 사용한다. production serializer는 이를 각각 Hitomi `n/series/*-all.nozomi`, `n/character/*-all.nozomi` namespace endpoint로 변환하고 값은 정규화·percent-encode한다.

## 즐겨찾기·검색 이력·Auto Find 계약

```ts
type FavoriteNamespace = "artist" | "group" | "series" | "character" | "tag";

type FavoriteKey = {
  namespace: FavoriteNamespace;
  value: string;
};

type FavoriteRecord = FavoriteKey & {
  revision: number;
  createdAt: string;
  updatedAt: string;
};

type FavoriteMutationResult = {
  enabled: boolean;
  favorite?: FavoriteRecord;
};

type SearchHistoryEntry = {
  historyId: number;
  text: string;
  includeTags: string[];
  excludeTags: string[];
  languages: Array<"korean" | "japanese" | "chinese" | "english">;
  sort: SearchRequest["sort"];
  pageSize: number;
  useCount: number;
  lastUsedAt: string;
};

type AutoFindRunState = "running" | "completed" | "failed" | "cancelled";

type AutoFindRun = {
  runId: string;
  revision: number;
  state: AutoFindRunState;
  totalFavorites: number;
  completedFavorites: number;
  candidatesFound: number;
  startedAt: string;
  updatedAt: string;
  finishedAt?: string;
  errorCode?: string;
  errorMessage?: string;
};

type AutoFindCandidate = GallerySummary & {
  runId: string;
  matchedFavorite: FavoriteKey;
  discoveredAt: string;
};

type AutoFindSnapshot = {
  run?: AutoFindRun;
  candidates: AutoFindCandidate[];
};

type AutoFindExclusionResult = {
  excludedGalleryIds: GalleryId[];
  snapshot: AutoFindSnapshot;
};
```

- `FavoriteKey.value`는 trim, 소문자화와 연속 공백 정규화 뒤 1~200 bytes로 저장한다. 원격 검색 token을 만들 때 값의 공백은 underscore로 바꾼다. 모든 namespace는 카드·상세·Related와 검색 suggestion에 영속 반영하지만 현재 Auto Find 자동 갱신 source는 `artist` 즐겨찾기만 사용한다.
- `search_submit`이 성공하고 text/include/exclude 중 하나 이상이 있을 때만 정규화된 전체 `SearchRequest` fingerprint를 이력에 upsert한다. 앱 시작의 빈 Recent와 입력 중 draft는 기록하거나 원격 제출하지 않는다. 같은 요청은 `useCount`와 `lastUsedAt`을 갱신하며 `search_history_list.limit`은 1~100이다.
- `auto_find_refresh`는 사용자 명령으로만 시작한다. 각 작가를 `artist:{value}`, 전체 4개 언어, `recent`, page size 200으로 조회하고 source의 마지막 page까지 진행하되 한 작가당 최대 250 page로 제한한다. 실행 중 다시 호출하면 같은 run snapshot을 반환한다.
- 후보는 SQLite에 run별로 저장한다. 어떤 상태든 `download_entries`에 존재하는 gallery와 `auto_find_exclusions`에 존재하는 gallery는 추가·조회에서 제외한다. 명시적 제외는 최대 200개 양의 ID와 1~500 bytes 이유를 받는다.
- 진행 중 앱이 닫히면 run은 `cancelled/AUTO_FIND_APP_EXIT`, startup에서 남은 `running` run은 `failed/AUTO_FIND_INTERRUPTED`로 바꾼다. source 실패는 `failed/AUTO_FIND_SOURCE_FAILED`로 저장하며 사용자는 명시적 갱신을 다시 실행해 retry한다.
- `auto-find:changed`는 run projection 갱신 신호다. 후보마다 event를 만들지 않고 시작, 작가별 진행, 최종 상태에서만 보내며 UI는 event 뒤 snapshot을 다시 읽는다. 이벤트가 유실되거나 앱이 재시작되면 `auto_find_snapshot`으로 최신 run과 후보를 복원한다. 화면의 전체/작가 그룹, 결과 문자열 검색과 언어 filter는 영속 후보에 대한 local projection이며 키 입력마다 원격 요청하지 않는다.
- 현재 구현은 다운로드 이력, Auto Find 명시적 제외, 작품 숨김과 resolved duplicate decision·pair 제외 기록을 후보 조건에 함께 반영한다.

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
- single-instance를 획득한 앱 시작 시 위 active 상태로 남은 job과 entry는 한 transaction에서 `interrupted`로 전환한다. download root가 유효하면 DB·manifest·파일 reconcile 뒤 같은 entry/job의 새 attempt로 자동 resume하며, verified page checkpoint는 다시 받지 않는다.
- `download_entries_list`의 `query`는 UTF-8 기준 최대 500 bytes이며 현재 `entryId`와 `galleryId`에만 적용한다. 결과는 `galleryId`, `entryId` 오름차순으로 고정한다.

### Retry와 cancel

- `download_retry`는 `interrupted`, `failed`, `cancelled` 항목을 같은 entry/job의 다음 attempt로 전환한다. 이미 active이면 기존 job을 재사용한다.
- `download_cancel`은 허용된 state의 job과 entry를 한 transaction에서 `cancelled`로 전환한다. 이미 취소된 항목의 반복 요청은 revision을 올리지 않는다.
- attempt, 마지막 오류와 시작·종료 시각은 SQLite에 남기고 list/event projection으로 UI에 전달한다. 이미 실패/중단으로 종료된 attempt를 취소해도 원래 오류 증거는 보존한다. UI는 새 queue를 만들어 retry를 흉내 내지 않는다.

## Thumbnail coordinator 계약

```ts
type ThumbnailKey =
  | { kind: "galleryCover"; galleryId: GalleryId }
  | { kind: "galleryPage"; galleryId: GalleryId; sourcePage: number }
  | { kind: "artifactPage"; entryId: string; sourcePage: number };

type ThumbnailRequest = {
  key: ThumbnailKey;
  consumer: "explore" | "downloads" | "detail" | "review";
  priority: "critical" | "visible" | "prefetch";
};
```

- `sourcePage`는 UI index가 아니라 1부터 시작하는 원본 page number다. `artifactPage`는 작품 Review에서 검증된 local artifact만 읽으며 raw 경로를 반환하지 않는다.
- 동일 key의 동시 요청은 프로세스 전역에서 하나로 합친다. 마지막 구독자가 사라지면 queued/running resolver에 cancellation을 전달한다.
- 완료는 `thumbnail:ready` event로 전달한다. 메모리 cache hit에서는 event가 command 응답보다 먼저 올 수 있으므로 frontend transport는 requestId별 미매칭 event를 잠시 보관한다.
- WebView decode 실패는 `thumbnail_invalidate`로 해당 key의 success/negative cache를 비운 뒤 다시 해석할 수 있다.
- frontend는 원본 URL, retry, cache eviction을 직접 결정하지 않는다. Tauri는 실제 HTTP resolver를, 브라우저 검토 모드는 결정론적 fixture resolver를 같은 port 뒤에서 사용한다. thumbnail cache는 재생성 가능한 bounded memory cache이며 영속 파일은 download artifact 경계가 소유한다.
- production resolver는 검색·download와 같은 pooled transport를 공유한다. HTTP dispatch는 `critical > visible > prefetch > download`, 전역·host별 동시성, 최소 시작 간격, cancellation, bounded retry, `Retry-After`, 429/503 cooldown을 적용한다.
- thumbnail failure code는 `cancelled`, `notFound`, `candidatesExhausted`, `responseInvalid`, `decodeFailed`, `temporarilyUnavailable`, `unauthorized`, `invalidData`, `resolver`, `coordinatorClosed` 중 하나다. frontend는 backend가 전달한 `retryable`을 보존하고 문자열 prefix로 retry를 추측하지 않는다.

## Artifact·reconcile·quarantine 계약

- `completed`는 실제 WebP page 전부의 decode·byte length·SHA-256, source page mapping, schema 1 manifest와 DB snapshot이 일치한 뒤에만 기록한다.
- `app_reconcile`은 `{ inspectedArtifacts, verifiedArtifacts, resumedJobs, issues[] }`를 반환한다. 각 issue는 `entryId`, stable `code`, 사용자 문구와 `recoverable`을 가진다.
- quarantine은 `pending_quarantine -> quarantined`, undo는 `pending_restore -> restored` saga다. filesystem atomic move와 SQLite commit 사이에 종료되면 다음 reconcile이 원본/격리 경로 존재를 비교해 마무리한다.
- 둘 다 존재하거나 둘 다 없으면 자동 삭제·덮어쓰기를 하지 않고 `QUARANTINE_CONFLICT`를 반환한다. 자동 purge command는 없다.

## 작품 중복 계약

- `HashProfile` 1은 algorithm 1, detail dHash 1024 bits, pHash 64 bits, visual threshold 0.80과 low-information threshold를 고정한다. 기존 profile 결과를 새 버전으로 재해석하지 않는다.
- scan은 `completed` artifact의 present·verified·non-excluded page만 읽고, gallery마다 완료 시각/revision이 가장 최신인 artifact 하나를 결정론적으로 선택한다. title/artist/group/page count metadata로 작업 순서를 정하되 전수 pair fallback을 유지한다.
- 후보는 `exact | contains | partial | translation_visual`과 confidence, coverage, typed evidence 및 원본 source page pair를 가진다. one-to-one monotonic alignment이므로 한 page를 여러 상대 page에 재사용하지 않는다.
- `duplicate_decision_apply` action은 `hide_parent | hide_candidate | series_link | series_unlink | exclude_pair`다. hide와 pair 제외는 후보를 resolve하고 Auto Find에서도 제외한다. series link는 양쪽 gallery를 같은 group에 원자적으로 연결하되 후보를 자동 resolve하지 않는다.
- scan event는 신호일 뿐이며 후보·Review·판정 이력의 canonical source는 SQLite다. UI는 event 유실·재시작·revision 충돌 때 snapshot/get을 다시 읽는다.
- Review page preview는 `{ kind: "artifactPage", entryId, sourcePage }` key로 같은 전역 thumbnail coordinator를 사용한다. backend는 root 내부의 검증된 local WebP만 읽고 1024px 이하 preview로 전달한다.
- E-Hentai relation provider는 명시적으로 제공된 적법 session이 없으면 비활성이다. session·cookie를 SQLite, manifest, 로그에 저장하지 않는다.

## 후속 계약

Phase 6~7에서 `internal scene block + page selection/removal plan/apply/undo`와 Classic import dry-run/apply/rollback command를 추가한다.
