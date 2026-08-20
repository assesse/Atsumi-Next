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
- SetFavorite
- ListSearchHistory
- RefreshFavoriteArtists
- CancelAutoFind
- ExcludeAutoFindCandidates
- QueueDownloads
- ResumeInterruptedJobs
- VerifyDownload
- ReviewGalleryDuplicate
- ReviewInternalDuplicates
- ApplyDuplicateDecision
- RemoveDownload

### Domain

- Gallery와 GalleryMetadata(`series[]`, `characters[]` 포함)
- FavoriteKey와 SearchHistoryEntry
- AutoFindRun과 AutoFindCandidate
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

- SQLite(schema v17): 설정, Gallery snapshot, 다운로드, immutable artifact 위치, job/page attempt 진단, 판정, 제외, 즐겨찾기, 검색 이력과 Auto Find run/후보/cutoff/truncation의 canonical source
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
- 검색, thumbnail, 다운로드는 하나의 pooled transport와 전역/host별 permit을 공유하고 `critical > visible > prefetch > download` 순서로 dispatch한다. 화면에 곧 보일 미리보기가 백그라운드 artifact 다운로드에 굶지 않는다.
- 429는 `Retry-After` 또는 기본 cooldown, 503·timeout은 bounded exponential backoff와 stable jitter를 적용한다. 404와 계약 오류는 반복 재시도하지 않는다.
- 대기·backoff·body read는 cancellation token을 확인하며, 취소 뒤 도착한 결과는 cache에 넣지 않는다.
- telemetry는 host, attempt, elapsed와 분류 code만 기록하고 URL query·cookie·검색어는 기록하지 않는다.

## Explore query session

- UI에는 동시에 하나의 `ExplorePageSession`만 둔다. query별 완료 page는 최대 5개이고 현재 page와 앞뒤 2개 창만 유지하며, 인접 page는 foreground 결과 뒤 낮은 우선순위로 prefetch한다. page별 scroll offset도 session projection에 저장한다.
- foreground와 prefetch가 같은 page를 요청하면 하나의 in-flight promise를 공유하고 prefetch를 foreground로 승격한다. 실패한 prefetch는 자동 반복하지 않으며 사용자의 foreground retry만 새 요청을 만든다.
- `search_page_get`은 query/page뿐 아니라 고유 requestId를 받는다. query 교체·reset·창 밖 eviction은 `search_page_cancel`로 backend active token을 실제 취소한다.
- cancel이 start보다 먼저 도착하는 race는 최대 256개의 bounded tombstone으로 흡수한다. 뒤늦은 start는 이미 cancelled token을 받고 source metadata loop를 진행하지 않으며, 늦은 completion도 새 query projection에 합쳐지지 않는다.

## 앨범 카드 projection

- Explore/Auto Find/Downloads는 점수·날짜를 제거한 가로 밀도형 카드를 공유한다. title/subtitle, artist/group, language, tags, page count와 gallery ID라는 정보 종류는 유지하되 화면별 상태 action과 선택 규칙만 projection한다.
- image는 source가 제공한 width/height 비율을 사용하고 preview width를 가용 content rect에 맞춘다. `ResizeObserver`, `document.fonts.ready`와 실제 chip width/height 측정으로 열 수·폰트·폭 변경 뒤 다시 배치한다.

## Thumbnail coordinator

- 앱 프로세스에는 탭별 worker가 아니라 `ThumbnailCoordinator` 하나만 둔다.
- Explore, Downloads, Detail은 `GalleryCover(galleryId)`/`GalleryPage(galleryId, sourcePage)`, gallery·internal Review는 root-bound `ArtifactPage(entryId, sourcePage)` key를 요청한다.
- coordinator가 `critical > visible > prefetch` queue, 동시성, 요청 시작 간격, in-flight 병합, 성공 cache와 짧은 실패 cache를 소유한다.
- 각 UI 구독은 고유 requestId를 가진다. 마지막 구독 취소 뒤 frontend는 400ms orphan grace를 두고 같은 key의 빠른 재구독을 공유한다. grace 뒤에도 구독이 없을 때 resolver cancellation token을 중단하고 늦은 결과를 cache에 넣지 않는다.
- worker 완료는 하나의 process-wide completion channel을 거쳐 `thumbnail:ready`로 전달한다. 카드 수만큼 대기 thread/task를 만들지 않는다.
- WebView는 전달된 byte payload를 Blob URL로 표시한다. 완료 asset은 마지막 구독 뒤 120초·최대 256개까지 frontend retention cache에 두며 최종 eviction에서만 revoke한다. 실제 원본 URL과 cache path는 backend 경계 밖으로 노출하지 않는다.
- 카드 preview는 `IntersectionObserver`의 near-viewport 경계 안에서만 구독하고, 경계를 벗어나면 frontend 구독과 Blob URL을 해제한다. Detail/Review의 현재 작업은 `critical`, 화면 안 카드는 `visible`, 나머지는 `prefetch`로 분류한다.
- backend success cache 기본값은 512 entries/64MiB/30분이다. retryable failure는 3초, permanent failure는 5분 negative-cache TTL로 반복 원격 호출을 막는다. WebView decode 실패는 해당 key cache를 무효화한 뒤 한 번만 재해석한다.
- production Tauri는 `HitomiLiveAdapter` 하나를 `SearchRepository`와 `ThumbnailResolver` 양쪽에 공유 주입한다. 브라우저 review mode와 단위 테스트만 fixture resolver를 사용한다.
- resolver는 HTTPS allowlist·redirect 재검증·응답 크기·MIME/signature·decode dimension/allocation을 검사하고 지원 후보를 순서대로 시도한다. thumbnail은 재생성 가능한 bounded memory cache이며, 영속 파일은 검증된 download artifact만 소유한다.

## 즐겨찾기·검색 이력·Auto Find

- 작가·그룹·시리즈·캐릭터·태그 즐겨찾기와 성공한 명시적 검색 이력은 SQLite가 소유한다. frontend set과 suggestion 목록은 backend snapshot의 projection이며 localStorage를 canonical source로 사용하지 않는다. 검색·상세·Related의 `GallerySummary`는 `series[]`와 `characters[]`를 항상 전달한다.
- 검색 입력은 local draft일 뿐이다. `search_submit`이 성공한 뒤 non-empty text/include/exclude가 있는 요청만 이력에 기록하며, 자동 Recent와 key 입력은 원격 요청이나 이력 쓰기를 만들지 않는다.
- `AutoFindSupervisor`는 프로세스에 하나만 두고 명시적 `auto_find_refresh`에서만 background worker를 시작한다. 동시에 하나의 run만 허용하며 실행 중 재요청은 기존 run을 재사용한다.
- 갱신 대상은 현재 `artist` 즐겨찾기다. 각 작가의 `artist:{value}` 검색은 production의 같은 `HitomiLiveAdapter`와 공용 HTTP scheduler를 사용한다. 별도 HTTP client나 thumbnail coordinator를 만들지 않는다.
- favorite 값은 사람이 읽는 정규화 공백으로 저장하고 source token을 만들 때 공백을 underscore로 바꾼다. `artist`, `group`, `series`, `character`, `tag` prefix는 명시적 Nozomi namespace로 직렬화되며 unknown prefix만 residual text filter로 남긴다.
- run, 진행률, 후보 metadata snapshot과 gallery 제외는 SQLite에 기록한다. schema v11 후보에는 series/characters JSON도 들어가며 이전 v10 후보는 `[]` 기본값으로 보존한다. `auto-find:changed`는 시작·작가별 진행·최종 상태에서 보내는 bounded UI 갱신 신호일 뿐이며, 앱 재시작이나 event 유실 뒤에는 `auto_find_snapshot`으로 최신 run과 후보를 복원한다.
- cancel token과 DB run state를 함께 확인해 취소 뒤 늦은 page를 저장하지 않는다. 정상 앱 종료는 active run을 `cancelled/AUTO_FIND_APP_EXIT`, 비정상 종료 뒤 startup recovery는 남은 run을 `failed/AUTO_FIND_INTERRUPTED`로 종결하고 부분 후보를 보존한다.
- 후보 insert와 snapshot은 모든 download entry, 명시적 Auto Find exclusion, 작품 숨김, resolved duplicate decision과 pair 제외를 제외한다. 이 판정은 frontend flag가 아니라 schema v12의 SQLite record를 조회한다.
- 전체/작가별 묶음, 결과 문자열 검색과 언어 filter는 이미 저장된 후보에 대한 frontend local projection이다. 이 조작은 source request를 만들지 않는다. 후보 일괄 다운로드는 기존 idempotent download queue use case를 재사용한다.
- run은 설정의 `include_all_history|newer_than_oldest_downloaded`를 snapshot한다. 후자는 complete/quarantined 상태의 실제 소유 artifact만 근거로 작가별 oldest gallery ID를 계산하고 `source=verified_owned_artifact`, `policyVersion=1`, qualified count를 저장한다. 증거가 없으면 임의 cutoff하지 않는다.
- production source는 언어별 Nozomi ID를 교집합·dedupe·내림차순 정렬한 뒤 cutoff를 metadata fetch 전에 적용한다. cutoff 뒤 candidate limit은 50,000개이고 초과는 `candidate_limit_after_cutoff` truncation으로 영속한다. 과거의 작가당 250-page 상한은 더 이상 현재 계약이 아니다.

## Download artifact pipeline

- queue commit 뒤 bounded gallery worker가 자동 시작하며, source metadata와 이미지 I/O 중에는 SQLite transaction을 잡지 않는다.
- 원본 `source_page_number`와 source revision을 immutable identity로 사용한다. source payload의 identity가 계획과 다르면 저장 전에 거부한다.
- 각 page는 bounded HTTP body read, `.part` 64KiB chunk write, flush/sync, decode/WebP 검증, SHA-256 후 atomic rename 순서를 따른다.
- 검증된 원본 WebP는 byte를 보존하고 JPEG/PNG/AVIF는 RGBA lossless WebP로 변환한다. AVIF는 pinned `avif-rust 0.0.6`/`bin-rs 0.0.10`과 dimension/allocation 제한을 사용하는 experimental 경로다. JPEG XL은 현재 decode하지 않고 후보 diagnostic 뒤 fallback을 계속하며 최종 실패는 non-retryable `IMAGE_FORMAT_UNSUPPORTED`다. alpha는 보존하며, 변환 입력의 animation은 첫 frame 정책이다. 이 정책과 writer/app version은 manifest에 기록한다.
- 새 artifact의 상대 폴더는 backend가 `folder_name_template`을 검증·sanitize해 최초 예약한다. `{id}`는 필수이고 기존 `relative_directory`와 최초 `root_snapshot`은 schema v15/v16 trigger로 immutable이다. 설정 변경은 이후 새 artifact에만 적용되며 자동 rename/move가 없다.
- manifest schema 1은 gallery snapshot, source page mapping, relative path, byte length, SHA-256, storage format, exclusion/quarantine, 완료 시각과 HashProfile version을 가진다.
- 모든 page 파일과 DB checkpoint를 다시 검증하고 manifest temp write/sync/atomic replace를 마친 뒤에만 DB artifact/job/entry를 `completed`로 바꾼다.
- startup/manual reconcile은 pending quarantine saga를 먼저 마무리하고, 완료 artifact의 파일·hash·manifest를 검사한 뒤 interrupted job을 verified checkpoint부터 재개한다.

## 해시와 중복

- `HashProfile`에 알고리즘, 크기, 전처리, threshold, 버전을 기록한다. 현재 profile 1/algorithm 1은 64-bit coarse dHash·pHash, 1024-bit detail dHash, luma 분산·non-uniform·edge gate를 사용한다.
- `DuplicateSupervisor`는 프로세스에 하나만 있고 검증 완료 local artifact만 읽는다. gallery별 최신 artifact를 선택하고 page SHA가 같은 profile cache와 일치할 때만 hash feature를 재사용한다.
- 제목·작가·그룹·page count metadata는 candidate worklist 우선순위를 정한다. recall을 metadata에 의존시키지 않도록 zero-affinity pair도 전수 fallback으로 비교한다.
- 최종 page evidence는 exact SHA와 perceptual match를 구분하고, 단조 1:1 gap-tolerant alignment로 포함·부분·번역·exact 관계를 판정한다. blank/저정보 page, 작은 장면 변화, 일부 공통 panel은 강한 후보를 만들지 않는다.
- scan·candidate·evidence·page pair·decision·series·숨김·pair 제외는 SQLite canonical state다. `duplicate:changed`는 진행 신호이고 UI는 snapshot/review command로 복원한다.
- 숨김, 양쪽 연작 연결/단일 member 해제, 오탐 pair 제외는 candidate revision CAS transaction과 append-only history로 적용한다. 자동 판정은 파일을 이동하거나 삭제하지 않는다.
- Review thumbnail은 기존 전역 coordinator의 `ArtifactPage(entryId, sourcePage)`를 사용한다. local root containment, byte length, SHA-256과 WebP decode를 재검증한 뒤 bounded preview만 WebView로 전달한다.
- E-Hentai relation은 optional port다. production 기본은 명시적 session이 없어 disabled provider이고, session/cookie를 DB·manifest·로그에 남기지 않는다.

### 내부 페이지 중복과 page quarantine

- `InternalDuplicateSupervisor`는 작품 중복과 별도의 단일 worker·run state를 가지며 verified complete artifact 안에서만 page를 비교한다. SHA/profile cache와 `ArtifactPage` thumbnail resolver는 공유하지만 gallery pair 후보와 내부 scene group은 별도 SQLite projection이다.
- exact SHA 반복은 한 synchronized row로 허용한다. perceptual match는 한 장짜리 shared panel 오탐을 차단하기 위해 원본 순서가 증가하는 최소 두 row, 각 방향 최대 2 missing-page gap을 통과해야 한다.
- Review는 block/sequence별 원본 source page를 나란히 표시하고 한 page를 keep으로 명시한다. plan은 group revision, remove set, 현재 byte 합계와 만료 시각을 저장하므로 UI가 파일 수나 경로를 추정하지 않는다.
- page quarantine은 DB intent를 먼저 commit한 뒤 artifact 내부 `.atsumi-page-quarantine/<plan-id>/`로 atomic move하고 manifest를 temp/sync/atomic replace한다. 마지막 transaction이 page state·artifact revision·group resolved·plan state를 함께 확정한다.
- undo와 시작 시 recovery는 원본/격리 경로 존재 조합을 확인한다. 한쪽만 있으면 의도한 상태로 수렴하고, 둘 다 있거나 둘 다 없으면 Review 대상으로 남기며 자동 삭제·덮어쓰지 않는다.
- 격리된 page는 original source page number와 verification metadata를 보존하고 `excluded=true`로 downloader/duplicate scan에서 제외한다. artifact 전체는 complete 상태를 유지할 수 있다.

## 삭제와 복구

- 삭제는 먼저 quarantine으로 이동한다.
- DB에는 원래 경로, 격리 경로, 이유와 시각을 기록한다.
- UI는 undo를 제공한다. 현재 구현에는 영구 purge command가 의도적으로 없다.
- 자동 만료·자동 영구 삭제는 하지 않는다.
- 다운로드 root 밖으로 해석되는 경로는 거부한다.

## Classic read-only import

- `FilesystemClassicSource`는 사용자가 고른 root를 canonicalize하고 symlink를 따라 inventory하지 않는다. bounded JSON/image read와 image dimension/allocation 제한을 적용하며 Classic hash DB는 SQLite read-only/query-only로 연다.
- dry-run은 source fingerprint와 typed plan을 Next SQLite에 저장한다. UI는 절대 source path가 없는 revisioned report만 받고 warning acknowledgement와 최종 승인을 별도로 수집한다.
- Classic page는 apply 때 SHA/length/decode를 다시 확인하고 기존 `ArtifactStore`를 통해 WebP·SHA·atomic manifest 규칙을 그대로 사용한다. Classic 폴더를 Next managed root로 간주하지 않는다.
- filesystem write 전에 import copy destination을 기록한다. DB apply는 모든 copy 검증 뒤 transaction으로 완료하며, rollback journal에는 이 import가 새로 삽입한 row만 기록한다.
- startup recovery는 중단된 apply를 failed→rolling_back으로 전환하고 추적된 부분 Next 폴더를 import 전용 quarantine으로 옮긴다. 원본과 격리 목적지가 동시에 존재하면 overwrite/delete하지 않고 중단한다.

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
