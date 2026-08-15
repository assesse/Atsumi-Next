# 데이터 소유권과 이전

## 기본 결정

새 버전의 영속 데이터 기준은 SQLite 하나로 통합한다. 파일 시스템은 artifact의 실제 존재를 증명하며, DB와 불일치하면 reconciliation job이 해결한다.

## 현재 구현 schema (v9)

| 테이블 | 책임 |
|---|---|
| `settings` | versioned 사용자 설정 |
| `window_placement` | 창 위치·크기 revision snapshot |
| `galleries` | 정규화된 gallery metadata snapshot |
| `download_entries` | 사용자가 관리하는 다운로드 항목 |
| `download_jobs` | 영속 job과 현재 단계 |
| `download_attempts` | job attempt와 종료 상태·오류 기록 |
| `download_queue_requests`·`download_queue_request_entries` | idempotent queue 응답 snapshot |
| `download_artifacts` | gallery artifact·manifest revision |
| `download_pages` | source page와 local artifact 상태 |
| `download_page_attempts` | job attempt 안의 source page/candidate 결과 |
| `quarantine_records` | 원본·격리 상대 경로와 crash-safe move/restore saga |
| `schema_migrations` | migration 적용 이력 |

아래 테이블은 Phase 4~7에서 추가할 계획 schema다.

| 계획 테이블 | 책임 |
|---|---|
| `gallery_tags` | namespace가 보존된 tag 관계 |
| `favorites` | 작가, 그룹, 시리즈, 캐릭터, 태그 즐겨찾기 |
| `search_history` | 전체 검색 기록과 최근 사용 시각 |
| `page_hashes` | versioned SHA-256, dHash, pHash |
| `duplicate_candidates` | 후보쌍과 생성 상태 |
| `duplicate_evidence` | 관계, 제목, 해시, 순서 근거 |
| `duplicate_decisions` | 사용자 판정 이력 |
| `series_groups` | 함께 처리할 연작 그룹 |
| `internal_review_blocks` | 갤러리 내부 장면 토막 |
| `page_exclusions` | 사용자가 제거한 원본 페이지 |
| `thumbnail_cache_entries` | cache index와 접근 시각 |

정확한 현재 DDL과 CHECK/FK는 `src-tauri/src/infrastructure/migrations.rs`가 기준이다. 기존 migration의 이름·순서·column 의미는 바꾸지 않고 additive migration만 추가한다.

## Classic 입력원

- `AtsumiData/state.json`
- `AtsumiData/state.json.bak`
- browser localStorage export
- `AtsumiData/atsumi_cache.sqlite`
- 다운로드 폴더의 `.atsumi-download.json`
- 다운로드 폴더의 `.atsumi-page-selection.json`
- 실제 `NNN.webp` 파일
- quarantine 폴더

localStorage는 Tauri WebView 내부 저장소이므로 import 전용 export command 또는 Classic 내보내기 도구가 필요하다.

## 이전 순서

1. Classic 데이터의 읽기 전용 snapshot을 만든다.
2. manifest와 실제 파일을 먼저 inventory한다.
3. state의 Gallery 목록과 폴더를 gallery ID로 연결한다.
4. favorites, 제외, 연작, 오탐 pair를 decision record로 변환한다.
5. Classic 해시 DB를 새 HashProfile version과 함께 import한다.
6. 실제 파일과 DB를 reconcile한다.
7. 변환 보고서를 사용자에게 보여준다.
8. 사용자가 승인하면 Next profile을 활성화한다.

## 충돌 정책 초안

| 충돌 | 기본 처리 |
|---|---|
| UI는 완료, 폴더 없음 | `missing_artifacts`, 사용자에게 재연결/재다운로드 제안 |
| 폴더는 있음, UI 목록 없음 | manifest가 유효하면 import 후보 |
| manifest ID와 폴더명 불일치 | ID를 우선하고 폴더명은 표시 정보로 취급 |
| 파일 수와 expected pages 불일치 | `incomplete`, 자동 완료 금지 |
| hash DB만 존재 | artifact 확인 전 duplicate blocking에 사용하지 않음 |
| 숨김 ID의 파일 존재 | 숨김은 유지하되 파일 처리 여부를 보고 |
| 같은 ID 여러 폴더 | 자동 병합하지 않고 충돌 목록에 표시 |

## 폴더 구조

Next가 새로 만드는 관리 구조는 gallery ID로 결정론적으로 고정한다. 사용자 제목이나 frontend 문자열로 경로를 만들지 않는다.

```text
D:\Atsumi\gallery-4051027\
  0001.webp
  0002.webp
  manifest.json
D:\Atsumi\.atsumi-quarantine\<record-id>\gallery-4051027\
  0001.webp
  manifest.json
```

폴더명을 만드는 canonical 함수는 backend 한 곳에만 둔다. 프론트는 최종 경로를 계산하지 않는다.

## rollback

- Next는 Classic 원본 DB와 state를 수정하지 않는다.
- import 시 모든 파일 작업은 dry-run report를 먼저 만든다.
- 실제 다운로드 파일은 import를 위해 이동하지 않는다.
- Classic으로 돌아갈 때는 Next 전용 profile을 사용하지 않는다. Next DB와 새 `gallery-{id}` artifact를 자동 삭제하지 않으며, 제거가 필요하면 별도 백업·사용자 확인을 거친다.
- Next가 생성한 새 manifest는 schema와 writer version을 가진다.

## 승인 필요

1. Next 첫 실행에서 Classic 데이터를 자동 발견만 할지, import 안내를 바로 열지
2. Classic localStorage export를 Classic 코드에 최소 변경으로 추가해도 되는지
3. 완료 폴더인데 manifest가 없는 기존 자료를 얼마나 적극적으로 가져올지

quarantine에는 자동 보존 만료가 없다. 사용자가 명시적으로 복원하거나, 별도 재확인을 거친 비우기 기능을 실행하기 전까지 파일을 유지한다.
