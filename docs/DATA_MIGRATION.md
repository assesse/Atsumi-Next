# 데이터 소유권과 이전

## 기본 결정

새 버전의 영속 데이터 기준은 SQLite 하나로 통합한다. 파일 시스템은 artifact의 실제 존재를 증명하며, DB와 불일치하면 reconciliation job이 해결한다.

## 현재 구현 schema (v20)

| 테이블 | 책임 |
|---|---|
| `settings` | versioned 사용자 설정; v15 folder template, v17 Auto Find history mode, v19 Related galleries preview width 포함 |
| `window_placement` | 창 위치·크기 revision snapshot |
| `galleries` | 정규화된 gallery metadata snapshot, v18 source revision 문자열 identity와 작은 내부 revision |
| `download_entries` | 사용자가 관리하는 다운로드 항목 |
| `download_jobs` | 영속 job과 현재 단계 |
| `download_attempts` | job attempt와 종료 상태·오류 기록 |
| `download_queue_requests`·`download_queue_request_entries` | idempotent queue 응답 snapshot |
| `download_artifacts` | gallery artifact·manifest revision, immutable `relative_directory`와 `root_snapshot` |
| `download_pages` | source page와 local artifact 상태 |
| `download_page_attempts` | job attempt 안의 source page/candidate 형식·HTTP/content-type·retryability 진단 |
| `quarantine_records` | 원본·격리 상대 경로와 crash-safe move/restore saga |
| `favorites` | 작가·그룹·시리즈·캐릭터·태그의 정규화 key와 revision |
| `tag_catalog_entries`·`tag_catalog_state` | 수동 최신화한 Hitomi tag/female/male catalog와 attempt/success 상태. 전체 교체는 단일 transaction이라 실패 시 이전 catalog를 유지한다. |
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
| `classic_import_runs`·`classic_import_artifact_copies`·`classic_import_changes`·`classic_import_legacy_hashes` | v14에서 생성된 역사적 호환 table. runtime command·repository·UI가 접근하지 않으며 기존 DB를 파괴적으로 다시 쓰지 않기 위해 DDL만 보존 |
| `schema_migrations` | migration 적용 이력 |

아래 테이블은 향후 검토할 계획 schema다.

| 계획 테이블 | 책임 |
|---|---|
| `gallery_tags` | namespace가 보존된 tag 관계 |
| `thumbnail_cache_entries` | cache index와 접근 시각 |

정확한 현재 DDL과 CHECK/FK는 `src-tauri/src/infrastructure/migrations.rs`가 기준이다. 기존 migration의 이름·순서·column 의미는 바꾸지 않고 additive migration만 추가한다.

### v10 추가 규칙

- migration 이름은 `favorites_search_history_and_auto_find`다.
- 즐겨찾기는 `(namespace, value)`가 primary key이며 값은 application 경계에서 trim·소문자·공백 정규화한다. 별도의 frontend memory 목록을 canonical source로 사용하지 않는다.
- 검색 이력은 성공한 non-empty 제출만 SHA-256 fingerprint로 합치며 text, include/exclude tags, language, sort, page size를 함께 보존한다.
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
- 이 migration과 네 table은 이미 적용된 DB의 version/name/checksum 연속성을 위해 그대로 둔다. 현재 runtime에는 관련 command, repository, source adapter, UI가 없고 새 row를 생성하거나 기존 row를 해석하지 않는다.
- table drop이나 과거 migration 편집으로 기존 DB를 재작성하지 않는다. v1~v13 의미와 manifest/HashProfile version을 바꾸지 않는 additive migration이라는 역사적 사실만 유지한다.

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
- v14→latest migration 회귀 테스트는 v15/v16/v17/v18/v19의 순서, 역사적 v14 table 보존, legacy relative directory 보존, `root_snapshot` backfill과 두 경로의 immutability를 검증한다.

### v18 추가 규칙

- migration 이름은 `gallery_source_revision_identity`다.
- `galleries.source_revision TEXT`를 nullable additive column으로 추가한다. 값이 있으면 1~512 bytes여야 한다.
- remote source의 unsigned fingerprint는 이 문자열 identity에 저장한다. signed SQLite `galleries.revision`은 내부 snapshot revision으로만 사용하므로 `u64`→`i64` 변환 오버플로가 없다.
- 기존 row의 source identity는 추측하지 않고 `NULL`로 둔다. 다음 실제 metadata 계획에서 identity를 저장하며 identity가 달라질 때만 내부 revision을 증가시킨다.

### v19 추가 규칙

- migration 이름은 `related_gallery_preview_preference`다.
- `settings.related_preview_width`는 180~320px의 고정 preset(20px 단위)만 허용하며 기본값은 240px이다. Explore·Downloads의 `preview_width`와 독립적으로 Floating Detail의 Related galleries cover에만 적용한다.

## 유지보수 데이터 초기화

- thumbnail cache clear는 완료된 재생성 가능 cache만 제거한다. DB artifact/page와 실제 파일에는 쓰지 않는다.
- exploration reset은 정확한 확인 literal을 받은 뒤 `favorites`, `search_history`, `auto_find_runs`와 그 candidate/cutoff/truncation, `auto_find_exclusions`만 한 transaction에서 삭제한다.
- active Auto Find run이 있으면 transaction 전에 `OPERATION_ACTIVE`로 거부한다. 따라서 일부만 지워진 상태가 생기지 않는다.
- download entry/job/attempt/page, gallery/artifact, duplicate 판정, quarantine, manifest와 실제 파일은 초기화 대상이 아니다.

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

- Next가 생성한 manifest는 schema와 writer version을 가진다.
- schema v15~v19 downgrade는 지원하지 않는다. 오래된 binary는 future-schema를 쓰기 전에 거부하며 실제 downgrade는 migration 전 backup과 호환 binary를 함께 복원해야 한다.
- 운영 DB에 과거 migration table/column을 수동 삭제하거나 migration history를 편집하지 않는다. 복구는 migration 전 일관 backup과 호환 binary를 함께 사용한다.

quarantine에는 자동 보존 만료가 없다. 사용자가 명시적으로 복원하거나, 별도 재확인을 거친 비우기 기능을 실행하기 전까지 파일을 유지한다.
