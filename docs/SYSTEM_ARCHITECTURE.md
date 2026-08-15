# 시스템 구조

## 확정 스택

- Desktop shell: Tauri 2
- Core: Rust
- Frontend: TypeScript + React
- Persistence: SQLite
- HTTP: pooled blocking client와 전역 scheduler (`spawn_blocking` application boundary)
- Image processing: worker task와 versioned hash profile

이 스택은 D-101~D-103과 ADR-0002로 승인됐다. SQLite가 canonical source이며 frontend memory state는 snapshot과 event의 projection만 가진다.

## 경계

```text
UI components
  -> typed frontend client
    -> Tauri commands and events
      -> application use cases
        -> domain model
          -> repository and external service interfaces
            -> SQLite / filesystem / Hitomi / E-Hentai / shell
```

### UI

- 화면 렌더링, 입력과 접근성
- local draft와 현재 selection
- backend snapshot과 event의 표시
- 검색식이나 중복 판정 규칙을 직접 구현하지 않음

### Interface

- Tauri command payload validation
- stable error envelope
- task event serialization
- command 내부에서 파일과 HTTP 로직을 직접 수행하지 않음

### Application

- SearchGalleries
- RefreshFavoriteArtists
- QueueDownloads
- ResumeInterruptedJobs
- VerifyDownload
- ReviewGalleryDuplicate
- ReviewInternalDuplicates
- ApplyDuplicateDecision
- RemoveDownload

### Domain

- Gallery와 GalleryMetadata
- DownloadJob과 JobStep
- DownloadArtifact와 PageArtifact
- DuplicateCandidate와 Evidence
- DuplicateDecision
- InternalSceneBlock와 PageSelection
- IntegrityResult
- Settings

### Infrastructure

- Hitomi client와 URL resolver
- E-Hentai relation provider
- SQLite repository
- filesystem artifact store
- thumbnail cache
- hash engine
- Windows shell adapter
- 향후 external sidecar adapter

## 영속 작업 상태 머신

```text
queued
  -> resolving_metadata
  -> downloading
  -> hashing
  -> verifying
  -> completed

각 단계 -> retry_wait -> 같은 단계 재시도
각 단계 -> review_required
각 단계 -> failed
실행 중 종료 -> interrupted -> resume 또는 cancel
허용된 미완료 상태 -> cancelled -> retry
```

작업 상태는 메모리 Map이 아니라 SQLite에 기록한다. 이벤트는 상태의 원본이 아니라 UI 갱신 신호다.

## command 설계 원칙

Classic command 이름을 새 내부 구조에 그대로 강제하지 않는다. 호환 facade가 필요하면 별도 adapter로 둔다.

모든 command 결과:

```ts
type ApiResult<T> =
  | { ok: true; data: T }
  | {
      ok: false;
      error: {
        code: string;
        message: string;
        retryable: boolean;
        details?: Record<string, unknown>;
      };
    };
```

긴 작업은 `jobId`를 즉시 반환하고 event stream으로 상태를 보낸다.

```ts
type JobEvent = {
  jobId: string;
  galleryId?: number;
  revision: number;
  state: string;
  completedUnits?: number;
  totalUnits?: number;
  message?: string;
};
```

`revision`으로 오래된 이벤트가 최신 상태를 덮지 못하게 한다.

## 데이터 소유권

- SQLite: 설정, Gallery snapshot, 다운로드, job, page, 판정, 제외, 즐겨찾기의 canonical source
- 실제 폴더: 다운로드 artifact
- 폴더 manifest: 이식성과 복구를 위한 파생 metadata
- thumbnail cache: 언제든 재생성 가능한 cache
- 프론트 store: backend snapshot의 projection과 임시 UI 상태
- localStorage: 사용하지 않음

## 공용 HTTP scheduler

- 전역 이미지 요청 예산과 갤러리 worker 수를 분리한다.
- host별 동시성, 전역 요청 시작 간격, cooldown을 독립 설정한다.
- 2026-08-04 Classic 실측의 `동시 5, 최소 25ms`를 초기 profile로 가져온다.
- connection pool 구조로 바뀌면 별도 probe로 다시 측정한다.
- 검색, thumbnail, 다운로드는 하나의 pooled transport와 전역/host별 permit을 공유하고 `critical > visible > download > prefetch` 순서로 dispatch한다.
- 429는 `Retry-After` 또는 기본 cooldown, 503·timeout은 bounded exponential backoff와 stable jitter를 적용한다. 404와 계약 오류는 반복 재시도하지 않는다.
- 대기·backoff·body read는 cancellation token을 확인하며, 취소 뒤 도착한 결과는 cache에 넣지 않는다.
- telemetry는 host, attempt, elapsed와 분류 code만 기록하고 URL query·cookie·검색어는 기록하지 않는다.

## Thumbnail coordinator

- 앱 프로세스에는 탭별 worker가 아니라 `ThumbnailCoordinator` 하나만 둔다.
- Explore, Downloads, Detail, Review는 `GalleryCover(galleryId)` 또는 `GalleryPage(galleryId, sourcePage)` key만 요청한다.
- coordinator가 `critical > visible > prefetch` queue, 동시성, 요청 시작 간격, in-flight 병합, 성공 cache와 짧은 실패 cache를 소유한다.
- 각 UI 구독은 고유 requestId를 가진다. 마지막 구독 취소 시 resolver cancellation token을 중단하고 늦은 결과를 cache에 넣지 않는다.
- worker 완료는 하나의 process-wide completion channel을 거쳐 `thumbnail:ready`로 전달한다. 카드 수만큼 대기 thread/task를 만들지 않는다.
- WebView는 전달된 byte payload를 짧게 유지되는 Blob URL로 표시하고 마지막 frontend 구독에서 해제한다. 실제 원본 URL과 cache path는 backend 경계 밖으로 노출하지 않는다.
- 카드 preview는 `IntersectionObserver`의 near-viewport 경계 안에서만 구독하고, 경계를 벗어나면 frontend 구독과 Blob URL을 해제한다. Detail/Review의 현재 작업은 `critical`, 화면 안 카드는 `visible`, 나머지는 `prefetch`로 분류한다.
- retryable failure는 짧은 negative-cache TTL 뒤 한정 재시도하고, permanent failure는 더 긴 negative cache로 반복 원격 호출을 막는다. WebView decode 실패는 해당 key cache를 무효화한 뒤 한 번만 재해석한다.
- production Tauri는 `HitomiLiveAdapter` 하나를 `SearchRepository`와 `ThumbnailResolver` 양쪽에 공유 주입한다. 브라우저 review mode와 단위 테스트만 fixture resolver를 사용한다.
- resolver는 HTTPS allowlist·redirect 재검증·응답 크기·MIME/signature·decode dimension/allocation을 검사하고 WebP 후보를 순서대로 시도한다. disk cache는 artifact pipeline에서 versioned cache로 추가한다.

## 해시와 중복

- `HashProfile`에 알고리즘, 크기, 전처리, threshold, 버전을 기록한다.
- exact SHA-256과 perceptual evidence를 분리한다.
- 작품 후보 생성은 보수적으로 하고 다운로드 후 강한 근거로 보강한다.
- 중복 판정은 boolean 하나가 아니라 evidence 목록이다.
- 사용자의 숨김, 연작, 오탐 결정은 append-only decision history로 남긴다.
- decision 적용은 파일, job, 해시와 후보 index를 한 use case에서 정리한다.

## 삭제와 복구

- 삭제는 먼저 quarantine으로 이동한다.
- DB에는 원래 경로, 격리 경로, 이유와 시각을 기록한다.
- UI는 undo와 사용자가 직접 실행하는 quarantine 비우기를 제공한다.
- 자동 만료·자동 영구 삭제는 하지 않는다.
- 다운로드 root 밖으로 해석되는 경로는 거부한다.

## 테스트 경계

- Domain unit test: 네트워크와 파일 없이 상태 전이와 판정 검증
- Contract test: command payload와 error code snapshot
- Repository test: 임시 SQLite migration과 transaction
- HTTP fixture test: 저장된 Hitomi 응답으로 검색과 URL resolver 검증
- Filesystem test: 임시 폴더로 resume, manifest, quarantine 검증
- Golden duplicate test: 실제 gallery fixture의 기대 pair와 오탐 금지
- UI component test: 카드, selection, progress, error state
- E2E: 검색부터 다운로드와 재시작 복구까지

## 금지사항

- `serde_json::Value`를 domain API의 기본 모델로 사용하지 않는다.
- 오류 문자열 prefix를 상태 판정 API로 사용하지 않는다.
- UI에서 다운로드 완료 여부를 추정하지 않는다.
- command 안에서 서로 무관한 DB, 파일, UI 상태를 임의로 따로 갱신하지 않는다.
- 자동 중복 판정만으로 사용자 파일을 영구 삭제하지 않는다.
