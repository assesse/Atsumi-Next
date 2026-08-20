# 데이터 소유권과 이전

## 기본 결정

새 버전의 영속 데이터 기준은 SQLite 하나로 통합한다. 파일 시스템은 artifact의 실제 존재를 증명하며, DB와 불일치하면 reconciliation job이 해결한다.

## 현재 구현 schema (v17)

| 테이블 | 책임 |
|---|---|
| `settings` | versioned 사용자 설정; v15 folder template, v17 Auto Find history mode 포함 |
| `window_placement` | 창 위치·크기 revision snapshot |
| `galleries` | 정규화된 gallery metadata snapshot |
| `download_entries` | 사용자가 관리하는 다운로드 항목 |
| `download_jobs` | 영속 job과 현재 단계 |
| `download_attempts` | job attempt와 종료 상태·오류 기록 |
| `download_queue_requests`·`download_queue_request_entries` | idempotent queue 응답 snapshot |
| `download_artifacts` | gallery artifact·manifest revision, immutable `relative_directory`와 `root_snapshot` |
| `download_pages` | source page와 local artifact 상태 |
| `download_page_attempts` | job attempt 안의 source page/candidate 형식·HTTP/content-type·retryability 진단 |
| `quarantine_records` | 원본·격리 상대 경로와 crash-safe move/restore saga |
| `favorites` | 작가·그룹·시리즈·캐릭터·태그의 정규화 key와 revision |
| `search_history` | 성공한 명시적 검색의 전체 정규화 요청 fingerprint, 사용 횟수와 최근 시각 |
| `auto_find_runs` | 명시적 작가 갱신의 상태·revision·진행률·안전 오류 |
| `auto_find_candidates` | run별 `GallerySummary` snapshot(시리즈·캐릭터 포함)과 일치한 즐겨찾기 key |
| `auto_find_exclusions` | gallery ID별 영속 후보 제외와 이유 |
| `owned_gallery_artists` | 검증 artifact의 보수적 작가 소유 증거 |
| `auto_find_run_cutoffs` | 작가별 oldest gallery, qualified count, 고정 source/policy version |
| `auto_find_run_truncations` | cutoff 뒤 candidate limit 초과 증거 |
| `duplicate_hash_profiles` | versioned 작품 중복 hash·threshold 계약 |
| `duplicate_page_hashes` | 검증 artifact page의 SHA-256·coarse/detail dHash·pHash·분산/edge feature cache |
| `duplicate_scan_runs` | full scan 상태·revision·hash/pair 진행률·안전 오류 |
| `duplicate_candidates` | profile별 gallery pair, relation, confidence, coverage와 해결 상태 |
| `duplicate_evidence`·`duplicate_page_pairs` | typed 근거와 immutable source page one-to-one 정렬 |
| `duplicate_hidden_galleries`·`duplicate_pair_exclusions` | 사용자 숨김과 오탐 pair 제외 |
| `duplicate_series_groups`·`duplicate_series_members` | 원자적으로 관리되는 연작 묶음 |
| `duplicate_decisions` | candidate revision별 append-only 사용자 판정 이력 |
| `internal_duplicate_runs` | 앨범 내부 page scan 상태·revision·진행률·오류 |
| `internal_duplicate_groups`·`internal_duplicate_group_pages` | immutable source page 기반 synchronized scene row와 근거 |
| `internal_removal_plans` | group revision과 파일 수·byte 합계를 고정한 만료형 격리 계획 |
| `page_quarantine_records` | page별 원본·격리 상대 경로와 crash-safe move/restore saga |
| `classic_import_runs` | Classic read-only inventory, fingerprint, revisioned report와 apply/rollback 상태 |
| `classic_import_artifact_copies` | 첫 write 전 기록하는 Next 복사 목적지와 copy/격리 상태 |
| `classic_import_changes` | 이 import가 새로 만든 DB 변경의 역순 rollback journal |
| `classic_import_legacy_hashes` | 신뢰 판정에 사용하지 않는 Classic hash row 수 provenance |
| `schema_migrations` | migration 적용 이력 |

아래 테이블은 Phase 7 이후 검토할 계획 schema다.

| 계획 테이블 | 책임 |
|---|---|
| `gallery_tags` | namespace가 보존된 tag 관계 |
| `thumbnail_cache_entries` | cache index와 접근 시각 |

정확한 현재 DDL과 CHECK/FK는 `src-tauri/src/infrastructure/migrations.rs`가 기준이다. 기존 migration의 이름·순서·column 의미는 바꾸지 않고 additive migration만 추가한다.

### v10 추가 규칙

- migration 이름은 `favorites_search_history_and_auto_find`다.
- 즐겨찾기는 `(namespace, value)`가 primary key이며 값은 application 경계에서 trim·소문자·공백 정규화한다. 별도의 frontend memory 목록을 canonical source로 사용하지 않는다.
- 검색 이력은 성공한 non-empty 제출만 SHA-256 fingerprint로 합치며 text, include/exclude tags, language, sort, page size를 함께 보존한다. Classic 검색 기록은 Phase 7 read-only dry-run에 포함되고 사용자가 승인한 신규 row만 적용·rollback journal로 추적한다.
- Auto Find는 동시에 하나의 `running` row만 허용한다. run·후보·전역 gallery 제외는 재시작 뒤에도 유지되고, snapshot은 최신 run의 후보 중 현재 download entry 또는 명시적 제외가 없는 항목만 반환한다.
- startup에서 남은 `running` run은 파일이나 후보를 삭제하지 않고 `failed/AUTO_FIND_INTERRUPTED`로 종결한다. 부분 후보는 증거로 보존된다.
- `auto_find_candidates`는 원격 source의 당시 metadata snapshot이다. 실제 다운로드 artifact나 schema v12 duplicate decision의 canonical record를 대신하지 않으며 snapshot 조회 시 해당 canonical 제외 상태를 결합한다.

### v11 추가 규칙

- migration 이름은 `auto_find_visible_metadata`다.
- `auto_find_candidates.series_json`과 `characters_json`을 `NOT NULL`, valid JSON, 기본값 `[]`로 additive하게 추가한다.
- v10에서 저장된 run과 후보는 그대로 유지하고 새 namespace metadata만 빈 배열로 backfill한다. v10→v11 보존 migration test가 이 조건을 검증한다.
- 새 후보는 source의 `GallerySummary.series`와 `characters`를 저장하며 snapshot 복원 뒤 카드·상세·Related가 같은 metadata와 favorite projection을 사용한다.

### v12 추가 규칙

- migration 이름은 `artifact_duplicate_evidence_and_decisions`다.
- HashProfile 1/algorithm 1의 SHA-256, 64-bit coarse dHash·pHash, 1024-bit detail dHash, luma/variance/non-uniform/edge feature를 artifact SHA와 함께 저장한다. artifact byte hash나 profile version이 바뀌면 cache를 그대로 재사용하지 않는다.
- scan run은 동시에 하나의 `running` row만 허용한다. 앱 시작 시 남은 run은 `failed/DUPLICATE_SCAN_INTERRUPTED`, 정상 종료·사용자 취소는 `cancelled`로 종결하고 이미 저장된 후보·판정은 보존한다.
- 후보·evidence·source page pair 교체, revision CAS 판정, 숨김·연작·pair 제외 side effect와 append-only history는 각각 하나의 SQLite transaction이다.
- `series_link`는 두 gallery를 한 group에 연결하고 후보를 resolve하지 않는다. `hide_*`와 `exclude_pair`만 후보를 resolved 처리하며 Auto Find의 insert/snapshot도 해당 영속 상태를 제외한다.
- v1~v11 table/column 의미는 바꾸지 않는 additive migration이며 migration 전 backup과 future-schema 거부 규칙을 그대로 따른다.

### v13 추가 규칙

- migration 이름은 `internal_scene_review_and_page_quarantine`이다.
- 동시에 하나의 `internal_duplicate_runs.state='running'`만 허용한다. 시작 시 남은 run은 `failed/INTERNAL_SCAN_INTERRUPTED`, 정상 종료·사용자 취소는 `cancelled`로 끝내며 이미 저장된 group과 page 격리 이력은 보존한다.
- 내부 group은 `entry_id`, block/sequence, immutable `source_page_number`, exact/visual evidence와 revision을 저장한다. exact SHA 반복은 한 행을 허용하지만 visual group은 최소 두 개의 단조 행을 통과한 경우에만 생성한다.
- removal plan은 현재 group revision, keep/remove page, 현재 present page의 파일 수와 byte 합계를 고정하고 15분 후 만료한다. apply 시 같은 page를 다른 active plan이 중복 소유하지 못한다.
- `page_quarantine_records`는 `pending_quarantine | quarantined | pending_restore | restored`만 허용한다. 파일 이동 전 DB intent를 commit하고, manifest를 원자 교체한 뒤 page/artifact/group/plan 상태를 한 transaction으로 확정한다.
- 격리 page는 `download_pages.state='quarantined'`, `excluded=1`이지만 source page number와 SHA/byte/format 검증 metadata는 유지한다. undo는 원래 relative path와 `present/excluded=0`을 복원한다.
- v1~v12 table/column 의미, manifest schema 1과 HashProfile 1을 바꾸지 않는 additive migration이다. v12 DB에는 migration 전 일관 backup을 만든다.

### v14 추가 규칙

- migration 이름은 `classic_read_only_import_and_rollback`이다.
- dry-run 원본 절대 경로와 전체 plan은 local Next SQLite에만 저장한다. frontend 보고서는 folder label과 source fingerprint만 받으며 기본 로그에는 경로가 없다.
- artifact copy 목적지는 첫 파일 write 전에 기록한다. apply/rollback이 중단되면 startup recovery가 Next 부분 폴더를 import 전용 quarantine으로 이동하고 상태를 수렴시킨다.
- DB apply는 실제 WebP·SHA·manifest 검증 뒤 한 transaction이며, rollback journal은 이 import가 새로 삽입한 favorite/history/exclusion/hidden/pair/series/artifact만 역순 제거한다. 기존 row는 `INSERT OR IGNORE`와 change journal로 보존한다.
- Classic hash DB는 `READ_ONLY + query_only`로 열고 row 수만 provenance로 저장한다. Next HashProfile duplicate blocking에는 사용하지 않는다.
- v1~v13 의미와 manifest/HashProfile version을 바꾸지 않는 additive migration이며 기존 DB는 migration 전 일관 backup을 만든다.

### v15 추가 규칙

- migration 이름은 `artifact_folder_template_and_immutable_path`다.
- `settings.folder_name_template`을 기본 `[{artist}] {title} [{group}] {id}`로 추가한다. application validation은 빈 값/512 bytes 초과/control 문자/중괄호 오류/알 수 없는 token을 거부하고 `{id}`를 필수로 한다.
- 새 artifact의 folder component는 Windows 금지 문자·reserved device name·trailing dot/space를 제거하고 gallery ID를 보존하면서 component 180 UTF-16 units, 관리 절대 경로 240 UTF-16 units 안으로 제한한다.
- `download_artifacts.relative_directory` update trigger가 기존 artifact 위치 변경을 거부한다. migration은 기존 `gallery-<id>` 또는 다른 legacy 상대 경로를 다시 계산하거나 이름 변경하지 않는다.

### v16 추가 규칙

- migration 이름은 `download_candidate_diagnostics_and_artifact_root_snapshot`이다.
- `download_jobs.last_error_retryable`, `download_attempts.error_retryable`과 `download_page_attempts.candidate_format/http_status/content_type/retryable`을 additive하게 추가한다. candidate 형식은 `unknown|webp|jpeg|png|avif|jxl`이다.
- `download_artifacts.root_snapshot`을 기존 `settings.download_root`로 backfill하고 immutable update trigger로 보호한다. resume/reconcile/Review가 현재 설정값이 아니라 artifact 최초 예약 root를 사용하게 한다.
- v15→v16은 artifact 파일을 이동하거나 manifest 경로를 다시 쓰지 않는다.

### v17 추가 규칙

- migration 이름은 `auto_find_history_cutoff_evidence`다.
- `settings.auto_find_history_mode`와 `auto_find_runs.history_mode`를 `include_all_history|newer_than_oldest_downloaded` enum으로 추가하고 기본값을 `include_all_history`로 둔다. 실행 중 설정 변경은 현재 run에 섞이지 않는다.
- `owned_gallery_artists`는 completed/quarantined entry와 complete/quarantined artifact가 모두 있는 경우만 소유 증거로 인정한다. legacy backfill은 저장된 `galleries.primary_artist`만 사용하며 추가 artist를 추측하지 않는다.
- `auto_find_run_cutoffs`는 `source='verified_owned_artifact'`, `policy_version=1` CHECK를 가지며 작가별 optional oldest ID와 qualified count를 저장한다. 증거가 없으면 cutoff가 없다.
- `auto_find_run_truncations`는 `reason='candidate_limit_after_cutoff'`, eligible count와 limit을 저장한다. 현재 application limit은 cutoff 적용 뒤 50,000 candidate다.
- v14→latest migration 회귀 테스트는 v15/v16/v17의 순서, legacy relative directory 보존, `root_snapshot` backfill과 두 경로의 immutability를 검증한다.

## Classic 입력원

- `AtsumiData/state.json`
- `AtsumiData/state.json.bak`
- browser localStorage export
- `AtsumiData/atsumi_cache.sqlite`
- 다운로드 폴더의 `.atsumi-download.json`
- 다운로드 폴더의 `.atsumi-page-selection.json`
- 실제 `NNN.webp` 파일
- quarantine 폴더

localStorage 값은 Classic 원본을 수정하지 않고 사용자가 별도로 둔 `classic-local-storage-export.json`, `atsumi-localstorage-export.json`, `localStorage-export.json`만 선택적으로 병합한다. 파일이 없으면 state/manifest 기반 안전 항목만 보고한다.

## 이전 순서

1. 사용자가 Classic data root와 선택적 download root를 명시적으로 고른다.
2. state, manifest, 실제 numbered file, quarantine과 hash DB를 읽기 전용으로 inventory하고 source fingerprint를 만든다.
3. gallery ID로 state와 폴더를 연결하고 충돌·eligible 항목·copy byte 수를 보고한다.
4. 사용자가 모든 acknowledgement 경고와 최종 적용 문구를 승인한다.
5. apply 직전에 source fingerprint를 다시 확인한다.
6. eligible page만 다시 SHA/length/decode하고 Next에 WebP로 복사·manifest 검증한다.
7. favorites, 검색 이력, 제외, 숨김, 오탐 pair, resolvable 연작과 완료 artifact를 한 SQLite transaction으로 등록한다.
8. rollback은 Next copy를 격리하고 이 import가 새로 만든 DB row만 제거한다.

## 구현된 충돌 정책

| 충돌 | 기본 처리 |
|---|---|
| UI는 완료, 폴더 없음 | `missing_artifacts`, 사용자에게 재연결/재다운로드 제안 |
| 폴더는 있음, UI 목록 없음 | manifest가 유효하면 import 후보 |
| manifest ID와 Classic state folder mapping 불일치 | blocking, 자동 추측·등록 금지 |
| 파일 수와 expected pages 불일치 | `incomplete`, 자동 완료 금지 |
| hash DB만 존재 | artifact 확인 전 duplicate blocking에 사용하지 않음 |
| 숨김 ID의 파일 존재 | 숨김은 유지하되 파일 처리 여부를 보고 |
| 같은 ID 여러 폴더 | 자동 병합하지 않고 충돌 목록에 표시 |

## 폴더 구조

Next가 새로 만드는 artifact 폴더는 backend가 검증한 `folder_name_template`으로 결정한다. frontend는 최종 경로를 계산하지 않으며 gallery ID token을 항상 포함한다. 아래는 기본 template 예시이고 기존 `gallery-<id>` 폴더는 그대로 유지된다.

```text
D:\Atsumi\[artist] Gallery title [group] 4051027\
  0001.webp
  0002.webp
  manifest.json
D:\Atsumi\.atsumi-quarantine\<record-id>\[artist] Gallery title [group] 4051027\
  0001.webp
  manifest.json
```

폴더명을 만드는 canonical 함수는 backend 한 곳에만 둔다. template 변경은 이후 새 artifact에만 적용되고 기존 `relative_directory`/`root_snapshot`을 자동 rename·move하지 않는다.

## rollback

- Next는 Classic 원본 DB와 state를 수정하지 않는다.
- import 시 모든 파일 작업은 dry-run report를 먼저 만든다.
- 실제 Classic 다운로드 파일은 import를 위해 이동하지 않는다. 검증된 Next 복사본만 만든다.
- rollback은 해당 import가 기록한 Next artifact 폴더만 import 전용 quarantine으로 이동하고, journal에 기록된 새 DB row만 제거한다. 격리본과 Classic 원본을 자동 삭제하지 않는다.
- Next가 생성한 새 manifest는 schema와 writer version을 가진다.
- schema v15~v17 downgrade는 지원하지 않는다. 오래된 binary는 future-schema를 쓰기 전에 거부하며 실제 downgrade는 migration 전 backup과 호환 binary를 함께 복원해야 한다.

## 승인 필요

1. Next 첫 실행에서 Classic 데이터를 자동 발견만 할지, import 안내를 바로 열지
2. Classic localStorage export를 Classic 코드에 최소 변경으로 추가해도 되는지
3. 완료 폴더인데 manifest가 없는 기존 자료를 얼마나 적극적으로 가져올지

quarantine에는 자동 보존 만료가 없다. 사용자가 명시적으로 복원하거나, 별도 재확인을 거친 비우기 기능을 실행하기 전까지 파일을 유지한다.
