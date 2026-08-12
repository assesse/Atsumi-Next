# Classic 기준선 감사

## 감사 목적

현재 Atsumi를 실행 가능한 참고 구현으로 보존하고, 새 버전에서 반드시 계승할 동작과 제거할 구조적 부채를 구분한다.

## 조사 근거

- 프론트: `index.html`, `app.js`, `styles.css`
- command 등록: `src-tauri/src/lib.rs`
- 앱 상태: `src-tauri/src/commands/app.rs`
- 검색과 메타데이터: `src-tauri/src/commands/hitomi.rs`
- 다운로드: `src-tauri/src/commands/downloads.rs`
- 해시와 중복 판독: `src-tauri/src/commands/image_hashes.rs`
- 썸네일: `src-tauri/src/commands/thumbnails.rs`
- 파일 열기: `src-tauri/src/commands/files.rs`
- Classic 명세 6종과 `docs/reliability-ux-spec.md`
- 다운로드 실측: `docs/download-tuning-2026-08-04.md`

## 규모

2026-08-12 작업 트리 기준 대략적인 규모다.

| 파일 | 줄 수 | 역할 |
|---|---:|---|
| `app.js` | 5,762 | 화면 상태, 렌더링, 입력, command 호출, 일부 판정 로직 |
| `styles.css` | 2,877 | 전체 화면과 상태별 스타일 |
| `index.html` | 231 | 앱 셸, 탭, 설정과 플로팅 레이어 |
| `downloads.rs` | 1,901 | 큐, 병렬 다운로드, 재시도, 무결성, 삭제 |
| `hitomi.rs` | 2,169 | 검색, 메타데이터, URL, E-Hentai 관계 |
| `image_hashes.rs` | 2,165 | SHA-256, dHash, pHash, 작품 및 내부 중복 판독 |
| `thumbnails.rs` | 492 | 썸네일 해석, 저장, 반환 |

- 등록된 Tauri command: 28개
- 프론트 command 호출 지점: 31개
- Rust 테스트 표식: 19개

수치는 품질을 직접 판정하지 않지만, UI 상태와 도메인 규칙이 큰 파일 몇 개에 집중되어 있음을 보여준다.

## 현재 책임 분포

### 프론트가 가진 책임

- 탭별 검색 상태와 페이지 상태
- 선택, Ctrl/Shift 범위 선택, 키보드 명령
- 다운로드 목록과 작업 상태의 영속 snapshot
- 즐겨찾기와 작가 기준
- 일반 제외, 유사작품 제외, 묶음 제외
- 연작 그룹과 중복 후보 해제
- 유사 후보 1차 판정 일부
- 상세 탭 트리와 관련 갤러리 이동
- 전체 DOM 렌더링과 일부 부분 갱신

### 백엔드가 가진 책임

- Hitomi 인덱스와 갤러리 상세 조회
- 이미지 URL 구성과 다운로드
- 재시도, host cooldown, 전역 요청 제한
- WebP 저장과 파일 metadata 기록
- SHA-256, dHash, pHash 생성과 SQLite 기록
- 작품 간 포함 검사와 내부 페이지 유사 검사
- 파일 열기, 삭제, 무결성 검사
- `state.json` 읽기와 쓰기

### 경계가 중복된 책임

- 다운로드 폴더 경로 생성
- 다운로드 상태의 canonical 값
- 갤러리 metadata 병합
- 중복 후보의 생존 여부
- 다운로드 완료 여부와 실제 파일 존재 여부
- 설정의 localStorage와 `state.json` 동기화

## 상태 저장 위치

| 위치 | 현재 역할 | 위험 |
|---|---|---|
| 브라우저 `localStorage` | UI 설정, 다운로드 목록, 제외와 묶음 | 외부 파일 및 DB와 불일치 가능 |
| `AtsumiData/state.json` | localStorage의 백업 성격 snapshot | 두 저장소 중 우선순위 불명확 |
| `atsumi_cache.sqlite` | 페이지 해시와 파일 해시 | 사용자 판정과 job 상태는 없음 |
| `.atsumi-download.json` | 폴더별 다운로드 상태 | 앱 목록과 다를 수 있음 |
| `.atsumi-page-selection.json` | 내부 판독으로 제외한 원본 페이지 | DB와 UI 상태 재조정 필요 |
| 실제 다운로드 폴더 | 완료 파일 | 외부 삭제나 이동 가능 |
| 메모리 Map/Set | 실행 중 요청과 캐시 | 종료 시 복구 불가 |

## 확인된 구조적 문제

1. **영속 상태의 주인이 여러 개다.** 같은 다운로드가 UI에서는 완료지만 폴더가 없거나, 숨긴 갤러리 해시가 DB에 남을 수 있다.
2. **command가 화면 요구에 직접 맞춰 누적됐다.** 반환값이 `serde_json::Value`인 영역이 많아 계약을 컴파일 시점에 검증하기 어렵다.
3. **오류가 문자열 규칙에 의존한다.** 예를 들어 `duplicate_detected:`를 프론트가 해석해 `review` 상태를 만든다.
4. **프론트가 도메인 결정을 보유한다.** 작품 후보 판정, 묶음, 해제와 영속 처리가 화면 코드에 섞여 있다.
5. **백그라운드 작업이 영속 job이 아니다.** 앱 종료 후 snapshot을 `interrupted`로 바꾸지만 정확한 단계와 재개 지점은 제한적이다.
6. **렌더링 경계가 넓다.** 작업 상태나 썸네일 하나의 변화가 큰 목록 갱신으로 번질 수 있다.
7. **실제 사이트 동작이 암묵지다.** 검색식, 정렬, URL 후보, CDN 실패 대응은 회귀 fixture 없이 구현에 묻혀 있다.
8. **중복 판정의 학습 자료가 코드 외부에 흩어져 있다.** 실제 오탐과 정탐 사례가 자동 회귀 corpus로 고정되지 않았다.

## 요구사항의 신뢰도 순서

1. 현재 사용자가 확인한 최신 정상 동작
2. 가장 최근의 명시적 결정
3. 실제 DB, 로그, 파일 metadata와 다운로드 결과
4. 현재 코드의 동작
5. 과거 문서와 대화
6. 개발자의 추정

충돌 시 하위 근거를 자동으로 채택하지 않고 결정 기록에 남긴다.

## Classic 보존 조치

2026-08-12 완료:

- 보존 commit: `3b3bedd Preserve Atsumi Classic baseline before rewrite`
- annotated tag: `atsumi-classic-baseline-2026-08-12`
- Next branch: `codex/atsumi-next`
- frontend: 기존 `node_modules`의 Vite 5.4.21 production build 통과
- backend: `cargo test --manifest-path src-tauri/Cargo.toml --offline`, 15 passed

아직 수행하지 않은 작업:

- Classic 실행 파일과 `AtsumiData`의 읽기 전용 백업 생성
- 사용자 다운로드 폴더의 metadata inventory 생성
- 중복 판정용 실제 gallery fixture 목록 고정

사용자 데이터 snapshot은 실제 파일에 영향을 줄 수 있으므로 import 작업 직전에 별도 승인받는다.
