# 데이터 소유권과 이전

## 기본 결정 초안

새 버전의 영속 데이터 기준은 SQLite 하나로 통합한다. 파일 시스템은 artifact의 실제 존재를 증명하며, DB와 불일치하면 reconciliation job이 해결한다.

## 후보 스키마

| 테이블 | 책임 |
|---|---|
| `settings` | versioned 사용자 설정 |
| `galleries` | 정규화된 gallery metadata snapshot |
| `gallery_tags` | namespace가 보존된 tag 관계 |
| `favorites` | 작가, 그룹, 시리즈, 캐릭터, 태그 즐겨찾기 |
| `search_history` | 전체 검색 기록과 최근 사용 시각 |
| `download_entries` | 사용자가 관리하는 다운로드 항목 |
| `download_jobs` | 영속 job과 현재 단계 |
| `download_attempts` | 페이지별 URL, 오류, retry 기록 |
| `download_pages` | source page와 local artifact 상태 |
| `page_hashes` | versioned SHA-256, dHash, pHash |
| `duplicate_candidates` | 후보쌍과 생성 상태 |
| `duplicate_evidence` | 관계, 제목, 해시, 순서 근거 |
| `duplicate_decisions` | 사용자 판정 이력 |
| `series_groups` | 함께 처리할 연작 그룹 |
| `internal_review_blocks` | 갤러리 내부 장면 토막 |
| `page_exclusions` | 사용자가 제거한 원본 페이지 |
| `quarantine_entries` | 복구 가능한 파일 이동 기록 |
| `thumbnail_cache_entries` | cache index와 접근 시각 |
| `schema_migrations` | migration 적용 이력 |

정확한 DDL은 domain 모델 승인 후 작성한다.

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

사용자 기본 구조는 유지한다.

```text
D:\Atsumi\[artist] title\
  001.webp
  002.webp
  info.txt
  .atsumi-download.json
  .atsumi-page-selection.json
```

폴더명을 만드는 canonical 함수는 backend 한 곳에만 둔다. 프론트는 최종 경로를 계산하지 않는다.

## rollback

- Next는 Classic 원본 DB와 state를 수정하지 않는다.
- import 시 모든 파일 작업은 dry-run report를 먼저 만든다.
- 실제 다운로드 파일은 import를 위해 이동하지 않는다.
- Next 전용 DB를 삭제하면 Classic으로 돌아갈 수 있어야 한다.
- Next가 생성한 새 manifest는 schema와 writer version을 가진다.

## 승인 필요

1. Next 첫 실행에서 Classic 데이터를 자동 발견만 할지, import 안내를 바로 열지
2. Classic localStorage export를 Classic 코드에 최소 변경으로 추가해도 되는지
3. quarantine 기본 보존 기간
4. 완료 폴더인데 manifest가 없는 기존 자료를 얼마나 적극적으로 가져올지
