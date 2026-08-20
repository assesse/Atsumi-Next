# 기능 유지와 재설계 행렬

2026-08-20의 schema v17과 통합 검증을 기준으로 한다. `구현`은 자동 테스트 또는 opt-in live 증거가 있는 기능, `제한`은 구현됐지만 아래 경계가 남은 기능, `보류`는 계약만 있거나 의도적으로 비활성화한 기능이다.

## 탐색·카드·미리보기

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| 시작 Recent·명시적 검색 | 구현 | 입력 change는 local draft이고 제출만 원격 요청·이력을 만든다. |
| Explore page cache | 구현 | query별 settled page 최대 5개, 현재 page ±2 창, 인접 page prefetch, page별 scroll 복원. |
| 오래된 검색 취소 | 구현 | reset/query 교체가 `requestId`별 `search_page_cancel`을 호출하며 cancel-before-start와 active 취소를 backend 테스트로 고정했다. 늦은 완료도 현재 query를 덮지 않는다. |
| 가로 밀도형 앨범 카드 | 구현 | 점수·날짜를 표시하지 않는다. 실제 chip 크기, font 준비와 container resize를 다시 측정하고 이미지 고유 비율을 보존한다. 페이지 수·gallery ID와 기존 metadata 종류는 유지한다. |
| 반응형 열·카드 폭 | 구현 | preview 폭을 가용 내용에 맞게 제한하고 1~4열 resize에서 텍스트·chip·이미지가 넘치거나 찌그러지지 않도록 테스트했다. |
| 중국어 badge | 구현 | 로컬 `cn.svg`와 텍스트 `CN` fallback을 사용하며 런타임 네트워크 의존성이 없다. |
| 전역 thumbnail coordinator | 구현 | Explore, Downloads, Detail, gallery/internal Review가 하나의 coordinator와 `galleryCover`/`galleryPage`/`artifactPage` key를 공유한다. |
| viewport 왕복 retention | 구현 | 마지막 구독 해제 뒤 400ms orphan grace, 완료 display asset 120초·최대 256개 보존. Rust cache는 512 entries/64MiB/30분이고 retryable/permanent negative TTL은 3초/5분이다. |

## 즐겨찾기와 Auto Find

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| 5개 metadata 즐겨찾기 | 구현 | artist/group/series/character/tag를 SQLite에 저장하고 카드·상세·Related가 같은 projection을 쓴다. |
| 명시적 작가 갱신 | 구현 | 현재 artist favorite만 source 조회 대상이며 동시에 한 run만 허용한다. 진행·후보·취소·실패가 재시작 뒤 복원된다. |
| 이력 범위 정책 | 구현 | `include_all_history` 또는 `newer_than_oldest_downloaded`를 run마다 snapshot한다. |
| cutoff 증거 | 구현 | complete 또는 quarantined이고 실제 artifact가 있는 소유 gallery만 사용한다. `source=verified_owned_artifact`, `policyVersion=1`, 작가별 oldest ID와 qualified count를 저장한다. 증거가 없으면 cutoff하지 않는다. |
| 대형 작가 제한 | 제한 | Nozomi ID에 cutoff를 먼저 적용한 뒤 최대 50,000 candidate를 처리한다. 초과 시 `candidate_limit_after_cutoff` truncation을 저장하며 무제한 조회를 주장하지 않는다. |

## 다운로드·artifact

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| queue·resume·reconcile | 구현 | 검증 page checkpoint부터 같은 entry/job attempt를 재개하고 manifest·DB·파일이 맞기 전에는 completed가 되지 않는다. |
| 새 artifact 폴더 template | 구현 | 기본 `[{artist}] {title} [{group}] {id}`. `{id}` 필수, Windows reserved/control 문자·길이·root containment를 검증한다. |
| 기존 artifact 자동 이름 변경 | 보류 | 구현하지 않았다. 기존 `relative_directory`와 `root_snapshot`은 DB trigger로 immutable이며 새 template은 새 artifact에만 적용된다. |
| WebP/JPEG/PNG 입력 | 구현 | decode·검증 후 lossless WebP로 저장하고 SHA-256·manifest를 기록한다. |
| AVIF 입력 | 제한 | 고정된 순수 Rust decoder로 bounded decode한다. experimental이며 대표 live corpus 검증은 아직 없다. |
| JPEG XL 입력 | 보류 | 후보 형식과 diagnostic은 기록하지만 decoder는 없다. fallback 후보를 계속 시도하고 모두 실패하면 non-retryable `IMAGE_FORMAT_UNSUPPORTED`다. |
| live Hitomi gallery download | 구현 | 2026-08-20 opt-in smoke: gallery 4113714, 18/18 WebP, selected payload 12,396,942 bytes. 단일 gallery 증거다. |
| 격리·undo | 구현 | root 내부 crash-safe saga만 사용하고 자동 overwrite/delete/purge하지 않는다. |

## Review·Classic·운영

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| 작품 중복 Review | 구현 | verified artifact, versioned hash/evidence, monotonic alignment, revision CAS, hide/series/pair-exclude 이력. 자동 파일 삭제 없음. |
| 앨범 내부 페이지 Review | 구현 | exact 또는 최소 2행 시각 블록, immutable source page number, 계획 preview, quarantine/undo saga. |
| Classic import | 구현 | 사용자가 고른 원본을 read-only inventory/dry-run하고 승인된 사본만 Next에 등록한다. rollback도 Next가 만든 row/copy만 다룬다. |
| E-Hentai relation | 보류 | port와 evidence type만 있고 명시적 session이 없는 production에서는 비활성이다. |
| 모바일 transfer | 보류 | 초기 완성 범위에 없다. |

## 완료 판단

Phase 3~7의 자동 completion gate는 통과했다. 최신 `tools/verify.ps1` 증거는 frontend 21 files/130 tests, Rust library 137 passed(외부 live smoke 1 ignored), startup 2 passed와 typecheck/build/fmt/check/clippy/whitespace, Tauri release `--no-bundle` 성공이다. 실데이터 안전 경계와 수동 검토 항목은 [KNOWN_ISSUES.md](KNOWN_ISSUES.md)에 별도로 유지한다.
