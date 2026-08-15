# 전달 계획

## Phase 0: 기준선과 명세

완료했다.

산출물:

- Classic 기준선 감사
- 제품 범위와 기능 행렬
- UX 정보 구조
- 시스템 구조와 데이터 이전 초안
- 사건 이력과 golden fixture 후보
- 사용자 승인 대기 결정

종료 조건:

- Classic 기준선이 Git과 파일 snapshot으로 복원 가능하다.
- 유지, 재설계, 보류 기능이 사용자에게 승인된다.
- 첫 수직 기능의 acceptance criteria가 확정된다.

## Phase 1: UX prototype과 계약

완료했다. 승인된 clickable prototype은 `prototype/`에 보존하고, 구현은 공통 component와 reducer로 옮긴다.

산출물:

- 앱 셸과 Explore clickable prototype
- Downloads 상태 및 오류 detail prototype
- Detail tab prototype
- 작품/내부 중복 Review prototype
- `API_CONTRACT_V2.md`
- `UX_INTERACTION_MATRIX.md`
- `ERROR_CATALOG.md`

종료 조건:

- 핵심 8개 작업을 mock data로 수행할 수 있다.
- 사용자가 기능 위치와 상호작용을 승인한다.
- backend command와 event payload가 type으로 확정된다.

## Phase 2: Core foundation

완료했다. 앱 셸, SQLite 기반 설정·창 배치, Gallery/Artifact model, typed command client, revision event projection, fixture event foundation, structured logging을 구현하고 검증했다. 실제 원격 기능은 Phase 3 command로 확장한다.

산출물:

- 새 Tauri workspace와 React frontend
- SQLite migration runner
- Gallery, DownloadJob, Artifact domain model
- typed command client
- event stream과 Activity Center foundation
- structured logging

종료 조건:

- 앱 셸이 실행된다.
- 설정과 window placement가 SQLite에서 저장 및 복원된다.
- fixture event의 상태 변경이 전체 목록 재렌더 없이 표시된다.

## Phase 3: 첫 수직 기능

진행 중이다. 저장 fixture로 고정한 Hitomi source parser를 실제 metadata 검색·상세·WebP 미리보기 read adapter에 연결했고, requestId/active-gallery 멱등성을 가진 다운로드 queue/list, revision snapshot, startup `interrupted` 복구 기반을 구현했다. retry/cancel, attempt 이력과 `cancelled` 상태도 SQLite 계약으로 연결했다. Explore/Downloads/Detail/Review는 하나의 전역 thumbnail coordinator를 사용하며 in-flight 병합·우선순위·취소·메모리/negative cache를 공유한다. 검색과 미리보기는 같은 HTTP pool·host gate·metadata/gg cache를 사용한다. production mock 완료 경로는 제거했으며 신규 queue는 실제 artifact pipeline이 없으면 `interrupted`에서 멈춘다. artifact 처리와 파일 열기는 이어서 연결한다.

범위:

`Explore -> Detail -> Queue -> Download -> Resume -> Complete -> Open`

필수 검증:

- Recent와 tag 검색 fixture
- 20, 50, 200개 카드 resize와 scroll
- 같은 ID queue 멱등성
- 단일 및 복수 gallery 다운로드
- HTTP 404, 503, timeout 재시도
- 다운로드 중 강제 종료와 복구
- 실제 파일과 DB reconciliation
- Windows 기본 viewer 열기

## Phase 4: Auto Find와 운영 UX

- 즐겨찾기와 검색 기록 import
- 즐겨찾기 작가 갱신
- 작가별 그룹
- Downloads 상태 filter
- 오류 detail과 재시도
- cache/data cleanup 범위

## Phase 5: 작품 중복

- candidate evidence model
- E-Hentai relation adapter
- staged image containment scan
- Review workspace
- 숨김, 연작, 오탐 decision transaction
- golden positive/negative suite

## Phase 6: 내부 페이지 중복

- local artifact hash index
- gap-tolerant scene block detection
- synchronized comparison rows
- selection preview
- quarantine apply와 undo
- 재다운로드 및 무결성 예외

## Phase 7: Classic import와 전환

- dry-run inventory
- state, folder, hash import
- conflict report
- 사용자 승인 후 Next profile 활성화
- Classic과 병행 사용 기간
- rollback 검증

## 작업 규칙

- 각 Phase는 실행 가능한 얇은 결과를 남긴다.
- 승인되지 않은 UX를 production component로 만들지 않는다.
- remote site 응답에 의존하는 test는 fixture test와 분리한다.
- 문제 수정은 Incident와 regression test를 같이 남긴다.
- Classic에 필요한 최소 수정은 별도 승인과 별도 commit으로 격리한다.

## 다음 즉시 작업

1. 완료: `search_submit`, `search_page_get`, `gallery_detail_get` adapter, metadata/thumbnail DTO, App projection과 저장 fixture.
2. 완료: 같은 gallery ID의 queue 멱등성, revision snapshot, 다운로드 상태 복구 기반과 production mock 완료 경로 제거.
3. 완료: 중앙 상태 전이, attempt/error/timestamp schema, `download_retry`/`download_cancel`, `cancelled`와 batch/CAS 회귀 검증.
4. 완료: 공용 thumbnail key/component와 프로세스 전역 coordinator, priority, in-flight dedupe, 취소, 성공/실패 cache 기반.
5. 완료: 실제 Hitomi metadata/thumbnail resolver, 공용 HTTP gate, 저장 fixture 기반 404/429/503/timeout 정책과 이미지 안전 검증.
6. 완료: 앱 내부 single-instance 계약과 migration 전 SQLite backup/future-schema 거부. repository open은 WAL·busy timeout을 사용하고 job 복구는 single-instance를 획득한 app setup에서만 명시적으로 실행한다.
7. 정식 queue runner와 artifact 저장·검증·resume·reconcile, 첫 이미지 열기를 연결한 뒤 Phase 3 E2E를 수행한다.
