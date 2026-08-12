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
2. Gallery 입력 규칙에서 아직 충돌하는 단일 클릭과 재선택 동작
3. Review 진입점과 대형 dialog/독립 window 중 선호
4. quarantine 보존 기간과 영구 삭제 UI
5. Classic import dry-run 보고서 형식
