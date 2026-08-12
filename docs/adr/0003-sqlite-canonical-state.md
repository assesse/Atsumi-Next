# ADR-0003: SQLite를 canonical state로 사용

- 상태: 승인
- 결정일: 2026-08-12

## 맥락

Classic은 localStorage, `state.json`, hash SQLite, folder manifest와 실제 파일에 상태가 분산되어 있다. 숨김, 완료와 다운로드 상태가 서로 어긋나는 문제가 반복됐다.

## 결정

- 사용자 설정, 즐겨찾기, 제외, 다운로드 entry, job, 판정 이력의 기준은 SQLite다.
- 실제 파일은 artifact이며 reconciliation 대상이다.
- folder manifest는 이식과 복구를 위한 파생 metadata다.
- frontend store는 SQLite snapshot의 projection과 임시 UI 상태만 가진다.

## 결과

- 상태 전이를 transaction으로 묶을 수 있다.
- migration과 backup 설계가 필수다.
- DB만 믿고 실제 파일을 완료로 간주해서는 안 된다.
