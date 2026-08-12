# UX 상호작용 행렬

## 목적

같은 Gallery와 metadata가 어느 화면에서든 예측 가능한 입력 규칙을 갖도록 한다. `초안` 표시는 prototype 검토 후 확정한다.

## Gallery card

| 상황 | 입력 | 동작 | 상태 |
|---|---|---|---|
| 선택 없음 | 좌클릭 | 해당 카드 하나 선택 | 초안 |
| 한 개 이상 선택 | 좌클릭 | 기존 선택을 지우고 해당 카드만 선택 | Classic 최신 요구 반영 |
| 같은 단일 카드 선택됨 | 좌클릭 | 선택 해제 | 최신 요구 반영 |
| 모든 상황 | Ctrl + 좌클릭 | 해당 카드만 선택 toggle | 확정 후보 |
| anchor 있음 | Shift + 좌클릭 | anchor부터 대상까지 범위 선택 | 확정 후보 |
| 다운로드 완료 | 더블클릭 | 첫 이미지 기본 연결 프로그램으로 열기 | 확정 |
| 다운로드 미완료 | 더블클릭 | 아무 파일도 열지 않고 상세 보기 | 초안 |
| card background | 우클릭 | 상세 workspace 열기 | 초안 |
| 중복 의심 badge | 좌클릭 | 작품 Review 열기 | 확정 |
| 검토 필요 status | 좌클릭 | 해당 작업 상세 또는 Review 열기 | 확정 |

## Metadata chip

| 입력 | 동작 | 상태 |
|---|---|---|
| 좌클릭 | Explore로 이동해 namespace 검색 제출 | 확정 |
| 우클릭 | 즐겨찾기 toggle | 확정 |
| hover | 번역 또는 전체 이름 tooltip | 확정 |

metadata target이 이벤트를 처리하면 card의 상세 열기와 선택은 발생하지 않는다.

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
| 모든 목록 | Escape | 선택 해제, 열린 menu 닫기 |

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

## Selection toolbar

- 목록 위에 예약된 layer 또는 overlay로 표시한다.
- 나타날 때 목록의 y 위치를 바꾸지 않는다.
- 명령 순서는 화면별 primary action 우선순위를 따른다.
- 선택 개수, 전체 선택, primary action, destructive action을 제공한다.

## 미확정 항목

1. 미완료 Gallery 더블클릭을 상세 보기로 사용할지
2. card 우클릭과 길게 누르기 중 상세 보기의 주 입력
3. 같은 단일 카드 재클릭 해제가 범위 선택 anchor도 지울지
4. Downloads에서 여러 완료 항목 Enter를 모두 외부 viewer로 열지, 첫 항목만 열지
