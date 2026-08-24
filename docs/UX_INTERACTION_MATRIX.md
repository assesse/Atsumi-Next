# UX 상호작용 행렬

## 목적

같은 Gallery와 metadata가 어느 화면에서든 예측 가능한 입력 규칙을 갖도록 한다. `초안` 표시는 prototype 검토 후 확정한다.

## Gallery card

| 상황 | 입력 | 동작 | 상태 |
|---|---|---|---|
| 선택 없음 | 카드 배경 좌클릭 | 해당 카드 하나 선택 | 확정 |
| 한 개 이상 선택 | 카드 어디든 좌클릭 | 기존 선택을 지우고 해당 카드만 선택 | 확정 |
| 같은 단일 카드 선택됨 | 좌클릭 | 해당 카드 선택과 anchor 해제 | 확정 |
| 모든 상황 | Ctrl + 좌클릭 | 해당 카드만 선택 toggle | 확정 |
| anchor 있음 | Shift + 좌클릭 | anchor부터 대상까지 범위 선택 | 확정 |
| Explore / Auto Find | 더블클릭 | 상세 workspace 열기 | 확정 |
| Downloads | 더블클릭 | `artifact_open_first`로 해당 항목의 첫 파일 실행. 실행 가능한 artifact가 없으면 상태 오류를 표시하고 상세로 전환하지 않음 | 확정 |
| card background | 우클릭 | 상세 workspace 열기 | 초안 |
| 중복 의심 warning icon | 좌클릭 | 작품 Review 열기 | 확정 |
| 검토 필요 status | 좌클릭 | 해당 작업 상세 또는 Review 열기 | 확정 |

## Metadata chip

| 입력 | 동작 | 상태 |
|---|---|---|
| 좌클릭 | Explore로 이동해 namespace 검색 제출 | 확정 |
| 우클릭 | 즐겨찾기 toggle | 확정 |
| hover | 번역 또는 전체 이름 tooltip | 확정 |

metadata target이 이벤트를 처리하면 card의 상세 열기와 선택은 발생하지 않는다.

단, Ctrl/Shift가 눌렸거나 선택 항목이 하나 이상이면 별도 toggle state 없이 선택 문맥으로 파생한다. 이 문맥에서는 metadata, 상태, 상세 button을 포함한 카드 내부의 모든 좌클릭이 카드 선택 규칙을 우선하며 내부 action은 실행하지 않는다. 일반 좌클릭은 toggle이 아니라 대상 하나로 교체한다.

## Keyboard

| 화면 | 입력 | 동작 |
|---|---|---|
| Explore | Enter | 선택 항목을 Downloads queue에 추가 |
| Explore | Delete | 선택 항목을 일반 영구 제외 |
| Auto Find | Enter | 선택 후보를 queue에 추가 |
| Auto Find | Delete | 선택 후보를 일반 제외 |
| Downloads 완료 | Enter | 선택 항목 첫 이미지 열기 |
| Downloads 대기/실패 | Enter | 선택 항목 시작 또는 재시도 |
| Downloads | Delete | 확인 후 목록과 파일을 quarantine |
| 모든 목록 | Escape | 열린 상세 active tab 하나 닫기 → 선택 해제 → 종료 선택창 순으로 처리. 열린 menu/dialog가 먼저 입력을 소비함 |

## Search

- 입력은 draft만 바꾼다.
- Enter는 suggestion이 방향키 또는 클릭으로 명시 선택된 경우 suggestion을 입력한다.
- suggestion 선택이 없으면 Enter가 검색을 제출한다.
- 검색 버튼도 같은 submit action을 사용한다.
- 비어 있는 input에 focus하면 최근 검색 7개를 표시한다.
- Auto Find와 Downloads 검색은 현재 탭 데이터만 filter한다.

## Detail tab

- Gallery에서 자식 detail을 열면 현재 tab 바로 뒤에 삽입한다.
- 기존에 같은 Gallery tab이 있으면 중복 생성하지 않고 해당 tab으로 이동한다.
- tab의 `x`는 해당 tab만 닫는다.
- 최우측 전체 닫기는 모든 tab을 제거한다.
- 최소화는 tab state를 유지하고 overlay만 숨긴다.
- 복원은 view header 중앙 control에서 수행한다.
- 다운로드 entry가 있는 gallery는 다운로드 button 옆에 `저장 폴더 열기`를 표시한다. 폴더가 아직 예약·생성되지 않은 초기 queue 구간에는 안전 오류를 안내하고 임의 경로를 만들거나 download root를 대신 열지 않는다.

## Selection toolbar

- 목록 위에 예약된 layer 또는 overlay로 표시한다.
- 나타날 때 목록의 y 위치를 바꾸지 않는다.
- 명령 순서는 화면별 primary action 우선순위를 따른다.
- 선택 개수, 전체 선택, primary action, destructive action을 제공한다.

## Settings maintenance

| 입력 | 동작 | 상태 |
|---|---|---|
| 미리보기 cache 비우기 | 비활성 frontend retention과 backend 완료 cache 제거 | 다운로드/현재 화면 보존, 확정 |
| 화면·네트워크 기본값 복원 | 현재 설정 draft를 기본 preset으로 변경 | 저장 전 취소 가능, download root/template 유지, 확정 |
| 탐색 데이터 초기화 | 범위 안내 후 확인 dialog, backend transaction 실행 | active Auto Find면 거부, 다운로드 DB/files 보존, 확정 |

## 앱 종료와 tray

| 상황 | 입력 | 동작 | 상태 |
|---|---|---|---|
| main window | X | 창을 즉시 닫지 않고 backend active-work snapshot을 표시 | 확정 |
| 종료 dialog, active work 없음 | 종료 | 최신 fingerprint를 backend에서 다시 확인한 뒤 graceful quit | 확정 |
| 종료 dialog, active work 있음 | 작업을 중단하고 종료 | 다운로드·Auto Find·작품 중복·내부 중복 supervisor를 안전하게 cancel/join한 뒤 종료 | 확정 |
| 종료 dialog | 트레이로 보내기 | 창만 숨기며 진행 중인 작업은 계속 실행 | 확정 |
| tray, active work 없음 | 종료 | 최신 상태를 원자적으로 확인하고 graceful quit | 확정 |
| tray, active work 있음 또는 상태 확인 실패 | 종료 | main window를 복원하고 같은 종료 확인 dialog를 열며 즉시 종료하지 않음 | 확정 |
| 종료 상태 조회 연속 실패 | 다시 확인 → 상태 확인 없이 종료 | 두 번째 명시적 선택에서만 graceful quit 허용 | 확정 |

종료 확인 대상은 queued/resolving/downloading/hashing/verifying/retry-wait 다운로드와 running Auto Find·작품 중복·내부 중복 run이다. 완료·실패·취소·중단·검토 상태와 검색·thumbnail·Detail media 요청은 대상이 아니다. dialog의 진행률이 달라져도 같은 work identity이면 확인은 유효하고, 작업 identity가 바뀌면 최신 snapshot을 표시한 뒤 다시 선택받는다.

## 미확정 항목

1. card 우클릭과 길게 누르기 중 상세 보기의 주 입력
2. Downloads에서 여러 완료 항목 Enter를 모두 외부 viewer로 열지, 첫 항목만 열지
