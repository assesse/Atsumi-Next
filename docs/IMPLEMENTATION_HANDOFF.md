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
| DB·migration | 완료 | schema v7, future-schema 무변경 거부, timestamp/version backup, WAL·explicit startup recovery 검증 완료 |
| Hitomi search | 부분 완료 | production live adapter 후보 구현, 전체 계약 검증 전 |
| detail·Related | 부분 완료 | typed detail projection은 있으나 Related·page 전체 흐름 검증 전 |
| thumbnail | 부분 완료 | 전역 coordinator와 live resolver 후보 구현, disk cache 없음 |
| download | blocker | queue는 실제 artifact pipeline 없이 `interrupted`에서 정직하게 중단 |
| resume·reconcile | blocker | 파일 checkpoint·manifest·reconcile 미구현 |
| file open | blocker | `artifact_open_first` 미구현 |
| Auto Find | blocker | Phase 안내 동작만 존재 |
| gallery duplicate | blocker | Review UI는 있으나 실제 evidence·decision 영속 경로 없음 |
| internal duplicate | blocker | 실제 artifact 기반 scan·removal plan 미구현 |
| quarantine | blocker | DB·filesystem move·undo 미구현 |
| Classic import | blocker | read-only dry-run·conflict·rollback 미구현 |
| Windows build·CI | 완료 | 최소 CSP·capability, 공용 `tools/verify.ps1`, Windows CI와 Tauri no-bundle release 검증 완료 |

## 3. Changes by subsystem

### Milestone A — startup·DB·CI

- `src-tauri/src/lib.rs`와 `main.rs`: 앱 single-instance가 두 번째 프로세스의 DB setup을 막고 기존 창을 복원한다. startup 오류를 성공처럼 삼키던 구조를 `Result`와 non-zero exit로 바꾸고, Windows GUI에서도 안정 문구와 `%LOCALAPPDATA%\Atsumi Next\Logs\startup-error.log` 위치를 표시한다.
- `migrations.rs`와 `sqlite_repository.rs`: future version·gap·name mismatch를 어떤 변경보다 먼저 거부한다. 실제 파일 DB의 pending migration은 version/timestamp가 포함된 `VACUUM INTO` snapshot을 먼저 만들며 실패 시 migration하지 않는다. 임시 exclusive lock을 제거하고 WAL·busy timeout·짧은 transaction을 사용한다.
- `tauri.conf.json`과 capability: CSP를 self/blob/data 및 IPC에 필요한 범위로 제한하고 window/event 권한만 허용한다.
- `.github/workflows/windows-ci.yml`과 `tools/verify.ps1`: 로컬/CI가 같은 frozen frontend, Rust fmt/check/test/clippy, Tauri release, whitespace gate를 실행한다.
- 데이터 호환성: schema version은 7로 유지된다. repository open은 더 이상 volatile job을 바꾸지 않으며, single-instance를 획득한 app setup이 startup recovery를 명시적으로 수행한다.

## 4. Contracts and versions

- 앱/package/Tauri version: `0.1.0`
- Rust MSRV: `1.88.0` (working tree)
- DB schema version: 7
- migration: `settings_and_window_placement`, `mock_job_event_foundation`, `gallery_and_artifact_foundation`, `gallery_primary_group`, `download_queue_contract`, `download_queue_response_revision`, `download_lifecycle_and_cancelled_state`
- manifest schema version: 미구현
- HashProfile version: 미구현
- Hitomi parser/resolver: working tree에서 typed contract를 도입 중이며 milestone 완료 시 version을 고정한다.
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
- A/B 통합 working tree: `tools/verify.ps1` 성공 — frontend 89/89, Rust lib 77/77(외부 live smoke 1개 opt-in 제외), startup 1/1, typecheck/build/clippy/Tauri release/whitespace 통과.
- 검증 로그: `.runtime/verification/verify-20260815-165051.log`.
- 외부 Hitomi live smoke는 일반 fixture CI와 분리해 기록한다.
- E2E는 실제 파일을 생성·검증한 결과가 있을 때만 통과로 기록한다.

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
- 생성 commit, push 결과, PR 상태는 각 milestone 완료 시 SHA와 함께 누적한다.
- PR merge, `main` 직접 push, force push, release/tag 생성은 수행하지 않는다.
