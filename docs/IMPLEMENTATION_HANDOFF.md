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
| DB·migration | 완료 | schema v9, future-schema 무변경 거부, timestamp/version backup, WAL·explicit startup recovery 검증 완료 |
| Hitomi search | 완료 | production live adapter, query serialization, paging/filter/popular fixture contract 검증 완료; live smoke만 미검증 |
| detail·Related | 완료 | typed galleryinfo detail·Related 5개와 source-page identity를 저장 fixture 통합 테스트로 검증 |
| thumbnail | 완료 | 전역 coordinator와 live resolver·viewport 구독·우선순위·취소·memory/negative cache 검증 완료 |
| download | 완료 | 실제 source page를 `.part`→decode/WebP→SHA-256→atomic rename→manifest 순서로 저장하고 검증 뒤에만 완료 |
| resume·reconcile | 완료 | verified page checkpoint resume, startup/manual DB·manifest·파일 검사와 quarantine saga 복구 검증 |
| file open | 완료 | verified first non-quarantined page를 root 내부 canonical path로 확인하고 Windows ShellExecute로 실행 |
| Auto Find | blocker | Phase 안내 동작만 존재 |
| gallery duplicate | blocker | Review UI는 있으나 실제 evidence·decision 영속 경로 없음 |
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

## 4. Contracts and versions

- 앱/package/Tauri version: `0.1.0`
- Rust MSRV: `1.88.0` (working tree)
- DB schema version: 9
- migration: `settings_and_window_placement`, `mock_job_event_foundation`, `gallery_and_artifact_foundation`, `gallery_primary_group`, `download_queue_contract`, `download_queue_response_revision`, `download_lifecycle_and_cancelled_state`, `verified_artifact_pipeline`, `crash_safe_quarantine_saga`
- manifest schema version: 1
- HashProfile version: 1 (artifact SHA-256 profile; perceptual duplicate profile은 Milestone E에서 별도 추가)
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
- URL query, cookie, session token, 로컬 사용자 경로를 로그에 남기지 않는다.

## 6. Tests and evidence

- 상세 검증 로그: `.runtime/verification/` (Git ignored)
- Milestone A 독립 snapshot: Rust lib 51/51, startup 1/1, clippy `-D warnings`, fmt와 whitespace 통과. 프런트 소스는 기준 commit과 동일하다.
- A/B 통합 working tree: `tools/verify.ps1` 성공 — frontend 89/89, Rust lib 77/77(외부 live smoke 1개 opt-in 제외), startup 1/1, typecheck/build/clippy/Tauri release/whitespace 통과. 이후 parser/resolver version 회귀 1개를 추가해 source suite 12/12를 재검증했다.
- 검증 로그: `.runtime/verification/verify-20260815-165051.log`.
- 외부 Hitomi live smoke는 일반 fixture CI와 분리했다. 2026-08-15 실행은 sandbox network/승인 사용량 제한 때문에 완료하지 못했으며 production 구현 완료와 live 검증 상태를 분리한다.
- Milestone C 통합 검증: `tools/verify.ps1 -SkipInstall` 성공 — Rust lib 83/83 + startup 1/1, frontend 89/89, typecheck·production build·Clippy `-D warnings`·Tauri release·whitespace 검사를 통과했다. synthetic PNG를 실제 WebP 파일·SHA-256·manifest로 만들고, 2-page 중단/재시작에서 page 1을 재요청하지 않는 resume, quarantine 이동/undo와 DB commit 직전 fault recovery를 검증했다. 로그는 `.runtime/verification/verify-20260815-180348.log`에 있다.
- 실제 Windows ShellExecute는 자동 테스트에서 외부 뷰어를 띄우지 않았으며, canonical first-page 선택까지 자동 검증했다. 최종 수동 앱 검토에서 실제 open을 확인한다.

## 7. Known limitations and blockers

현재 제품을 막는 blocker는 completion status 표에 기록한다. 각 milestone 구현 후 정확한 재현, 영향, 필요한 입력과 임시 안전 동작을 이 절에 남긴다.

## 8. Future change cautions

- 적용된 migration의 순서와 이름을 바꾸지 않는다.
- 기존 manifest·HashProfile을 version 없이 새 의미로 재해석하지 않는다.
- coordinator 밖에서 UI별 이미지 worker나 직접 원격 요청을 만들지 않는다.
- page 배열 index와 원본 source page number를 혼용하지 않는다.
- 최종 파일 atomic rename과 manifest 검증 전에 `completed`로 전환하지 않는다.
- retry 시 새 download entry를 만들지 말고 기존 entry/job attempt를 증가시킨다.
- quarantine, duplicate decision, manifest, DB artifact를 서로 독립적으로 갱신하지 않는다.
- parser 변경 시 저장 fixture와 golden contract test를 함께 갱신한다.

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
- Milestone C 이후 commit, push 결과와 PR 상태는 완료 시 SHA와 함께 누적한다.
- PR merge, `main` 직접 push, force push, release/tag 생성은 수행하지 않는다.
