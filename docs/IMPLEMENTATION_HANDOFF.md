# Atsumi Next 구현 전달 기록

이 문서는 Atsumi Next의 실제 구현·검증·Git 전달 결과를 누적한다. 계획이나 추정은 완료 근거로 사용하지 않으며, 각 상태는 코드·테스트·파일·commit으로 확인된 결과만 반영한다.

## 1. Baseline

- 시작 브랜치: `agent/phase-3-foundation`
- 시작 commit: `5450c8a1a77b45cb7683c75bd32ad94dd2ac72dc` (`Build Phase 3 application foundation`)
- 기준 branch: `main` (`23a3b66`)
- 기준 PR: `assesse/Atsumi-Next#1` (GitHub 인증 상태 재확인 필요)
- 작업 시작일: 2026-08-15 (Asia/Seoul)
- 시작 시 Git 상태: 원격 작업 branch와 동일한 HEAD, Milestone A/B 후보 변경이 working tree에 미커밋 상태
- 시작 시 구현 상태: SQLite queue/retry/cancel과 전역 thumbnail coordinator는 존재하며, 실제 Hitomi read path는 working tree에 연결 중이다. 실제 artifact download·resume·open, Auto Find, 작품/내부 중복, Classic import는 미완성이다.

## 2. Completion status

| 영역 | 상태 | 현재 근거 |
|---|---|---|
| startup·single-instance | 완료 | 두 번째 실행은 기존 창을 복원하며, fatal startup은 non-zero exit·사용자 안내·로컬 오류 로그를 남긴다 |
| DB·migration | 완료 | schema v12, v11→v12 additive duplicate evidence schema, future-schema 무변경 거부, version backup, WAL·explicit startup recovery 검증 완료 |
| Hitomi search | 완료 | production live adapter, query serialization, paging/filter/popular fixture contract 검증 완료; live smoke만 미검증 |
| detail·Related | 완료 | typed galleryinfo detail·Related 5개와 source-page identity를 저장 fixture 통합 테스트로 검증 |
| thumbnail | 완료 | 전역 coordinator와 live resolver·viewport 구독·우선순위·취소·memory/negative cache 검증 완료 |
| download | 완료 | 실제 source page를 `.part`→decode/WebP→SHA-256→atomic rename→manifest 순서로 저장하고 검증 뒤에만 완료 |
| resume·reconcile | 완료 | verified page checkpoint resume, startup/manual DB·manifest·파일 검사와 quarantine saga 복구 검증 |
| file open | 완료 | verified first non-quarantined page를 root 내부 canonical path로 확인하고 Windows ShellExecute로 실행 |
| Auto Find | 완료 | SQLite favorite/history/run/candidate/exclusion, 실제 source supervisor, 5개 namespace 카드·상세·Related projection, 명시적 갱신·취소·복원·local filter/group·batch queue 검증 완료 |
| gallery duplicate | 완료 | verified artifact HashProfile evidence, full scan/cancel/recovery, 실제 source-page Review와 CAS decision history 검증 완료 |
| internal duplicate | blocker | 실제 artifact 기반 scan·removal plan 미구현 |
| quarantine | 완료 | root 내부 atomic move, pending saga, startup 복구, undo와 무자동삭제 검증 |
| Classic import | blocker | read-only dry-run·conflict·rollback 미구현 |
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
- `application/auto_find_supervisor.rs`: 명시적 refresh만 실제 `SearchRepository`를 호출한다. 현재 artist favorite를 전체 4개 언어·Recent·page size 200으로 순회하고, 한 작가당 250 page 안전 상한 안에서 후보를 저장한다. 실행 중 refresh는 같은 run을 재사용하고 cancel token과 DB state로 늦은 결과를 차단한다.
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

## 4. Contracts and versions

- 앱/package/Tauri version: `0.1.0`
- Rust MSRV: `1.88.0` (working tree)
- DB schema version: 12
- migration: `settings_and_window_placement`, `mock_job_event_foundation`, `gallery_and_artifact_foundation`, `gallery_primary_group`, `download_queue_contract`, `download_queue_response_revision`, `download_lifecycle_and_cancelled_state`, `verified_artifact_pipeline`, `crash_safe_quarantine_saga`, `favorites_search_history_and_auto_find`, `auto_find_visible_metadata`, `artifact_duplicate_evidence_and_decisions`
- manifest schema version: 1
- HashProfile version: 1 / algorithm version 1 (artifact SHA-256 + 작품 중복 64-bit coarse dHash·pHash, 1024-bit detail dHash와 content gate)
- Hitomi parser version: 1
- Hitomi resolver version: 1
- 주요 command/event: `docs/API_CONTRACT_V2.md`를 기준으로 하며 실제 handler와 함께 갱신한다.

## 5. Reliability and security invariants

- SQLite만 영속 상태의 canonical source로 사용한다.
- 실제 파일·manifest·해시 검증을 모두 통과한 artifact만 `completed`가 될 수 있다.
- 원본 source page number는 배열 index와 분리하고 변경하지 않는다.
- 모든 화면은 프로세스 전역 `ThumbnailCoordinator` 하나를 공유한다.
- 검색·썸네일·다운로드는 공용 HTTP budget과 host cooldown을 공유한다.
- filesystem 경로는 canonical download root 내부만 허용한다.
- 자동 판정만으로 사용자 파일을 영구 삭제하지 않는다.
- quarantine의 영구 삭제는 사용자의 명시적 명령으로만 수행한다.
- Classic 저장소와 원본 사용자 데이터는 읽기 전용 입력이다.
- 지원 버전보다 새로운 DB schema는 변경 전에 거부한다.
- 실제 파일 DB migration 전에 timestamp와 version이 포함된 일관된 backup을 만들고 덮어쓰지 않는다.
- SQLite는 WAL·busy timeout·짧은 transaction을 사용하며 repository open 자체는 job 복구나 독점 잠금을 수행하지 않는다.
- single-instance를 획득한 앱 setup만 중단된 job을 명시적으로 복구한다.
- 오류 문자열을 parsing해 상태나 retry 정책을 결정하지 않는다.
- 검색 입력은 local draft이며 명시적 검색 제출 또는 Auto Find refresh 외에는 원격 요청을 만들지 않는다.
- favorite, search history, Auto Find run·후보·제외의 canonical source도 SQLite다. `auto-find:changed` event나 frontend set을 복원 원본으로 사용하지 않는다.
- Auto Find 취소와 종료 뒤에는 cancellation token뿐 아니라 SQLite run state도 확인해 늦은 source 결과를 저장하지 않는다.
- URL query, cookie, session token, 로컬 사용자 경로를 로그에 남기지 않는다.

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

## 7. Known limitations and blockers

현재 제품을 막는 blocker는 completion status 표에 기록한다. 각 milestone 구현 후 정확한 재현, 영향, 필요한 입력과 임시 안전 동작을 이 절에 남긴다.

- Auto Find의 현재 안전 상한은 작가당 250 page다. 그 이상을 가진 작가에서는 source가 보고한 전체 page 중 후반 후보가 이번 run에 포함되지 않는다. 범위를 임의로 무제한화하지 말고 scheduler 부하·중단 복구와 함께 정책을 조정해야 한다.
- E-Hentai relation evidence는 사용자가 명시적인 적법 session을 제공하지 않아 production에서 비활성이다. 작품 중복 검사는 session 없이 local artifact evidence만으로 정상 동작한다. 향후 활성화할 때 cookie/session은 process memory의 redacted provider 입력으로만 취급하고 DB·manifest·로그에는 쓰지 않아야 한다.
- Classic favorite/search history import는 Milestone G의 read-only dry-run·conflict·rollback 경계 전에는 수행하지 않는다.
- artifact decode는 현재 검증된 WebP와 JPEG/PNG 입력을 지원한다. source의 AVIF 가능 flag와 후보는 parse하지만 raw AVIF만 남은 page를 실제 WebP로 decode하는 기능은 아직 없다. downloader는 WebP와 원본 JPEG/PNG fallback을 우선하며 지원 후보가 없으면 typed failure로 종료한다.

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

## 9. Recovery and rollback

- migration backup은 실제 DB와 같은 디렉터리에 version이 포함된 `.bak` snapshot으로 둔다.
- migration 실패 시 원본 DB transaction을 commit하지 않고 backup 위치를 사용자 오류에 제공한다.
- interrupted download는 page checkpoint와 `.part`를 reconcile한 뒤 명시적 resume한다.
- 손상되거나 모호한 manifest·파일은 자동 삭제하지 않고 review 대상으로 분류한다.
- Next는 Classic을 변경하지 않으므로 Next 전용 profile을 사용하지 않으면 Classic으로 돌아갈 수 있다.
- Git rollback 범위는 milestone별 독립 commit으로 유지한다.

## 10. Git delivery

- 최종 branch: `agent/phase-3-foundation`
- 기준 원격: `origin` → `assesse/Atsumi-Next`
- `6a2c96a` — `chore: harden startup database safety and windows ci`
- `af9878a` — `feat: connect live hitomi search metadata and thumbnails`
- `80919bd` — `feat: implement resilient artifact downloads and recovery`
- `a8f0ca1` — `feat: complete favorites and auto find workflows`
- Milestone E 이후 commit, push 결과와 PR 상태는 완료 시 SHA와 함께 누적한다.
- PR merge, `main` 직접 push, force push, release/tag 생성은 수행하지 않는다.
