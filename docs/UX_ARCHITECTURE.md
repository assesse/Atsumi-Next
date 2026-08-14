# UX 정보 구조 초안

## 설계 목표

- 기능을 많이 보여주는 것보다 다음 행동을 쉽게 찾게 한다.
- 화면은 `탐색`, `관리`, `검토`의 목적을 섞지 않는다.
- 동일한 Gallery는 어느 화면에서도 같은 기본 상호작용을 가진다.
- 비동기 상태는 버튼 문구 하나가 아니라 작업 상태로 표시한다.
- 선택 도구가 나타나도 목록의 세로 위치가 바뀌지 않는다.

## 앱 셸

```text
+-------------------------------------------------------------+
| Left rail | View header: search / filters / activity / gear |
|           +-------------------------------------------------+
| Explore   | Context controls                                |
| Auto Find +-------------------------------------------------+
| Downloads | Scrollable content viewport                     |
|           |                                                 |
| collapse  | Persistent selection toolbar or reserved layer  |
+-------------------------------------------------------------+
```

### Left rail

- 앱 identity
- Explore
- Auto Find
- Downloads
- 하단 접기/펼치기
- 오류 또는 검토 대기 개수는 작은 badge로만 표시

좌측 rail은 페이지 스크롤과 무관하게 고정한다.

### View header

- 탭별 독립 검색창
- Explore에서만 웹 검색 submit
- Auto Find와 Downloads에서는 현재 결과에 대한 로컬 검색
- 언어, 정렬, 인덱스 갱신, 설정은 아이콘과 tooltip으로 배치
- 현재 백그라운드 작업은 Activity 버튼에서 확인

### Content viewport

- 앨범 목록만 스크롤
- 1~4열 responsive grid
- 카드 최소 폭을 우선하고 설정의 최대 열 수를 상한으로 사용
- grid container 하나만 관찰해 `미리보기 폭 + metadata 최소 폭`을 만족하는 열 수를 계산하고, 매 카드 폭은 측정하지 않는다.
- 작가와 선택적 그룹은 카드 좌측 정렬 바이라인 한 줄에 namespace 아이콘과 함께 표시한다.

## 화면별 구조

### Explore

1. 검색어, 정렬, 언어와 기본 tag 조건을 제출한다.
2. skeleton이 아니라 text metadata 카드가 먼저 나타난다.
3. thumbnail은 두 batch로 갱신한다.
4. 다음 페이지 text가 준비되면 다음 버튼을 사용할 수 있다.
5. 다음 페이지 thumbnail까지 준비되면 준비 상태만 강조한다.

### Auto Find

- 제목과 결과 요약
- 우측에 전체/작가별, 즐겨찾기 작가 갱신, 후보 다운로드
- 작가별 보기에서 작가 기준값은 정보 action으로 제공
- 후보 카드의 구조와 선택 규칙은 Explore와 같다.

### Downloads

- 상단 우측에 전체/작가별, 내부 중복 검사, 전체 다운로드
- 상태 filter: 전체, 작업 중, 검토, 실패, 완료
- 카드에는 짧은 상태만 표시
- 상세 오류와 재시도는 카드 상태 badge 또는 Activity detail에서 연다.
- 완료 항목의 progress bar는 100%로 유지한다.

### Detail workspace

- 화면 위에 떠 있지만 자체 tab strip과 scroll context를 가진다.
- Classic과 같은 중앙 정렬 1120px 폭을 기준으로 하되, 충분히 넓은 앱 창에서는 최대 1860px까지 연속적으로 확장한다.
- 전체 닫기는 tab strip 최우측, 탭 닫기는 각 탭에 위치한다.
- 최소화 시 view header 중앙의 복원 control로 돌아온다.
- Related gallery를 열면 현재 탭 바로 다음에 자식 탭으로 삽입한다.
- 현재 mock sprite는 실제 cell 비율인 1:1로 대표 이미지와 페이지 preview를 표시한다. Phase 3의 실제 thumbnail 계약은 각 이미지의 width/height를 전달해 source별 비율을 적용한다.
- 페이지 preview 전체는 잘라내거나 짧은 중첩 스크롤에 가두지 않고 상세 본문의 단일 scroll context에서 확인한다.

### Review workspace

- 일반 상세와 분리된 작업 화면 또는 대형 dialog다.
- 작품 중복과 내부 페이지 중복은 서로 다른 mode다.
- 작품 비교는 왼쪽과 오른쪽 metadata row 높이를 맞춘다.
- `first gid`, `parent gid`는 양쪽 값과 일치 여부를 함께 표시한다.
- 전수 검사는 progress, 취소, 완료 후 모든 match pair 펼치기를 제공한다.
- 내부 판독은 장면 토막 단위로 행을 만들고 후보 행을 동기 스크롤한다.

## Gallery 상호작용 초안

| 입력 | 기본 동작 | 비고 |
|---|---|---|
| 짧은 좌클릭 | 단일 선택 | 이미 단일 선택이면 같은 항목 재클릭 시 해제 여부는 승인 필요 |
| Ctrl + 좌클릭 | 항목 toggle | 다른 선택 유지 |
| Shift + 좌클릭 | anchor부터 범위 선택 | text/image native selection 금지 |
| 선택 상태에서 좌클릭 | 해당 항목만 선택 또는 toggle | Classic 최신 규칙 확인 필요 |
| 더블클릭 | 완료 파일은 외부 뷰어, 미완료는 상세 또는 무동작 | 승인 필요 |
| 우클릭 card | 상세 보기 | metadata chip 우클릭과 충돌하지 않게 target 우선 |
| metadata chip 좌클릭 | 해당 prefix로 Explore 검색 | 작가, 그룹, 시리즈, 캐릭터, 태그 |
| metadata chip 우클릭 | 즐겨찾기 toggle | 즐겨찾기면 주황색 |
| Enter | Explore/Auto Find는 queue, Downloads 완료는 열기 | 선택 대상에 적용 |
| Delete | Explore는 일반 제외, Downloads는 제거 확인 | 제거 정책에 따라 quarantine |

이 표는 Classic의 여러 시기 규칙이 섞여 있어 사용자 승인 전 확정하지 않는다.

## 상태 표시

공통 작업 상태:

- 대기
- 다운로드 중 animation icon과 접근 가능한 진행률
- 해시 중
- 검사 중
- 중단됨
- 검토 필요
- 실패
- 완료

각 상태는 다음 정보를 가진다.

```ts
type WorkPresentation = {
  label: string;
  severity: "neutral" | "info" | "warning" | "danger" | "success";
  progress?: number;
  detailAvailable: boolean;
  primaryAction?: string;
};
```

카드에는 label과 progress만 표시하고, 오류 원문과 재시도 이력은 detail에 둔다.

## 프로토타입 검증 과제

1. `artist:chin`을 검색하고 결과 3개를 queue한다.
2. Related galleries를 세 단계 연속 열고 원래 결과로 돌아온다.
3. 실패한 다운로드의 원인을 확인하고 재시도한다.
4. 다운로드 완료 파일을 외부 뷰어로 연다.
5. 작품 중복 후보를 전수 검사하고 후보를 숨긴다.
6. 내부 중복 장면 블록에서 남길 행을 선택하고 적용 전 결과를 확인한다.
7. 즐겨찾기 작가를 갱신하고 작가별로 후보를 다운로드한다.
8. 캐시 제거와 전체 파일 제거의 차이를 설명 없이 구분한다.

화면 구현 전에 이 과제를 클릭 가능한 mock으로 수행해 본다.
