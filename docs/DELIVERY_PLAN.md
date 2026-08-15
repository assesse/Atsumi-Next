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

완료했다. 실제 Hitomi read adapter와 공용 HTTP scheduler를 검색·상세·미리보기·페이지 다운로드에 연결했다. requestId/active-gallery 멱등 queue, retry/cancel과 attempt 이력, bounded gallery worker, source page별 checkpoint를 SQLite에 영속한다. 페이지는 bounded body read와 `.part` write, MIME/signature/decode 검증, WebP 정규화, SHA-256, atomic rename을 거치며 versioned manifest와 DB snapshot이 모두 맞을 때만 완료된다. 시작 시 active job을 `interrupted`로 바꾼 뒤 verified checkpoint부터 재개하고, startup/manual reconcile은 누락·hash·manifest 문제를 안전 상태로 표시한다. 완료 첫 페이지는 canonical root 확인 뒤 Windows 기본 viewer로 열며, 제거는 crash-safe quarantine saga와 undo로 처리하고 자동 영구 삭제하지 않는다.

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

검증 근거:

- synthetic 실제 PNG 입력을 WebP·SHA-256·manifest로 완료하는 filesystem/SQLite 통합 테스트
- 두 번째 페이지 중단 뒤 첫 verified page를 다시 받지 않는 resume 테스트
- 파일 이동과 DB commit 사이 강제 종료를 재조정하는 quarantine fault-injection 테스트
- quarantine/undo 후 manifest path·상태와 실제 폴더 복원 테스트

## Phase 4: Auto Find와 운영 UX

Milestone D의 즐겨찾기·검색 이력·Auto Find workflow를 실제 SQLite와 production source adapter에 연결했다. Classic 데이터 import는 원본을 읽는 Phase 7 workflow 전에는 수행하지 않는다.

구현 범위:

- 작가·그룹·시리즈·캐릭터·태그 즐겨찾기 영속, namespace 검색 serializer와 카드·상세·Related의 공통 favorite projection
- 성공한 명시적 검색 이력과 최근 suggestion 복원
- 사용자가 시작하는 `즐겨찾기 작가 갱신`; 입력 중 원격 요청 없음
- run·후보·진행률·취소·오류의 영속 상태와 restart snapshot 복원
- 이미 download entry가 있거나 명시적으로 제외한 gallery의 후보 제거
- 전체/작가별 보기와 결과 문자열·언어 local filter
- 선택 또는 현재 filter 결과의 기존 download queue 일괄 추가
- source 실패·중단·취소 상태 표시와 명시적 재갱신

현재 경계:

- source pagination은 전체 기간을 순회하되 한 작가당 250 page 안전 상한을 둔다.
- 작품 숨김·resolved duplicate decision·pair 제외는 schema v12의 canonical record를 Auto Find 후보 조건에 결합한다.
- Classic 즐겨찾기·검색 기록 import와 충돌 보고는 Phase 7 read-only workflow로 완료했다.
- 최종 운영 polish는 구현된 설정만 노출하고, cache purge·영구 삭제처럼 안전 계약이 없는 작업을 설명 있는 disabled 상태로 고정했다.

## Phase 5: 작품 중복

완료했다. 검증된 최신 local artifact만 대상으로 versioned SHA/perceptual/detail/edge evidence를 만들고, metadata-prioritized exhaustive pair worklist와 monotonic one-to-one gap alignment로 exact·contains·partial·translation 후보를 저장한다. scan 진행률·취소·앱 종료/비정상 종료 복구, candidate revision CAS와 append-only 판정 이력, 숨김·양쪽 연작 연결/해제·pair 제외를 SQLite transaction으로 처리한다. 대형 Review는 전역 thumbnail coordinator의 root-bound `artifactPage`로 정확한 원본 page pair와 근거를 표시한다. blank/B&W, 실제 장면 변화, 일부 공통 panel 오판 금지와 재압축·해상도/번역형 positive를 회귀 테스트로 고정했다. 자동 파일 삭제는 없다. E-Hentai relation은 port와 evidence type만 두고 명시적 session이 없는 production에서는 비활성화한다.

## Phase 6: 내부 페이지 중복

완료했다. 작품 중복과 같은 verified local page hash cache를 재사용하되 한 artifact 안에서만 exact 반복 또는 최소 2행의 단조 gap-tolerant visual scene block을 만든다. synchronized Review는 실제 `artifactPage(entryId, sourcePage)`를 표시하고 각 행에서 유지할 원본 page를 선택한다. revision CAS와 현재 byte snapshot으로 removal plan을 먼저 고정하며, 적용은 page별 pending DB intent → artifact 내부 quarantine move → manifest atomic replace → page/artifact/group/plan transaction 순서다. undo는 원래 relative path와 immutable source page number를 복원한다. 앱 종료가 filesystem/DB 사이에 발생해도 시작 시 pending saga를 재개하며 모호한 경로는 덮어쓰거나 삭제하지 않는다. 자동 영구 삭제는 없다.

## Phase 7: Classic import와 전환

완료했다. 사용자가 고른 Classic data/download root에서 state, 선택적 localStorage export, manifest, numbered page, quarantine과 legacy hash DB를 읽기 전용으로 조사한다. gallery ID 연결과 typed conflict, eligible copy byte를 revisioned dry-run으로 저장하고 모든 경고와 최종 적용을 명시적으로 승인받는다. eligible page는 apply 직전에 source fingerprint·SHA·length·decode를 다시 검사한 뒤 Next `gallery-{id}`에 WebP 복사·manifest 검증하고 metadata와 함께 transaction으로 등록한다. rollback은 이 import의 Next copy만 관리 quarantine으로 옮기고 journal에 기록된 새 DB row만 제거한다. 중단된 apply/rollback은 시작 시 안전하게 수렴한다. Classic 원본은 이동·수정·삭제하지 않는다.

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
7. 완료: 정식 queue runner와 artifact 저장·검증·resume·reconcile, 첫 이미지 열기와 Phase 3 filesystem/SQLite 통합 검증.
8. 완료: 영속 즐겨찾기·검색 이력, 명시적 작가 갱신, Auto Find 진행·취소·복원과 local grouping/filter/batch queue 연결.
9. 완료: 실제 artifact evidence를 사용하는 작품 단위 중복 후보·판정·Review와 Auto Find decision 제외 연동.
10. 완료: 완료 artifact 내부의 반복 장면 block, synchronized source-page Review, removal plan, page quarantine·undo와 manifest/DB 일관성.
11. 완료: Classic read-only inventory, typed dry-run conflict report, 승인형 verified copy/import, crash recovery와 Next-only rollback.
12. 완료: 최종 운영 polish, 실제 page 확대, dialog focus 복원, production fixture fallback 제거, 사용자 오류와 내부 detail 분리, launcher/startup log redaction, dependency·third-party 고지와 일치 version 검증.
13. 최종 전달: 전체 Windows 검증, Git push와 PR 갱신, 실제 Tauri 앱 수동 검토를 수행한다.
