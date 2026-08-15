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
| D-117 | 국가 아이콘은 Classic이 실제 UI에서 사용한 FlagCDN KR/JP/US PNG를 byte-identical 로컬 자산으로 묶는다. Classic에 없던 중국어 badge는 생략한다. | 2026-08-13 사용자 승인 및 Classic 런타임 감사 |
| D-118 | 구현 검토 단계에서는 MSI/Setup 패키징을 반복하지 않고 검증 후 Tauri 개발 앱을 직접 실행한다. 배포 패키지는 명시적인 릴리스 시점에만 만든다. | 2026-08-12 사용자 승인 |
| D-119 | Esc는 열린 상세의 active tab 하나를 먼저 닫고, 상세가 없으면 선택 해제, 선택도 없으면 트레이 최소화/프로그램 종료 선택창을 연다. 진행 중 다운로드 수와 종료 시 중단 복구를 함께 안내한다. | 2026-08-13 사용자 승인 |
| D-120 | 카드 표지의 hover `+`/`…` command를 제거하고, sole selection 일반 재클릭은 선택 해제한다. 선택 0의 첫 일반 metadata/status 클릭은 원래 action을 수행한다. | 2026-08-13 사용자 승인 |
| D-121 | 카드의 중복 의심·다운로드 중 상태는 텍스트 대신 접근 가능한 warning/download 아이콘으로 표시하며 상세 설명은 Activity Center에 유지한다. | 2026-08-13 사용자 승인 |
| D-122 | Explore, Auto Find, Downloads, Detail, Review의 미리보기는 화면별 worker가 아니라 프로세스 전역 ThumbnailCoordinator 하나가 우선순위·중복 요청·취소·cache를 담당한다. | 2026-08-14 사용자 승인 |
| D-123 | 다운로드 폴더가 정해지지 않은 첫 다운로드에서만 Windows 폴더 선택 dialog를 열고, 선택한 경로를 설정에 영속한다. 취소하면 queue를 만들지 않는다. | 2026-08-15 전체 구현 지시 |
| D-124 | Downloads에서 여러 완료 항목을 선택하고 Enter를 누르면 첫 번째 항목의 첫 검증 파일만 연다. | 2026-08-15 전체 구현 지시 |
| D-125 | Review는 별도 창이 아니라 현재 앱 위의 대형 dialog로 유지한다. | 2026-08-15 전체 구현 지시 |
| D-126 | quarantine은 자동으로 영구 삭제하지 않는다. 영구 삭제는 명시적인 사용자 명령과 재확인을 거친다. | 2026-08-15 전체 구현 지시 |

## 제안

현재 Phase 1의 큰 방향 제안은 모두 승인됐다. 세부 상호작용과 화면 시안은 prototype 검토에서 확정한다.

## 조사

| ID | 조사 내용 | 종료 조건 |
|---|---|---|
| R-201 | React virtual list가 20~200개 카드와 resize에서 충분한가 | prototype frame과 input latency 측정 |
| R-202 | SQLite writer 단일화와 hash worker 병렬 처리 | lock 없이 동시 download/hash integration test |
| R-203 | 비동기 pooled HTTP에서 동시성 5 경계가 유지되는가 | download-probe 새 client mode 결과 |
| R-204 | Classic localStorage 안전 export | Classic 코드 최소 변경 또는 WebView profile read 방법 결정 |
| R-205 | E-Hentai relation을 초기 milestone에서 제외해도 후보 품질이 충분한가 | golden candidate recall 비교 |

## 다음 사용자 확인 항목

1. clickable prototype의 정보 위치와 화면 밀도
2. quarantine 수동 영구 삭제 화면에서 보여 줄 evidence 범위
3. Classic import dry-run 보고서 형식
