# 결정 기록

## 상태

- `확정`: 사용자 결정 또는 보존 원칙으로 승인됨
- `제안`: 구현 전에 사용자 승인이 필요함
- `조사`: prototype 또는 기술 spike가 필요함
- `보류`: 현재 milestone에서 결정하지 않음

## 확정

| ID | 결정 | 근거 |
|---|---|---|
| D-001 | Atsumi Classic을 유지하고 Atsumi Next를 별도로 만든다. | 사용자 승인 |
| D-002 | 코드를 먼저 작성하지 않고 현재 기능, UX, 문제 이력을 명세한다. | 사용자 승인 |
| D-003 | 새 UI는 기능 위치와 사용자 작업 흐름을 다시 설계한다. | 사용자 승인 |
| D-004 | 자동 다운로드는 queue 추가 후 시작하되 최종 파일 삭제는 자동 확정하지 않는다. | 최신 요구사항 |
| D-005 | 다운로드 파일은 기본적으로 WebP를 사용한다. | 최신 요구사항과 Classic 동작 |
| D-006 | 작품 중복과 앨범 내부 페이지 중복을 별도 workflow로 취급한다. | 최신 UI 방향 |
| D-007 | 내부 페이지 제거 후 원본 페이지 번호를 유지한다. | 최신 구현과 무결성 규칙 |
| D-101 | Desktop stack은 Tauri 2와 Rust를 유지한다. | 2026-08-12 전체 승인 |
| D-102 | Frontend는 TypeScript와 React로 재작성한다. | 2026-08-12 전체 승인 |
| D-103 | SQLite를 영속 상태의 canonical source로 사용한다. | 2026-08-12 전체 승인 |
| D-104 | Classic command는 새 domain의 기준이 아니라 import/compatibility adapter로만 사용한다. | 2026-08-12 전체 승인 |
| D-105 | 최상위 navigation은 Explore, Auto Find, Downloads 3개를 유지한다. | 2026-08-12 전체 승인 |
| D-106 | Review는 Downloads의 검토 filter와 context action에서 진입한다. | 2026-08-12 전체 승인 |
| D-107 | 어두운 compact rail과 밝은 작업 영역의 identity를 유지하고 controls를 재설계한다. | 2026-08-12 전체 승인 |
| D-108 | 파일 삭제는 quarantine을 기본으로 하고 undo 가능한 기간을 둔다. | 2026-08-12 전체 승인 |
| D-109 | 첫 milestone은 검색부터 다운로드 복구와 파일 열기까지다. | 2026-08-12 전체 승인 |
| D-110 | 실제와 유사한 mock data의 UI prototype을 구현 전에 승인한다. | 2026-08-12 전체 승인 |
| D-111 | 카드의 작가·그룹은 캡슐 control이 아니라 namespace 아이콘이 있는 좌측 정렬 바이라인으로 표시하고 없는 값은 생략한다. | 2026-08-12 사용자 피드백 |
| D-112 | 카드 더블클릭은 Explore/Auto Find에서 상세를 열고 Downloads에서는 해당 artifact를 실행한다. | 2026-08-12 사용자 피드백 |
| D-113 | 상세 workspace는 Classic 기준 1120px 폭을 출발점으로 유지하되, 앱 창이 충분히 넓으면 연속적으로 확장하여 최대 1860px까지 사용한다. | 2026-08-13 사용자 피드백 및 후속 조정 |
| D-114 | 최대 열 수는 고정 열 수가 아니라 상한이며, 실제 container 폭과 미리보기 폭이 부족하면 더 적은 열을 사용한다. | 2026-08-12 사용자 피드백 |
| D-115 | 상세 대표·페이지 preview는 fixture/실제 source가 제공한 이미지 비율을 사용하고 잘린 중첩 viewport에 가두지 않으며 Related 표지는 식별 가능한 크기로 제공한다. 실제 source별 width/height는 Phase 3 thumbnail 계약에 포함한다. | 2026-08-12 사용자 피드백 |
| D-116 | 별도 선택 mode 상태를 만들지 않는다. Ctrl/Shift 또는 선택 개수로 선택 문맥을 파생하며, 문맥 안의 일반 좌클릭은 대상 하나로 교체하고 Ctrl만 toggle, Shift만 범위 선택으로 동작한다. | 2026-08-12 사용자 승인 |
| D-117 | 국가 아이콘은 Classic이 실제 UI에서 사용한 FlagCDN KR/JP/US PNG를 byte-identical 로컬 자산으로 묶는다. 중국어는 런타임 네트워크 요청 없는 로컬 CN SVG와 텍스트 fallback을 사용한다. | 2026-08-13 사용자 승인 및 2026-08-20 로컬 CN 보완 |
| D-118 | 구현 검토 단계에서는 MSI/Setup 패키징을 반복하지 않고 검증 후 Tauri 개발 앱을 직접 실행한다. 배포 패키지는 명시적인 릴리스 시점에만 만든다. | 2026-08-12 사용자 승인 |
| D-119 | Esc는 열린 상세의 active tab 하나를 먼저 닫고, 상세가 없으면 선택 해제, 선택도 없으면 트레이 최소화/프로그램 종료 선택창을 연다. 진행 중 다운로드 수와 종료 시 중단 복구를 함께 안내한다. | 2026-08-13 사용자 승인 |
| D-120 | 카드 표지의 hover `+`/`…` command를 제거하고, sole selection 일반 재클릭은 선택 해제한다. 선택 0의 첫 일반 metadata/status 클릭은 원래 action을 수행한다. | 2026-08-13 사용자 승인 |
| D-121 | 카드의 중복 의심·다운로드 중 상태는 텍스트 대신 접근 가능한 warning/download 아이콘으로 표시하며 상세 설명은 Activity Center에 유지한다. | 2026-08-13 사용자 승인 |
| D-122 | Explore, Auto Find, Downloads, Detail, Review의 미리보기는 화면별 worker가 아니라 프로세스 전역 ThumbnailCoordinator 하나가 우선순위·중복 요청·취소·cache를 담당한다. | 2026-08-14 사용자 승인 |
| D-123 | 다운로드 폴더가 정해지지 않은 첫 다운로드에서만 Windows 폴더 선택 dialog를 열고, 선택한 경로를 설정에 영속한다. 취소하면 queue를 만들지 않는다. | 2026-08-15 전체 구현 지시 |
| D-124 | Downloads에서 여러 완료 항목을 선택하고 Enter를 누르면 첫 번째 항목의 첫 검증 파일만 연다. | 2026-08-15 전체 구현 지시 |
| D-125 | Review는 별도 창이 아니라 현재 앱 위의 대형 dialog로 유지한다. | 2026-08-15 전체 구현 지시 |
| D-126 | quarantine은 자동으로 영구 삭제하지 않는다. 영구 삭제는 명시적인 사용자 명령과 재확인을 거친다. | 2026-08-15 전체 구현 지시 |
| D-127 | 다운로드 page는 source page 번호를 immutable identity로 두고, `.part`→decode/WebP→SHA-256→atomic rename→manifest 검증 뒤에만 완료한다. | 2026-08-15 Milestone C 구현 |
| D-128 | quarantine과 undo는 filesystem move 전 pending DB record를 만드는 crash-safe saga로 처리한다. 원본/격리 경로가 모두 있거나 모두 없으면 자동 삭제·덮어쓰지 않는다. | 2026-08-15 Milestone C 구현 |
| D-129 | 앱 시작 시 유효한 download root에 대해 quarantine saga와 artifact 무결성을 먼저 reconcile하고, 그 뒤 interrupted job을 verified page checkpoint부터 자동 재개한다. | 2026-08-15 Milestone C 구현 |
| D-130 | 검색 입력은 local draft로 유지하고, 원격 요청은 사용자가 검색을 제출하거나 `즐겨찾기 작가 갱신`을 명시적으로 실행할 때만 만든다. | 2026-08-15 Milestone D 구현 지시 |
| D-131 | 작가·그룹·시리즈·캐릭터·태그 즐겨찾기를 SQLite에 영속하되 Auto Find 원격 갱신의 대상은 작가 즐겨찾기로 한정한다. | 2026-08-15 Milestone D 구현 |
| D-132 | 검색 이력과 Auto Find run·진행률·후보·명시적 제외를 SQLite canonical state로 저장한다. event는 갱신 신호이고 앱 재시작은 snapshot으로 복원한다. | 2026-08-15 Milestone D 구현 |
| D-133 | Auto Find 후보에서 download entry와 명시적 gallery 제외를 먼저 제거한다. 숨김·중복 판정은 존재하지 않는 상태를 추측하지 않고 Phase 5의 versioned decision schema가 추가될 때 결합한다. | 2026-08-15 Milestone D 안전 경계 |
| D-134 | Auto Find 취소는 run 상태와 cancellation token을 함께 사용하며, 앱 종료 중 run은 cancelled, 비정상 종료 뒤 남은 running run은 failed로 종결하고 부분 후보는 보존한다. | 2026-08-15 Milestone D 구현 |
| D-135 | `GallerySummary`와 `GalleryDetail`은 시리즈·캐릭터를 항상 배열로 전달하고, 검색·상세·Related·Auto Find 복원과 favorite 표시가 같은 metadata를 사용한다. 기존 v10 후보는 v11 migration에서 빈 배열로 안전 보존한다. | 2026-08-15 Milestone D 구현 |
| D-136 | 여러 단어 metadata는 favorite 값에는 정규화된 공백으로 저장하고 source 검색 token에는 underscore를 사용한다. `series:`와 `character:`는 각 Hitomi Nozomi namespace endpoint로 직렬화한다. | 2026-08-15 Milestone D source 계약 |
| D-137 | 작품 중복 검사는 verified local artifact와 versioned HashProfile만 사용한다. metadata는 전수 pair 작업의 우선순위만 정하고 실제 SHA/perceptual/sequence evidence 없이 강한 후보를 만들지 않는다. | 2026-08-16 Milestone E 구현 |
| D-138 | 작품 page matching은 단조 1:1 gap-tolerant alignment로 고정하고 blank·저정보·작은 장면 변화·일부 공통 panel은 강한 후보에서 제외한다. 재압축·해상도·번역 차이는 exact와 분리된 visual evidence다. | 2026-08-16 Milestone E 구현 |
| D-139 | Review는 전역 thumbnail coordinator의 검증 local `artifactPage(entryId, sourcePage)`를 사용한다. 숨김·연작·pair 제외는 revision CAS와 append-only history로 적용하며 자동 파일 삭제를 하지 않는다. | 2026-08-16 Milestone E 구현 |
| D-140 | E-Hentai relation은 적법한 session을 사용자가 명시적으로 제공한 경우에만 활성화한다. 기본 production provider는 비활성이고 cookie/session을 DB·manifest·로그에 저장하지 않는다. | 2026-08-16 Milestone E 안전 경계 |
| D-141 | 내부 visual page 중복은 단일 유사 page로 만들지 않고 최소 2행의 단조 scene block만 Review에 올린다. exact SHA 반복은 별도 근거로 한 행을 허용한다. | 2026-08-16 Milestone F 오탐 안전 경계 |
| D-142 | 내부 page 제거는 group revision·파일 수·byte 합계를 고정한 plan preview 후에만 artifact 내부 quarantine으로 적용한다. source page number와 검증 metadata를 유지하고 자동 영구 삭제하지 않는다. | 2026-08-16 Milestone F 구현 |
| D-143 | page quarantine·undo는 DB intent, atomic file move, manifest atomic replace, SQLite completion으로 구성된 crash-safe saga다. 시작 시 pending saga를 재개하고 모호한 두 경로는 overwrite/delete하지 않는다. | 2026-08-16 Milestone F 안전 경계 |
| D-144 | Classic import는 사용자가 고른 경로만 읽기 전용으로 inventory하고 원본 state·DB·파일을 이동·수정·삭제하지 않는다. | 2026-08-16 Milestone G 안전 경계 |
| D-145 | Classic artifact는 dry-run·경고 확인·최종 승인 뒤 Next 관리 폴더에 검증된 WebP 복사본으로 만들며 실제 파일·manifest 확인 뒤에만 completed로 등록한다. | 2026-08-16 Milestone G 구현 |
| D-146 | Classic legacy hash는 provenance로만 보존하고 현재 HashProfile의 중복 차단 근거로 신뢰하지 않는다. manifestless·ID mismatch·중복 폴더를 자동 추측하거나 병합하지 않는다. | 2026-08-16 Milestone G 안전 경계 |
| D-147 | Classic rollback은 해당 import가 새로 만든 Next DB row와 복사본만 대상으로 하며 파일은 영구 삭제하지 않고 import 전용 quarantine으로 이동한다. 적용/rollback 중단도 시작 시 같은 상태로 수렴한다. | 2026-08-16 Milestone G 구현 |
| D-148 | production thumbnail client는 앱 composition root가 반드시 명시적으로 주입한다. React context가 브라우저 fixture로 암묵 fallback하지 않으며 browser review mode만 명시적인 fixture adapter를 사용한다. | 2026-08-16 Milestone H 보안 경계 |
| D-149 | 설정에는 실제 구현된 일반·저장 공간만 노출한다. 안전한 plan·undo가 없는 cache/영구 삭제는 이유를 표시한 disabled 상태로 두며, 상세 page 확대는 전역 thumbnail coordinator를 사용하는 실제 dialog로 제공한다. | 2026-08-16 Milestone H 운영 UX |
| D-150 | 앨범 카드는 포스터형 대신 가로 밀도형을 유지한다. 점수·날짜는 제거하고 기존 metadata 종류는 보존하되, 실제 chip 측정·font 준비·container resize와 원본 이미지 비율로 배치를 적응시킨다. | 2026-08-20 사용자 지시 및 adaptive card 구현 |
| D-151 | frontend thumbnail 구독은 마지막 이탈 뒤 400ms grace를 두고, 완료 display asset은 120초·최대 256개 보존한다. 이는 전역 Rust cache와 별개이며 최종 eviction 때만 Blob URL을 해제한다. | 2026-08-20 viewport churn 회귀 수정 |
| D-152 | Explore는 query별 완료 page를 최대 5개, 현재 page ±2로 제한하고 인접 page를 prefetch한다. query reset은 requestId별 backend 작업도 `search_page_cancel`로 실제 취소하며 late completion은 projection을 바꾸지 않는다. | 2026-08-20 Explore cache·취소 구현 |
| D-153 | artifact folder template은 새 artifact에만 적용하고 `{id}`를 필수로 한다. 이미 등록된 `relative_directory`와 `root_snapshot`은 immutable이며 자동 rename·move·일괄 migration은 제공하지 않는다. | 2026-08-20 schema v15/v16 안전 경계 |
| D-154 | AVIF는 `avif-rust=0.0.6`, `bin-rs=0.0.10`의 순수 Rust bounded decoder로 실험 지원한다. JPEG XL은 형식 diagnostic만 보존하고 decoder가 없으므로 fallback 뒤 `IMAGE_FORMAT_UNSUPPORTED`로 종료한다. | 2026-08-20 source recovery 구현 |
| D-155 | Auto Find history mode는 run 시작 시 snapshot한다. `newer_than_oldest_downloaded` cutoff는 검증 소유 artifact만 근거로 하고 `source=verified_owned_artifact`, `policyVersion=1`을 영속한다. 증거가 없으면 cutoff하지 않는다. | 2026-08-20 schema v17 안전 경계 |
| D-156 | Auto Find는 Nozomi ID에 cutoff를 먼저 적용한 뒤 최대 50,000 candidate를 처리하고 초과 시 `candidate_limit_after_cutoff`를 기록한다. 기존 작가당 250-page 상한은 폐기한다. | 2026-08-20 Auto Find source 정책 |

## 제안

첫 완성판의 핵심 화면 구조와 상호작용은 확정·구현됐다. 아래 조사 항목은 성능·외부 evidence 품질을 높이는 후속 연구이며 현재 canonical 상태를 fixture나 추정값으로 대체하는 근거가 아니다.

## 조사

| ID | 조사 내용 | 종료 조건 |
|---|---|---|
| R-201 | React virtual list가 20~200개 카드와 resize에서 충분한가 | prototype frame과 input latency 측정 |
| R-202 | SQLite writer 단일화와 hash worker 병렬 처리 | lock 없이 동시 download/hash integration test |
| R-203 | 비동기 pooled HTTP에서 동시성 5 경계가 유지되는가 | download-probe 새 client mode 결과 |
| R-204 | Classic localStorage 안전 export | Classic 코드 최소 변경 또는 WebView profile read 방법 결정 |
| R-205 | E-Hentai relation을 초기 milestone에서 제외해도 후보 품질이 충분한가 | golden candidate recall 비교 |

## 다음 사용자 확인 항목

1. quarantine 수동 영구 삭제 화면에서 보여 줄 evidence 범위
