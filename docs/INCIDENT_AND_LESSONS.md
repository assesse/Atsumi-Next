# 문제 이력과 재발 방지

## 목적

과거 문제를 단순한 개발 기록이 아니라 Atsumi Next의 설계 제약과 회귀 테스트로 변환한다.

| 문제 | 관찰된 원인 또는 구조 | Classic 조치 | Next 재발 방지 |
|---|---|---|---|
| 과거 mock 목록과 최신 목록 혼재 | fallback cache와 live source 경계 불명확 | live 조회 우선, mock 제거 | `DataOrigin` 타입, fixture와 live 경로 분리 |
| 검색 결과 수가 설정값보다 적음 | 언어와 숨김 tag를 가져온 결과에 후처리 | 검색식과 index clause 조정 | query planner가 서버 조건과 client 조건을 보고 |
| 제외 tag가 적용되지 않음 | `female:mother`와 `-female:mother` 조합 불일치 | 검색식 조합 수정 | query AST와 serialization golden test |
| 입력마다 검색되어 지연 | input event와 submit 혼합 | 검색 버튼 전까지 요청 금지 | draft query와 submitted query 별도 state |
| 검색 중 앱이 응답 없음 | blocking 네트워크와 대량 DOM 갱신 | async command와 점진 표시 | 비동기 service, worker, virtual list |
| 목록이 모두 나온 뒤 한 장만 남음 | preload 응답이 현재 page state를 덮음 | merge와 page state 보정 | query revision과 stale response 폐기 |
| thumbnail 전부 404/대기 | URL 후보, cache, retry 상태 혼합 | resolver와 queue 보강 | typed resolution result와 negative cache TTL |
| 다운로드 목록 thumbnail만 죽음 | Explore와 로컬 목록의 resolver 차이 | 규칙 통합 | GalleryThumbnail component와 단일 backend API |
| 상세 19p 이후 lazy load 실패 | 내부 scroll container 감지 오류 | debug scroll과 batch 조정 | virtualized preview와 scroll owner 단일화 |
| 정렬을 바꿔도 최신순 | Hitomi 검색 token과 내부 sort 불일치 | order token 처리 수정 | 실제 응답 fixture로 sort contract test |
| queue가 재시작 후 사라짐 | 프론트 목록 중심 영속 | state snapshot 저장 | SQLite download entry와 job 분리 |
| 다운로드 503와 실패 급증 | 동시 연결 경계 초과 | probe로 동시 5, 25ms 적용 | scheduler profile, telemetry, 재측정 도구 |
| 일부 페이지 성공 후 gallery 실패 | 페이지 URL 후보와 retry가 gallery 상태에 묶임 | 페이지별 retry와 resume | page attempt table과 독립 재개 |
| 외부 열기가 CMD/그림판으로 연결 | shell 호출 방식과 대상 경로 오류 | 첫 image shell open | Windows shell adapter contract test |
| 강제 종료 후 진행 상태가 남음 | 메모리 작업과 snapshot 상태 불일치 | interrupted 변환과 metadata | 영속 job checkpoint와 startup recovery |
| 무결성 검사 중 UI 정지 | 대량 해시가 호출 흐름을 점유 | async command와 상태 표시 | cancellable worker job과 progress event |
| 유사하지 않은 작품이 후보로 묶임 | tag와 느슨한 pHash 기준 과신 | threshold와 sequence anchor 강화 | evidence별 confidence, 보수적 후보 생성, golden negatives |
| 완전 포함 작품을 놓침 | 대표 thumbnail과 제한된 hash만 비교 | 전수 비교와 anchor scan | staged full scan과 page matrix fixture |
| 한 페이지가 여러 페이지와 잘못 매칭 | greedy matching과 page 재사용 | 가중치와 정렬 조건 수정 | bipartite/sequence alignment test corpus |
| 부모를 숨겨도 다시 후보화 | localStorage, hash DB, 파일 정리가 분리 | purge와 목록 제거 보강 | ApplyDuplicateDecision 단일 transaction |
| 후보 숨김이 유사작품 제외에 남지 않음 | UI action과 영속 목록 분리 | snapshot 저장 보강 | decision history에서 derived exclusion 생성 |
| 내부 판독이 무조건 A/B/C 전역 판본 생성 | 전체 갤러리를 고정 개수로 partition | 장면 토막별 비교 UI로 변경 | SceneBlock을 독립 단위로 모델링 |
| 페이지/Related preview가 모두 1p처럼 보임 | image source와 page identity를 배열 위치 또는 cover 전용 style에 잘못 결합 | source page별 URL 보정 | 모든 thumbnail key를 gallery ID와 source page로 구성하고 component 공용 fixture로 검증 |
| 한 장 누락 때문에 긴 묶음 전체 실패 | 완전 정렬만 허용 | 앞뒤 block 분리 | gap-tolerant sequence alignment와 split evidence |
| 묶음 제외 후 thumbnail/page binding 파손 | 배열 index와 source page 혼용 | 원본 번호 보존 manifest | `SourcePageNumber` value type과 immutable mapping |
| 다운로드 완료인데 실제 파일 없음 | 앱 외부 삭제 감지 부족 | 무결성 검사 | startup reconciliation과 on-open verification |
| 창 크기 복원 실패 | WebView state와 native window 적용 시점 경쟁 | 저장과 복원 재시도 | native window repository가 위치와 크기를 소유 |
| 카드 진행률 갱신 시 화면 깜빡임 | 전체 render 함수 재호출 | 부분 patch 추가 | component keyed update와 normalized store |

## 필수 golden 사례

| 사례 | 보존할 기대 결과 |
|---|---|
| `3996214` 대 `3949301` | 다운로드 중 포함 중복을 감지하고 검토로 전환 |
| `4050378` | 대규모 내부 장면 토막과 다언어 순서 패턴을 안정적으로 분리 |
| `4093093` 대 `4074667` | 해상도 차이가 있어도 충분한 근거를 제공하되 자동 삭제하지 않음 |
| `3778299` 대 `3363034` | 실제 불일치 사례를 강한 중복으로 판정하지 않음 |
| `4034362` | 느슨한 기준으로 전체 페이지를 문제로 판정하지 않음 |
| `3880196` 대 `3880197` | 전체 page pair를 표시하고 각 thumbnail이 실제 page를 가리킴 |
| `3005910` | 내부 page selection 후 원본 번호와 manifest를 보존 |

실제 원격 데이터는 변할 수 있으므로 가능한 범위에서 metadata, image sample, expected hash를 로컬 fixture로 고정한다.

Milestone E의 repository-safe synthetic regression은 저작물 원본을 커밋하지 않고 다음 경계를 고정한다: blank/저정보 page 거부, 서로 다른 고대비 흑백 layout 거부, 중앙 장면 변화 거부, 10-page 중 2-page 공통 panel만 있는 pair 거부, 재압축·해상도와 작은 번역 overlay visual match, 긴/짧은 gallery 양방향 containment, 단조 1:1 page 비재사용. 위 golden ID의 실제 파일 검증은 사용자가 보유한 local artifact 또는 opt-in live smoke에서만 수행하며 일반 CI의 완료 근거로 가장하지 않는다.

## 사건 기록 양식

앞으로 문제를 수정할 때 다음을 남긴다.

```text
Incident ID:
Observed version:
User-visible symptom:
Minimal reproduction:
Root cause:
Data affected:
Fix:
Regression test:
Migration or cleanup required:
```
