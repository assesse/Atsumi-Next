# Atsumi Next 구현 전달 기록

이 문서는 Atsumi Next의 실제 구현·검증·Git 전달 결과를 누적한다. 계획이나 추정은 완료 근거로 사용하지 않으며, 각 상태는 코드·테스트·파일·commit으로 확인된 결과만 반영한다.

## 1. Baseline

- 시작 브랜치: `agent/phase-3-foundation`
- 시작 commit: `5450c8a1a77b45cb7683c75bd32ad94dd2ac72dc` (`Build Phase 3 application foundation`)
- 기준 branch: `main` (`23a3b66`)
- 기준 PR: `assesse/Atsumi-Next#1`
- 작업 시작일: 2026-08-15 (Asia/Seoul)
- 시작 시 Git 상태: 원격 작업 branch와 동일한 HEAD, Milestone A/B 후보 변경이 working tree에 미커밋 상태
- 시작 시 구현 상태: SQLite queue/retry/cancel과 전역 thumbnail coordinator는 존재하며, 실제 Hitomi read path는 working tree에 연결 중이었다. 이후 구현 이력은 아래 milestone 기록에 남기되 현재 active 범위는 completion 표와 최신 안정화 절을 따른다.

## 2. Completion status

| 영역 | 상태 | 현재 근거 |
|---|---|---|
| startup·single-instance | 완료 | 두 번째 실행은 기존 창을 복원하며, fatal startup은 non-zero exit·사용자 안내·로컬 오류 로그를 남긴다 |
| DB·migration | 완료 | schema v18, v15 folder/path·v16 candidate/root·v17 Auto Find cutoff·v18 source identity additive migration, future-schema 무변경 거부, version backup, WAL·explicit startup recovery 검증 완료 |
| Hitomi search | 완료 | production live adapter, query serialization, bounded Explore cache·prefetch, requestId별 실제 cancellation과 paging/filter/popular fixture contract 검증 완료 |
| detail·Related | 완료 | typed galleryinfo detail·Related 5개와 source-page identity를 저장 fixture 통합 테스트로 검증 |
| thumbnail | 완료 | 전역 coordinator와 live resolver·viewport 구독·400ms orphan grace·120초/256 frontend retention·우선순위·실제 취소·memory/negative cache 검증 완료 |
| download | 완료 | 실제 source page를 `.part`→bounded WebP/JPEG/PNG/experimental AVIF decode→WebP→SHA-256→atomic rename→manifest 순서로 저장하고 검증 뒤에만 완료; JXL은 typed unsupported |
| resume·reconcile | 완료 | startup은 pending quarantine·interrupted job만 빠르게 복구하고, 전체 DB·manifest·파일 검사는 명시 `app_reconcile`에서만 수행 |
| file open | 완료 | verified first non-quarantined page를 root 내부 canonical path로 확인하고 Windows ShellExecute로 실행 |
| Auto Find | 완료 | SQLite favorite/history/run/candidate/exclusion/cutoff/truncation, verified-owned `source`/`policyVersion`, 실제 source supervisor, 5개 namespace projection, 명시적 갱신·취소·복원·local filter/group·batch queue 검증 완료 |
| gallery duplicate | 완료 | verified artifact HashProfile evidence, full scan/cancel/recovery, 실제 source-page Review와 CAS decision history 검증 완료 |
| internal duplicate | 완료 | verified artifact 내부 exact/2행 이상 visual scene scan, synchronized source-page Review, CAS plan, page quarantine·undo·startup recovery 검증 완료 |
| quarantine | 완료 | root 내부 atomic move, pending saga, startup 복구, undo와 무자동삭제 검증 |
| 과거 데이터 이전 runtime | 제거 | active frontend/API/Rust source·repository·command를 제거했다. 이미 적용된 v14 migration과 역사적 table은 기존 DB 호환 때문에 불변 보존한다 |
| 설정 초기화 | 완료 | 완료 thumbnail cache만 제거하고, 명시적 확인 뒤 favorites/history/Auto Find 데이터만 transaction으로 초기화한다. 다운로드 DB/artifact/files는 보존한다 |
| Windows build·CI | 완료 | 최소 CSP·capability, 공용 `tools/verify.ps1`, Windows CI와 Tauri no-bundle release 검증 완료 |

## 3. Changes by subsystem

### Milestone A — startup·DB·CI

- `src-tauri/src/lib.rs`와 `main.rs`: 앱 single-instance가 두 번째 프로세스의 DB setup을 막고 기존 창을 복원한다. startup 오류를 성공처럼 삼키던 구조를 `Result`와 non-zero exit로 바꾸고, Windows GUI에서도 안정 문구와 `%LOCALAPPDATA%\Atsumi Next\Logs\startup-error.log` 위치를 표시한다.
- `migrations.rs`와 `sqlite_repository.rs`: future version·gap·name mismatch를 어떤 변경보다 먼저 거부한다. 실제 파일 DB의 pending migration은 version/timestamp가 포함된 `VACUUM INTO` snapshot을 먼저 만들며 실패 시 migration하지 않는다. 임시 exclusive lock을 제거하고 WAL·busy timeout·짧은 transaction을 사용한다.
- `tauri.conf.json`과 capability: CSP를 self/blob/data 및 IPC에 필요한 범위로 제한하고 window/event 권한만 허용한다.
- `.github/workflows/windows-ci.yml`과 `tools/verify.ps1`: 로컬/CI가 같은 frozen frontend, Rust fmt/check/test/clippy, Tauri release, whitespace gate를 실행한다.
- 데이터 호환성: schema version은 7로 유지된다. repository open은 더 이상 volatile job을 바꾸지 않으며, single-instance를 획득한 app setup이 startup recovery를 명시적으로 수행한다.

### Milestone B — Hitomi read path

- `source/hitomi`: galleryinfo·gg.js·Nozomi range와 WebP 후보를 명시적 type으로 parsing하며 parser/resolver contract version을 각각 1로 고정했다. 저장 fixture는 필드 누락, AVIF/WebP flag, 404/429/503/timeout과 잘못된 payload 경계를 포함한다.
- `infrastructure/hitomi_live`: production 검색·상세·Related·썸네일·페이지 다운로드가 같은 `HitomiLiveAdapter`와 pooled HTTP scheduler를 공유한다. 전역/host별 concurrency, 최소 시작 간격, `critical > visible > prefetch > download`, cancellation, bounded backoff+jitter, Retry-After와 cooldown을 적용한다.
- 원격 응답은 HTTPS allowlist와 redirect host를 다시 확인하고 size·MIME·signature·decode dimension/allocation을 제한한다. query·cookie·raw URL은 사용자 오류나 기본 로그에 노출하지 않는다.
- `ThumbnailResolver`와 frontend adapter는 새 worker를 만들지 않고 기존 전역 coordinator를 사용한다. backend의 typed failure와 retryability를 WebView까지 보존하고 접근 가능한 한국어 fallback 상태를 표시한다.
- production Tauri는 live adapter를 기본 주입하며 브라우저 review mode와 테스트만 fixture를 사용한다. 데이터 schema 변경은 없다.

### Milestone C — download·artifact·recovery

- `application/download_supervisor.rs`: bounded gallery worker가 실제 queue를 자동 claim한다. source/file 작업 밖에서만 짧은 SQLite transaction을 사용하고, cancel token·attempt generation으로 오래된 worker가 새 retry를 변경하지 못하게 한다.
- `infrastructure/artifact_store.rs`: 절대 download root의 쓰기 가능성과 canonical containment를 확인한다. page는 `.part`에 64KiB 단위로 쓰고 sync·decode·lossless WebP·SHA-256을 검증한 뒤 atomic rename한다. manifest도 temp/sync/round-trip/atomic replace를 거친다.
- `domain/artifact.rs`: manifest schema 1과 HashProfile 1, immutable source page mapping, writer/conversion policy, page digest·format·quarantine 상태를 typed model로 고정했다.
- `sqlite_repository.rs` migration 8/9: artifact/page verification metadata, page candidate attempt, quarantine saga를 additive schema로 추가했다. 파일·manifest 검증 전에는 repository가 `completed` 전이를 거부한다.
- `DownloadSupervisor::reconcile`: pending quarantine move를 먼저 복구하고 completed artifact의 page hash·manifest를 점검한 뒤 interrupted job을 verified checkpoint부터 재개한다. 모호한 원본/격리 경로는 삭제·덮어쓰지 않는다.
- frontend는 실제 `artifact_open_first`, 수동 무결성 검사, Downloads 격리·undo를 typed client로 호출한다. 브라우저 review mode는 실제 파일이 없음을 안정 오류로 표시한다.
- 데이터 호환성: DB schema는 9로 상승한다. v8은 artifact 검증 metadata, v9는 crash-safe quarantine 상태를 추가하며 기존 v7 column 의미를 바꾸지 않는다.

### Milestone D — favorites·search history·Auto Find

- `domain/auto_find.rs`와 `application/ports.rs`: 5개 metadata namespace의 정규화 favorite key, typed search history, revisioned Auto Find run·candidate·snapshot·exclusion과 `AutomationRepository` port를 추가했다.
- `migrations.rs`와 `sqlite_repository.rs`: migration 10으로 favorite, search request history, Auto Find run/candidate/exclusion을 additive schema로 추가했다. migration 11은 후보에 `series_json`, `characters_json`을 기본값 `[]`로 추가해 기존 v10 run과 후보를 손실 없이 보존한다. 성공한 non-empty 검색만 전체 정규화 요청 fingerprint로 upsert하며, run과 후보는 앱 재시작 뒤에도 남는다.
- `application/auto_find_supervisor.rs`: 명시적 refresh만 실제 `SearchRepository`를 호출한다. 현재 artist favorite를 전체 4개 언어·Recent로 조회하고 실행 중 refresh는 같은 run을 재사용하며 cancel token과 DB state로 늦은 결과를 차단한다. Milestone D 당시의 250-page 구현은 Milestone I에서 verified-owned cutoff 선적용과 cutoff 뒤 50,000-candidate limit으로 대체됐다.
- 후보 조회는 상태와 무관한 기존 download entry, 전역 Auto Find exclusion을 제외한다. Phase 5 이전에는 아직 존재하지 않는 작품 숨김·중복 판정을 추측하거나 메모리 flag로 대체하지 않는다.
- `interface/commands.rs`와 `lib.rs`: favorite/history/snapshot/refresh/cancel/exclude command와 `auto-find:changed` event를 연결했다. startup은 남은 running run을 `AUTO_FIND_INTERRUPTED`, 정상 앱 종료는 active run을 `AUTO_FIND_APP_EXIT`로 안전 종결한다.
- `domain/search.rs`, live/fixture source와 frontend projection: `GallerySummary`에 non-optional `series[]`, `characters[]`를 추가했다. 검색·상세·Related·Auto Find restore가 이를 보존하며, 여러 단어 값은 `series:rain_archives`, `character:mira_lane` token으로 각각의 Nozomi namespace endpoint에 직렬화한다.
- `App.tsx`, `GalleryCard.tsx`, `DetailWorkspace.tsx`, `ViewHeader.tsx`와 typed frontend client: startup에서 favorite/history/Auto Find snapshot을 복원하고, 5개 namespace favorite state를 카드·상세·Related가 공유하는 projection으로 계산한다. 시리즈·캐릭터 chip의 좌클릭은 namespace 검색, 우클릭은 canonical spaced favorite toggle이다. 검색 suggestion은 text/tag/language/sort/page size를 포함한 영속 요청을 재생하며 입력 change는 local draft만 바꾼다.
- Auto Find 화면은 명시적 갱신, running/completed/cancelled/failed 진행 상태, 취소와 부분 후보 보존, 다시 탐색, local 문자열·언어 filter, 전체/작가별 묶음, 현재 표시 후보 batch queue를 제공한다. toolbar와 Delete는 선택 후보를 영속 제외하고 다음 run에도 반영한다.
- 브라우저 검토 모드는 실제 원격 source를 호출하지 않는 fixture adapter 경계를 유지하면서 같은 lifecycle을 재현한다. 취소 generation 뒤 늦은 fixture 결과를 무시하고 download/exclusion을 후보에서 제거한다.
- 데이터 호환성: DB schema는 11로 상승한다. v10은 새 automation table만 추가하고 v11은 Auto Find 후보의 visible namespace metadata만 additive column으로 확장한다. v1~v10의 기존 의미, download manifest schema와 HashProfile은 변경하지 않는다.

### Milestone E — gallery duplicate evidence·Review

- `domain/duplicate.rs`, `application/duplicate_analyzer.rs`, `duplicate_supervisor.rs`: HashProfile 1/algorithm 1, exact SHA-256, 64-bit coarse dHash·pHash, 1024-bit detail dHash, luma/variance/non-uniform/edge gate와 monotonic one-to-one gap alignment를 typed domain으로 추가했다. title/artist/group/page count는 exhaustive pair worklist의 우선순위만 정한다.
- `migrations.rs`와 `sqlite_repository.rs`: migration 12로 hash profile/cache, scan run, candidate/evidence/page pair, hidden gallery, series group/member, pair exclusion과 append-only decision table을 추가했다. scan 상태·candidate replace·CAS decision side effect는 짧은 SQLite transaction이고 startup recovery는 남은 running scan만 안전하게 실패 처리한다.
- `DuplicateSupervisor`: gallery별 최신 verified complete artifact 하나만 읽고 hash cache를 artifact SHA/profile로 검증한다. 동시에 한 worker만 허용하며 취소는 worker join 뒤에만 재시작할 수 있다. progress event는 bounded 신호이고 snapshot이 canonical state다.
- `artifact_store.rs`, `artifact_thumbnail.rs`와 global thumbnail coordinator: Review는 정확한 `entryId/sourcePage`로 root-bound local WebP의 byte length·SHA를 다시 검사하고 1024px 이하 preview를 전달한다. source URL이나 local path는 frontend에 노출하지 않는다.
- `App.tsx`와 `DuplicateReviewDialog.tsx`: Downloads에서 검사 시작·취소·오류·재시도·진행률, 양쪽 gallery candidate count, 실제 evidence/page pair/history, 숨김·양쪽 연작 연결·해제·pair 제외를 연결했다. 늦은 event/snapshot과 revision conflict는 최신 snapshot/get으로 복구하고 focus를 원래 trigger로 돌린다. Downloads double-click의 artifact open 계약은 유지한다.
- E-Hentai relation은 typed optional port와 evidence kind를 제공하지만 명시적인 적법 session이 없는 현재 production에서는 disabled provider를 사용한다. session/cookie를 저장하거나 로그에 남기는 fallback은 없다.
- 데이터 호환성: DB schema는 12로 상승한다. v12는 additive하고 v1~v11 의미, manifest schema 1, download artifact HashProfile field를 재해석하지 않는다.

### Milestone F — internal scene Review·page quarantine

- `domain/internal_duplicate.rs`, `application/internal_duplicate_analyzer.rs`, `internal_duplicate_supervisor.rs`: 작품 중복 HashProfile/page cache를 공유하되 한 verified artifact 안에서만 검사한다. exact SHA 반복은 한 행을 허용하고 visual evidence는 한 장짜리 shared panel을 제외하기 위해 최소 2행의 단조 gap-tolerant block만 저장한다.
- `migrations.rs`와 `internal_duplicate_repository.rs`: migration 13으로 scan run, block/row/page evidence, 만료형 removal plan과 page quarantine saga를 additive하게 추가했다. run은 singleton이고 start/cancel/shutdown/recovery, group replace, plan revision CAS와 apply/undo completion은 짧은 SQLite transaction이다.
- `InternalDuplicateSupervisor`: gallery별 최신 verified artifact를 hash하고 진행 event를 보낸다. 사용자가 고른 keep/remove source page와 현재 파일 수·byte 합계를 15분 계획으로 고정한다. page move는 DB intent 뒤 artifact 내부 `.atsumi-page-quarantine/<plan-id>/`로 수행하고 manifest atomic replace 뒤 DB state를 확정한다.
- startup은 pending page move/restore를 원본·격리 경로 존재 상태로 재개한다. 양쪽이 모두 있거나 모두 없으면 overwrite/delete하지 않고 Review 오류로 남긴다. source page number, SHA·byte·format metadata는 격리와 undo 동안 유지된다.
- `App.tsx`, typed backend와 `InternalDuplicateDialog.tsx`: Downloads에서 전체 scan 시작·취소·진행·오류·재시도, 완료 앨범 하나의 synchronized row 검토, 행별 keep 선택, 파일 수·용량 계획 preview, 명시적 격리 적용과 이력 기반 undo를 연결했다. page 이미지는 live source가 아니라 전역 coordinator의 verified `artifactPage(entryId, sourcePage)`만 사용한다.
- 브라우저 검토 adapter는 동일한 run/plan/revision/quarantine/undo 계약을 결정론적으로 재현한다. production과 browser 모두 자동 영구 삭제 command가 없다.
- 데이터 호환성: DB schema는 13으로 상승한다. v13은 additive하고 v1~v12 의미, manifest schema 1과 HashProfile 1을 재해석하지 않는다.

### 역사적 Milestone G — 제거된 데이터 이전 구현

- 2026-08-16에는 v14 table과 active runtime 경로가 존재했다. 2026-08-21 사용자 지시로 관련 dialog, TypeScript contract/adapter, Rust domain/application/source/repository/command와 startup recovery를 전부 제거했다.
- migration 14의 version, name, SQL과 이미 생성된 네 table은 과거 migration 불변성과 기존 DB 호환을 위해 그대로 둔다. 현재 runtime은 이 table을 읽거나 쓰지 않는다.
- 이 절은 과거 전달 증거를 설명하는 기록일 뿐 현재 기능이나 API 계약이 아니다.

### Milestone H — production UI·diagnostics·delivery polish

- `SettingsDialog.tsx`: 실제 동작하는 단일 설정 화면만 남겼다. cache clear, 화면·네트워크 draft 기본값, 탐색 데이터 초기화를 별도 control로 제공하고 다운로드 DB/artifact/files 보존 범위를 명시한다.
- `DetailWorkspace.tsx`: source page를 누르면 전역 `ThumbnailCoordinator`의 같은 `sourcePage` key를 critical priority로 다시 사용하는 확대 dialog가 열린다. Esc·닫기와 opener focus 복원, page count 변경 시 안전한 자동 닫기를 검증했다.
- `ThumbnailProvider.tsx`, `main.tsx`, `SideRail.tsx`: production composition root가 thumbnail client를 반드시 주입하며 context의 fixture fallback을 제거했다. 패키지 앱은 `Hitomi live`, 명시적 browser review mode만 `Browser fixture`로 표시한다.
- Settings·page preview dialog는 열기 trigger와 close button focus를 관리한다. 기존 Review·내부 Review·종료 dialog의 focus 복원, reduced motion, keyboard/accessible label 계약을 유지한다.
- `interface/api.rs`, `main.rs`, `start_app_hidden.ps1`: DB 내부 detail을 사용자 API message에서 분리하고 stable code만 노출한다. startup/launcher log는 사용자 profile과 전체 URL을 가리며 release GUI launcher는 project/executable 절대 경로를 로그에 남기지 않는다.
- `THIRD_PARTY_NOTICES.md`: vendored Fluent UI System Icons 1.1.328의 정확한 자산 범위·MIT 고지와 lockfile 기반 dependency provenance를 기록했다. package/Cargo/Tauri version은 모두 `0.1.0`으로 일치하며 미검증 `1.0.0` 표시는 하지 않는다.
- 데이터 호환성: H 완료 당시 DB schema는 14였고 H 자체는 schema·manifest·HashProfile 의미를 바꾸지 않았다. 이후 Milestone I의 additive v15~v18이 적용됐으며 manifest와 HashProfile은 계속 version 1이다.

### Milestone I — post-completion 안정화 (adaptive card·cache/cancel·schema v15~v18)

- `GalleryCard.tsx`와 `galleryCardLayout.ts`: 포스터형 대신 가로 밀도형을 유지하고 점수·날짜를 제거했다. cover root를 `ResizeObserver`로 측정해 card CSS height를 직접 고정하므로 content와 태그가 외곽을 늘리지 않는다. 160/220/280/360px에서 card/cover 차이 1px 이하를 component test로 고정했다.
- `design/card-adaptive-layout-review.html`은 preview 160/220/280/360px와 1/2/3/4열, 짧고 긴 제목, pipe 번역, 명시 subtitle, 작가/그룹, 3/10/25+ 태그, 한중일, F/M/중립 favorite, download/duplicate 상태를 한 matrix에서 비교한다. 열/preview를 다시 넓히면 전체 원본 tag element를 다시 드러낸 뒤 재측정하므로 앞선 좁은 상태에서 숨긴 태그가 계속 누락되지 않는다.
- matrix의 태그 결과는 “160px=1줄”이나 고정 개수로 정하지 않는다. 제목·byline·meta를 뺀 실제 높이에 24px chip과 자릿수별 `+N`을 함께 측정해 가능한 최대 개수를 표시한다. `+N`이 들어오려면 tag를 더 줄일 수 있고, `+N`만 들어가거나 overflow 자체가 숨는 극단도 순수 함수로 고정했다. 표시 순서는 favorite 우선, Female→Male→중립, 같은 bucket 원래 순서이며 canonical `gallery.tags`는 바꾸지 않는다. F/M marker와 주황 star는 별도 span이라 favorite에도 namespace가 남는다. 숨긴 chip은 DOM·Tab 순서에서 제거된다. pipe 제목은 첫 segment를 주 제목, 나머지와 중복 제거한 명시 subtitle을 한 secondary line으로 표시하고 canonical title은 tooltip/accessible name에 보존한다.
- `thumbnail/client.ts`: 마지막 구독 해제 뒤 400ms orphan grace와 resolved asset 120초/최대 256개 retention을 추가했다. 빠른 viewport 왕복은 동일 request/blob을 재사용하고 최종 eviction에서만 URL을 revoke한다. Rust coordinator 기본 cache 512 entries/64MiB/30분, retryable/permanent negative TTL 3초/5분은 유지된다.
- `ExplorePageSession`, frontend/backend adapter와 `search_page_cancel`: query별 settled page 최대 5개·현재 ±2·인접 prefetch·page별 scroll을 구현했다. query reset은 모든 requestId를 취소하고 backend는 active token뿐 아니라 최대 256개의 cancel-before-start tombstone으로 실제 source 작업을 막는다.
- schema v15 `artifact_folder_template_and_immutable_path`: 새 artifact folder template, `{id}` 필수, Windows path sanitization과 기존 relative path immutable trigger를 추가했다. 기존 artifact 자동 rename/move는 없다.
- schema v16 `download_candidate_diagnostics_and_artifact_root_snapshot`: page candidate 형식/status/content-type/retryability와 artifact 최초 root snapshot을 추가하고 root를 immutable하게 고정했다.
- download/thumbnail image pipeline은 pinned pure-Rust `avif-rust 0.0.6`/`bin-rs 0.0.10`으로 bounded AVIF를 experimental 지원한다. JXL은 diagnostic만 남기고 fallback 뒤 non-retryable `IMAGE_FORMAT_UNSUPPORTED`다.
- schema v17 `auto_find_history_cutoff_evidence`: 설정/run history mode, verified owned artist, 작가별 cutoff evidence와 truncation을 추가했다. 현재 literal은 `source=verified_owned_artifact`, `policyVersion=1`; cutoff 뒤 50,000 candidate limit이며 과거 250-page 정책은 폐기됐다.
- Windows download root 표시 경계는 well-formed `\\?\D:\...`와 `\\?\UNC\...`만 일반 drive/UNC로 바꾼다. 폴더 선택 뒤 canonical root를 설정에 그대로 저장하던 유입 경로를 차단했으며, 기존 artifact `root_snapshot`과 파일은 그대로 둔다. 폴더 template 미리보기는 실제 Rust planner command를 사용한다.
- schema v18 `gallery_source_revision_identity`: remote source fingerprint를 문자열 identity로 저장하고 signed SQLite 내부 revision과 분리했다. gallery 4113714/4132312에서 발생한 unsigned source revision 변환 오류를 `u64::MAX` 회귀 test로 차단한다.
- schema v19 `related_gallery_preview_preference`: Floating Detail의 Related galleries cover 폭(180~320px, 기본 240)을 Explore·Downloads card preview와 독립적으로 저장한다. 상세와 Related의 일반 태그는 동일한 favorite → Female → Male → neutral 순서를 쓰며 Related에는 series/character chip을 표시하지 않는다.
- card layout은 일곱 preview preset(160/190/220/250/280/320/360, 기본 220), preset별 typography·2/2/3/4/5/6/7 tag rows, grid별 시각 행 최대 intrinsic cover 높이를 공유한다. 독립 grid와 불완전 마지막 행은 서로 영향을 주지 않는다.
- 데이터 호환성: DB schema는 19이다. v15~v19은 additive하고 기존 `relative_directory`를 다시 계산하지 않으며 manifest schema 1과 HashProfile 1을 재해석하지 않는다.

## 4. Contracts and versions

### Maintenance actions

- Settings의 저장 데이터 관리는 `빠른 복구`, `라이브러리 검사 및 재구축`, `앱 데이터 완전 초기화` 세 action만 사용한다. Cache/history/user decision을 혼합 삭제하는 기존 exploration reset은 이 UI 경로에서 사용하지 않는다.
- Factory reset은 실행 중 SQLite를 삭제하지 않는다. worker shutdown 후 marker를 기록하고 프로세스를 종료한 다음 startup이 DB/WAL/SHM을 recovery backup으로 이동한다. 외부 원본, `.atsumi-quarantine`, `.atsumi-page-quarantine`, `.atsumi-recovery`는 보존한다.

- 앱/package/Tauri version: `0.1.0`
- Rust MSRV: `1.88.0` (working tree)
- DB schema version: 19
- migration: `settings_and_window_placement`, `mock_job_event_foundation`, `gallery_and_artifact_foundation`, `gallery_primary_group`, `download_queue_contract`, `download_queue_response_revision`, `download_lifecycle_and_cancelled_state`, `verified_artifact_pipeline`, `crash_safe_quarantine_saga`, `favorites_search_history_and_auto_find`, `auto_find_visible_metadata`, `artifact_duplicate_evidence_and_decisions`, `internal_scene_review_and_page_quarantine`, `classic_read_only_import_and_rollback`(역사적 DDL만 보존), `artifact_folder_template_and_immutable_path`, `download_candidate_diagnostics_and_artifact_root_snapshot`, `auto_find_history_cutoff_evidence`, `gallery_source_revision_identity`
- manifest schema version: 1
- HashProfile version: 1 / algorithm version 1 (artifact SHA-256 + 작품 중복 64-bit coarse dHash·pHash, 1024-bit detail dHash와 content gate)
- Hitomi parser version: 1
- Hitomi resolver version: 1
- 주요 command/event: `docs/API_CONTRACT_V2.md`를 기준으로 하며 실제 handler와 함께 갱신한다.

## 5. Reliability and security invariants

- SQLite만 영속 상태의 canonical source로 사용한다.
- 실제 파일·manifest·해시 검증을 모두 통과한 artifact만 `completed`가 될 수 있다.
- 원본 source page number와 source revision identity는 배열 index·SQLite 내부 revision과 분리하고 변경하지 않는다.
- 모든 화면은 프로세스 전역 `ThumbnailCoordinator` 하나를 공유한다.
- 검색·썸네일·다운로드는 공용 HTTP budget과 host cooldown을 공유한다.
- filesystem 경로는 canonical download root 내부만 허용한다.
- 자동 판정만으로 사용자 파일을 영구 삭제하지 않는다.
- 현재 quarantine은 undo만 제공하고 영구 purge command는 제공하지 않는다.
- Classic 저장소와 원본 사용자 데이터에는 active runtime 접근 경로가 없다.
- 지원 버전보다 새로운 DB schema는 변경 전에 거부한다.
- 실제 파일 DB migration 전에 timestamp와 version이 포함된 일관된 backup을 만들고 덮어쓰지 않는다.
- SQLite는 WAL·busy timeout·짧은 transaction을 사용하며 repository open 자체는 job 복구나 독점 잠금을 수행하지 않는다.
- single-instance를 획득한 앱 setup만 중단된 job을 명시적으로 복구한다.
- 오류 문자열을 parsing해 상태나 retry 정책을 결정하지 않는다.
- 검색 입력은 local draft이며 명시적 검색 제출 또는 Auto Find refresh 외에는 원격 요청을 만들지 않는다.
- favorite, search history, Auto Find run·후보·제외의 canonical source도 SQLite다. `auto-find:changed` event나 frontend set을 복원 원본으로 사용하지 않는다.
- Auto Find 취소와 종료 뒤에는 cancellation token뿐 아니라 SQLite run state도 확인해 늦은 source 결과를 저장하지 않는다.
- URL query, cookie, session token, 로컬 사용자 경로를 로그에 남기지 않는다.
- 기존 artifact `relative_directory`와 `root_snapshot`은 immutable이다. folder template 변경은 새 artifact에만 적용하고 자동 rename/move하지 않는다.
- Auto Find cutoff는 검증 소유 artifact와 versioned evidence로만 만들며 provenance가 없으면 전체 이력을 포함한다.

## 6. Tests and evidence

- 상세 검증 로그: `.runtime/verification/` (Git ignored)
- Milestone A 독립 snapshot: Rust lib 51/51, startup 1/1, clippy `-D warnings`, fmt와 whitespace 통과. 프런트 소스는 기준 commit과 동일하다.
- A/B 통합 working tree: `tools/verify.ps1` 성공 — frontend 89/89, Rust lib 77/77(외부 live smoke 1개 opt-in 제외), startup 1/1, typecheck/build/clippy/Tauri release/whitespace 통과. 이후 parser/resolver version 회귀 1개를 추가해 source suite 12/12를 재검증했다.
- 검증 로그: `.runtime/verification/verify-20260815-165051.log`.
- 외부 Hitomi live smoke는 일반 fixture CI와 분리했다. 2026-08-15 실행은 sandbox network/승인 사용량 제한 때문에 완료하지 못했으며 production 구현 완료와 live 검증 상태를 분리한다.
- Milestone C 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — Rust lib 83/83 + startup 1/1, frontend 89/89, typecheck·production build·Clippy `-D warnings`·Tauri release·whitespace 검사를 통과했다. synthetic PNG를 실제 WebP 파일·SHA-256·manifest로 만들고, 2-page 중단/재시작에서 page 1을 재요청하지 않는 resume, quarantine 이동/undo와 DB commit 직전 fault recovery를 검증했다. 로그는 `.runtime/verification/verify-20260815-180348.log`에 있다.
- 실제 Windows ShellExecute는 자동 테스트에서 외부 뷰어를 띄우지 않았으며, canonical first-page 선택까지 자동 검증했다. 최종 수동 앱 검토에서 실제 open을 확인한다.
- Milestone D backend 집중 검증: `cargo +stable test --locked --manifest-path src-tauri/Cargo.toml --lib application::auto_find_supervisor::tests -- --test-threads=1`에서 6/6 통과했다. 5개 namespace 정규화·영속, non-empty 제출 이력, 단일 refresh 직렬화, download/명시적 제외, restart 결과 복원, 취소 뒤 늦은 후보 차단, startup interrupted와 부분 후보 보존을 검증했다.
- Milestone D backend 전체 검증: `cargo +stable test --locked --manifest-path src-tauri/Cargo.toml --all-targets`에서 Rust lib 90/90(외부 live smoke 1개 opt-in 제외), main 1/1을 통과했다. v10→v11 후보 보존과 series/character Nozomi serializer를 포함하며 `cargo fmt --check`, `cargo check --all-targets`, `cargo clippy --all-targets -- -D warnings`도 통과했다.
- Milestone D frontend 집중 검증: TypeScript typecheck와 Vite production build(63 modules)가 통과했다. `tools/run_frontend.ps1 test`는 16 files, 100/100 tests를 통과해 5개 namespace card/detail/Related favorite 일관성, multiword namespace 검색, remount 복원, 구조화 이력 재생, 입력 중 무요청, 명시적 Auto Find lifecycle, 부분 후보 취소 보존, download/exclusion filter와 stale fixture 차단을 검증했다. 기존 React act-environment warning은 실패가 아니며 별도 운영 polish 항목이다.
- Milestone D 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — frontend 100/100, Rust lib 90/90(외부 live smoke 1개 opt-in 제외), startup 1/1, typecheck·production build·Clippy `-D warnings`·Tauri release·whitespace 검사를 통과했다. 로그는 `.runtime/verification/verify-20260815-184914.log`에 있다.
- Milestone E 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — frontend typecheck·Vite production build와 17 files 109/109 tests, Rust lib 105/105(외부 live smoke 1개 opt-in 제외), main 1/1, fmt/check/Clippy `-D warnings`, Tauri release no-bundle, whitespace를 통과했다. blank/저정보, 서로 다른 고대비 흑백, 작은 실제 장면 변화와 2/10 공통 panel negative, 재압축·해상도/번역 visual positive, containment 양방향, metadata 우선순위+전수 fallback, page 비재사용 alignment, cancel→join→restart, recovery, CAS/series/Auto Find filter를 포함한다. 로그는 `.runtime/verification/verify-20260816-025506.log`에 있다.
- Milestone F 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — frontend typecheck·Vite production build와 18 files 112/112 tests, Rust lib 109/109(외부 live smoke 1개 opt-in 제외), main 1/1, fmt/check/Clippy `-D warnings`, Tauri release no-bundle, whitespace를 통과했다. exact source page 2·8 탐지, page 8 단독 격리·manifest/DB 상태, 원본 번호·위치 undo, DB intent와 file move 뒤 강제 중단을 모사한 startup convergence를 실제 임시 SQLite/WebP/파일 시스템 통합 테스트로 검증했다. 로그는 `.runtime/verification/verify-20260816-033555.log`에 있다.
- Milestone G 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — frontend typecheck·Vite production build와 19 files 114/114 tests, Rust lib 112/112(외부 live smoke 1개 opt-in 제외), main 1/1, fmt/check/Clippy `-D warnings`, Tauri release no-bundle, whitespace를 통과했다. 실제 임시 Classic state/manifest/PNG와 별도 Next SQLite/root를 사용해 read-only inventory, conflict 비추측, verified WebP copy·manifest·completed 등록, favorite/download rollback, Classic 원본 byte 불변, applying 중 부분 폴더의 startup quarantine 수렴을 검증했다. 로그는 `.runtime/verification/verify-20260816-041453.log`에 있다.
- Milestone H 최종 통합 검증: `tools/verify.ps1` 성공 — frozen pnpm 11.16.0 install, frontend 20 files 115/115, typecheck, Vite production build 66 modules, Rust lib 113/113와 startup 2/2(기본 suite에서 opt-in live 1개 제외), fmt/check/Clippy `-D warnings`, Tauri release no-bundle와 whitespace를 통과했다. 로그는 `.runtime/verification/verify-20260816-043508.log`에 있다.
- opt-in 실제 Hitomi smoke: sandbox 밖 읽기 전용 실행에서 Recent search와 첫 metadata/page 계약 1/1 통과했다. `pnpm audit --prod --audit-level high`도 2026-08-16 registry advisory 기준 알려진 취약점 0건이다. `cargo-audit`은 이 PC에 설치되어 있지 않아 별도 RustSec CLI audit은 수행하지 않았다.
- 숨김 launcher check는 typecheck와 `tauri-cli 2.11.4` 확인을 통과했고 `.runtime/launcher-check.log`에 사용자 profile/project 절대 경로를 남기지 않았다.
- 최종 release `src-tauri/target/release/atsumi-next.exe`를 실제 Windows GUI로 실행했고 `Atsumi Next` main window가 생성되어 responsive 상태임을 확인했다. 사용자가 현재 열린 창에서 실제 검색·다운로드 폴더 선택·viewer open의 대화형 동작을 검토할 수 있다.
- 2026-08-20 UI·경로 안정화 전체 검증: `tools/verify.ps1` 성공 — frontend 21 files 135/135, Rust lib 140/140(일반 suite에서 opt-in live 1개 제외), startup 2/2, typecheck·Vite production build·fmt/check/clippy·Tauri release `--no-bundle`·whitespace를 통과했다. 로그는 `.runtime/verification/verify-20260820-211217.log`에 있다.
- opt-in full download live 증거: gallery `4113714`의 metadata 18 pages, download/store/reopen 검증 18/18, 선택 형식 WebP 18개, 선택 payload 합계 12,396,942 bytes. 이는 단일 gallery 증거이며 AVIF/JXL 전체 corpus 보증은 아니다.
- 2026-08-21 카드 행/preset·schema v18 source identity·초기화·runtime 제거 전체 검증: `tools/verify.ps1 -SkipInstall` 성공 — frontend 23 files 140/140, Rust lib 140/140(일반 suite에서 opt-in live 1개 제외), startup 2/2, typecheck·Vite production build·fmt/check/clippy·Tauri release `--no-bundle`·whitespace를 통과했다. 로그는 `.runtime/verification/verify-20260821-011639.log`에 있다.

## 7. Known limitations and blockers

제품 기능과 Windows release build를 막는 알려진 blocker는 없다. 아래 항목은 의도된 안전 제한 또는 후속 개선 가능 범위다.

- Auto Find는 history cutoff를 Nozomi ID에 먼저 적용한 뒤 작가당 최대 50,000 candidate를 처리한다. 초과는 `candidate_limit_after_cutoff`로 영속하므로 무제한 전체 조회로 표현하지 않는다.
- E-Hentai relation evidence는 사용자가 명시적인 적법 session을 제공하지 않아 production에서 비활성이다. 작품 중복 검사는 session 없이 local artifact evidence만으로 정상 동작한다. 향후 활성화할 때 cookie/session은 process memory의 redacted provider 입력으로만 취급하고 DB·manifest·로그에는 쓰지 않아야 한다.
- 과거 데이터 이전 UI/API/runtime은 제거됐다. v14 migration table은 기존 DB 호환을 위해 남지만 새 데이터 이전 수단으로 사용할 수 없다.
- page quarantine은 undo를 제공하지만 영구 purge는 의도적으로 제공하지 않는다. 공간 회수 UI는 자동 삭제 없이 별도 사용자 승인 정책으로만 추가해야 한다.
- artifact decode는 WebP/JPEG/PNG와 experimental AVIF를 지원한다. AVIF decoder는 정확히 고정한 순수 Rust crate와 bounded allocation을 사용하지만 대표 live corpus 검증은 남아 있다. JPEG XL은 아직 지원하지 않고 fallback 뒤 `IMAGE_FORMAT_UNSUPPORTED`로 종료한다.
- 새 folder template은 기존 artifact에 소급 적용되지 않는다. 기존 `relative_directory`/`root_snapshot` 자동 rename·move 기능이 없으며 수동 파일 이동도 manifest/DB 불일치를 만든다.
- GitHub 전달 blocker는 해소했다. 2026-08-20 GitHub CLI의 device flow를 사용자가 직접 승인했고 `gh auth status`가 keyring의 `assesse` 계정을 정상으로 확인했다. credential을 추출하거나 문서·로그·Git history에 저장하지 않았다.

## 8. Future change cautions

- 적용된 migration의 순서와 이름을 바꾸지 않는다.
- 기존 manifest·HashProfile을 version 없이 새 의미로 재해석하지 않는다.
- coordinator 밖에서 UI별 이미지 worker나 직접 원격 요청을 만들지 않는다.
- page 배열 index와 원본 source page number를 혼용하지 않는다.
- 최종 파일 atomic rename과 manifest 검증 전에 `completed`로 전환하지 않는다.
- retry 시 새 download entry를 만들지 말고 기존 entry/job attempt를 증가시킨다.
- quarantine, duplicate decision, manifest, DB artifact를 서로 독립적으로 갱신하지 않는다.
- parser 변경 시 저장 fixture와 golden contract test를 함께 갱신한다.
- Auto Find 입력 change handler에서 `search_submit`이나 `auto_find_refresh`를 호출하지 않는다. 명시적 사용자 command 경계를 유지한다.
- `auto-find:changed`만으로 후보 목록을 구성하지 말고 revisioned run event 뒤 `auto_find_snapshot`을 다시 읽을 수 있게 유지한다.
- 새 Auto Find run을 메모리에만 만들거나 기존 running run과 병렬 시작하지 않는다. SQLite의 단일 running invariant와 supervisor gate를 함께 보존한다.
- 작품 중복 hash cache는 artifact SHA-256과 HashProfile version이 모두 맞을 때만 재사용한다. threshold나 feature를 바꾸면 새 profile/algorithm version과 migration·golden test를 함께 추가한다.
- `duplicate:changed` event로 후보나 판정 이력을 구성하지 않는다. 서로 다른 run의 늦은 snapshot이 최신 run을 덮지 않도록 startedAt/revision/token 경계를 유지한다.
- Review에 live `galleryPage`를 사용하지 않는다. 판정 evidence는 반드시 root-bound verified `artifactPage(entryId, sourcePage)`와 immutable source page 번호를 사용한다.
- 내부 visual duplicate를 한 page match만으로 생성하지 않는다. 최소 2행 monotonic scene block, plan revision/byte snapshot과 page quarantine saga를 함께 유지한다.
- 과거 데이터 이전 command/source adapter를 v14 table 존재만 보고 되살리지 않는다. 새 이전 기능은 별도 승인·계약·migration으로 다시 설계해야 한다.
- 기존 artifact 위치를 template 설정에 맞춰 갱신하지 않는다. relocation이 필요하면 별도 dry-run·revisioned journal·rollback migration을 먼저 설계한다.
- `search_page_cancel`을 frontend stale-result 무시로 대체하지 않는다. active source token과 cancel-before-start tombstone 경계를 유지한다.

## 9. Recovery and rollback

- migration backup은 실제 DB와 같은 디렉터리에 version이 포함된 `.bak` snapshot으로 둔다.
- migration 실패 시 원본 DB transaction을 commit하지 않고 backup 위치를 사용자 오류에 제공한다.
- interrupted download는 page checkpoint와 `.part`를 reconcile한 뒤 명시적 resume한다.
- 손상되거나 모호한 manifest·파일은 자동 삭제하지 않고 review 대상으로 분류한다.
- v15~v18은 additive지만 schema downgrade는 지원하지 않는다. 실제 downgrade는 자동 migration 전 backup과 호환 binary를 함께 복원하고 운영 DB에서 수동 DDL을 실행하지 않는다.
- v14의 역사적 table은 runtime에서 접근하지 않지만 rollback 목적으로 수동 삭제하지 않는다.
- 기존 artifact folder/root는 rollback 과정에서도 자동 rename하지 않는다. 모호한 경로는 reconcile Review에 남기고 overwrite/delete하지 않는다.
- Git rollback 범위는 milestone별 독립 commit으로 유지한다.

## 10. Git delivery

- 현재 작업 branch: `agent/phase-3-foundation`
- 이번 UI·경로 안정화를 시작한 원격 코드 기준 HEAD: `88b4f6e3fdcb0f24c4c74390b1ed9085c390f4fb`
- 안정화 commit 순서: `bff5cee` adaptive card, `e781e70` thumbnail churn retention, `1f969d5` Explore cache/prefetch, `8297b2b` safe artifact folder templates, `c64c3b6` unsupported gallery recovery/diagnostics, `aacda1c` Auto Find history cutoff, `38fa2c0` actual Explore cancellation.
- 이번 2026-08-21 작업은 사용자 지시에 따라 로컬 구현·검증까지만 수행하며 stage/commit/push/PR 수정은 하지 않는다.
- 전달자는 `git diff --check`, 전체 verify와 최종 `git status --short`를 보고하고 Git 변경은 사용자 검토 뒤 별도 승인으로 수행한다.
