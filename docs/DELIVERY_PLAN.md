# 전달 계획

## Phase 0: 기준선과 명세

현재 단계다.

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
- mock job의 상태 변경이 전체 목록 재렌더 없이 표시된다.

## Phase 3: 첫 수직 기능

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

1. 사용자에게 `DECISION_REGISTER.md`의 D-101~D-110 승인을 받는다.
2. 승인 후 Classic 보존 commit 또는 tag를 만든다.
3. UX interaction matrix와 저해상도 prototype을 만든다.
4. 동시에 API contract와 SQLite schema 초안을 만든다.
5. 두 문서가 합의되면 빈 scaffold를 생성한다.
