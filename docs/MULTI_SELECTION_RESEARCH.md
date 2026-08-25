# 갤러리 카드 다중 선택 조사

## 문제 재현

현재 카드는 카드 배경 선택 외에도 작가·그룹·태그 검색, 상태 상세, 명시적 상세 열기와 더블클릭을 제공한다. 실제 브라우저 검증에서도 카드 중앙을 더블클릭했을 때 pointer target이 tag button에 걸려 상세 대신 검색으로 이동했다. 이 구조에서 빈 배경을 정확히 눌러야 하는 Ctrl/Shift 선택은 항목이 조밀할수록 실패하기 쉽다.

## 공식 패턴에서 확인한 점

- Microsoft의 현재 Windows selection mode는 context menu, Ctrl/Shift, gallery rollover target 등으로 mode에 진입하고, 진입 후에는 모든 항목에 checkbox와 action bar를 표시하며 항목 어디를 눌러도 선택하도록 권장한다. 선택 mode는 바깥 클릭으로 우연히 종료하지 않고 Back으로 명시적으로 종료한다.
  - <https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/selection-modes>
- Windows list 지침은 Ctrl/Shift extended selection이 숙련자에게는 익숙하지만 발견성이 낮고, 다중 선택이 중요하거나 흔하면 checkbox list가 더 명확하고 안정적이라고 설명한다.
  - <https://learn.microsoft.com/en-us/windows/win32/uxguide/ctrl-list-boxes>
- WAI-ARIA는 multi-select composite가 `aria-multiselectable`과 각 항목의 선택 상태를 일관되게 노출해야 한다고 규정한다. 카드 내부에 여러 독립 control이 있는 현재 구조를 억지로 하나의 복합 grid로 바꾸면 별도 roving-focus와 keyboard contract까지 필요하다.
  - <https://www.w3.org/TR/wai-aria/>

## 확정된 Atsumi 적용안

별도 on/off mode state는 만들지 않고 Ctrl/Shift 또는 현재 선택 개수에서 선택 문맥을 파생한다.

1. 기본 탐색 mode
   - 작가·그룹·태그·상태·상세 열기 control을 현재처럼 사용할 수 있다.
   - Explore/Auto Find 더블클릭은 상세, Downloads 더블클릭은 파일 실행으로 유지한다.
   - Ctrl/Shift 입력 또는 카드 배경의 첫 좌클릭으로 선택 문맥을 시작한다.

2. 선택 mode
   - hover 선택 원과 상세 `…` command는 표시하지 않는다.
   - 일반 좌클릭은 기존 선택을 지우고 대상 카드 하나만 선택한다. 같은 단일 카드를 다시 누르면 선택과 anchor를 해제한다.
   - Ctrl+좌클릭만 개별 toggle, Shift+좌클릭만 anchor 범위 선택으로 동작한다.
   - 카드 내부 metadata/status/detail pointer action은 mode가 끝날 때까지 비활성화한다.
   - 상단의 예약된 selection toolbar를 계속 표시하고, 바깥 클릭으로 mode를 해제하지 않는다.
   - Escape 또는 toolbar의 `선택 해제`로 선택을 0개로 만들면 선택 문맥이 자동으로 끝난다.

3. 접근성
   - 전체 카드를 복잡한 `gridcell`로 만들지 않고, 카드의 accessible name과 focus 상태로 선택 여부를 전달한다.
   - mode 진입/종료와 선택 개수 변화는 restrained live status로 알린다.
   - Shift 범위와 Ctrl toggle은 기존 reducer를 재사용한다.

## 결론

이 방식은 카드 내부 검색·상태 control을 삭제하지 않으면서 선택할 때만 click 의미를 하나로 고정한다. 별도의 `browse | selecting` reducer 상태가 없으므로 전환 상태가 어긋날 여지가 없고, 선택 문맥 중에는 카드 내부 action을 억제해 pointer 목표가 조밀해도 선택 규칙을 일관되게 유지한다.
