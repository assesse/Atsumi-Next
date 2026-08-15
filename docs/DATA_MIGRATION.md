# 데이터 소유권과 이전

## 기본 결정

새 버전의 영속 데이터 기준은 SQLite 하나로 통합한다. 파일 시스템은 artifact의 실제 존재를 증명하며, DB와 불일치하면 reconciliation job이 해결한다.

## 현재 구현 schema (v11)

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
| `favorites` | 작가·그룹·시리즈·캐릭터·태그의 정규화 key와 revision |
| `search_history` | 성공한 명시적 검색의 전체 정규화 요청 fingerprint, 사용 횟수와 최근 시각 |
| `auto_find_runs` | 명시적 작가 갱신의 상태·revision·진행률·안전 오류 |
| `auto_find_candidates` | run별 `GallerySummary` snapshot(시리즈·캐릭터 포함)과 일치한 즐겨찾기 key |
| `auto_find_exclusions` | gallery ID별 영속 후보 제외와 이유 |
| `schema_migrations` | migration 적용 이력 |

아래 테이블은 Phase 5~7에서 추가할 계획 schema다.

| 계획 테이블 | 책임 |
|---|---|
| `gallery_tags` | namespace가 보존된 tag 관계 |
| `page_hashes` | versioned SHA-256, dHash, pHash |
| `duplicate_candidates` | 후보쌍과 생성 상태 |
| `duplicate_evidence` | 관계, 제목, 해시, 순서 근거 |
| `duplicate_decisions` | 사용자 판정 이력 |
| `series_groups` | 함께 처리할 연작 그룹 |
| `internal_review_blocks` | 갤러리 내부 장면 토막 |
| `page_exclusions` | 사용자가 제거한 원본 페이지 |
| `thumbnail_cache_entries` | cache index와 접근 시각 |

정확한 현재 DDL과 CHECK/FK는 `src-tauri/src/infrastructure/migrations.rs`가 기준이다. 기존 migration의 이름·순서·column 의미는 바꾸지 않고 additive migration만 추가한다.

### v10 추가 규칙

- migration 이름은 `favorites_search_history_and_auto_find`다.
- 즐겨찾기는 `(namespace, value)`가 primary key이며 값은 application 경계에서 trim·소문자·공백 정규화한다. 별도의 frontend memory 목록을 canonical source로 사용하지 않는다.
- 검색 이력은 성공한 non-empty 제출만 SHA-256 fingerprint로 합치며 text, include/exclude tags, language, sort, page size를 함께 보존한다. Classic 검색 기록 import는 Phase 7의 read-only workflow 전에는 수행하지 않는다.
- Auto Find는 동시에 하나의 `running` row만 허용한다. run·후보·전역 gallery 제외는 재시작 뒤에도 유지되고, snapshot은 최신 run의 후보 중 현재 download entry 또는 명시적 제외가 없는 항목만 반환한다.
- startup에서 남은 `running` run은 파일이나 후보를 삭제하지 않고 `failed/AUTO_FIND_INTERRUPTED`로 종결한다. 부분 후보는 증거로 보존된다.
- `auto_find_candidates`는 원격 source의 당시 metadata snapshot이다. 이후 실제 다운로드 artifact나 Phase 5 duplicate decision의 canonical record를 대신하지 않는다.

### v11 추가 규칙

- migration 이름은 `auto_find_visible_metadata`다.
- `auto_find_candidates.series_json`과 `characters_json`을 `NOT NULL`, valid JSON, 기본값 `[]`로 additive하게 추가한다.
- v10에서 저장된 run과 후보는 그대로 유지하고 새 namespace metadata만 빈 배열로 backfill한다. v10→v11 보존 migration test가 이 조건을 검증한다.
- 새 후보는 source의 `GallerySummary.series`와 `characters`를 저장하며 snapshot 복원 뒤 카드·상세·Related가 같은 metadata와 favorite projection을 사용한다.

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
