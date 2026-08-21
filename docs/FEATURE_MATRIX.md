# 기능 유지와 재설계 행렬

2026-08-21의 schema v18과 통합 검증을 기준으로 한다. `구현`은 자동 테스트 또는 opt-in live 증거가 있는 기능, `제한`은 구현됐지만 아래 경계가 남은 기능, `보류`는 계약만 있거나 의도적으로 비활성화한 기능이다.

## 탐색·카드·미리보기

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| 시작 Recent·명시적 검색 | 구현 | 입력 change는 local draft이고 제출만 원격 요청·이력을 만든다. |
| Explore page cache | 구현 | query별 settled page 최대 5개, 현재 page ±2 창, 인접 page prefetch, page별 scroll 복원. |
| 오래된 검색 취소 | 구현 | reset/query 교체가 `requestId`별 `search_page_cancel`을 호출하며 cancel-before-start와 active 취소를 backend 테스트로 고정했다. 늦은 완료도 현재 query를 덮지 않는다. |
| 가로 밀도형 앨범 카드 | 구현 | 점수·날짜를 표시하지 않는다. grid별 행 coordinator가 같은 시각 행의 intrinsic cover 최대 높이를 공유하고 본문은 외곽 높이를 늘리지 않는다. 페이지 수·gallery ID와 기존 metadata 종류는 유지한다. |
| 높이 제한 adaptive 태그 | 구현 | 일곱 preset이 2/2/3/4/5/6/7줄 예산을 정의하고 실제 chip과 자릿수별 `+N` 폭을 함께 측정한다. favorite 우선/Female→Male→중립 stable sort이며 namespace marker와 favorite star를 분리한다. |
| Explore 태그 자동완성 | 구현 | 수동 최신화한 SQLite tag/female/male catalog만 조회한다. loaded gallery metadata와 synthetic 후보는 사용하지 않으며 artist/group/series/character 자동완성은 범위 밖이다. |
| 반응형 열·카드 폭 | 구현 | 160/190/220/250/280/320/360px만 허용하고 기본 220px, 1~4열을 사용한다. 원본 비율은 `contain`으로 보존하며 각 grid·불완전 마지막 행은 독립 계산한다. |
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
| source revision identity | 구현 | v18에서 remote 문자열 identity와 작은 내부 revision을 분리해 4113714/4132312에서 발생한 unsigned→signed SQLite overflow 경로를 제거했다. `u64::MAX` 회귀 test가 경계를 고정한다. |
| 격리·undo | 구현 | root 내부 crash-safe saga만 사용하고 자동 overwrite/delete/purge하지 않는다. |

## Review·운영

| 기능 | 상태 | 현재 계약과 증거 |
|---|---|---|
| 작품 중복 Review | 구현 | verified artifact, versioned hash/evidence, monotonic alignment, revision CAS, hide/series/pair-exclude 이력. 자동 파일 삭제 없음. |
| 앨범 내부 페이지 Review | 구현 | exact 또는 최소 2행 시각 블록, immutable source page number, 계획 preview, quarantine/undo saga. |
| 과거 데이터 이전 UI/API | 폐기 | active frontend/backend/runtime 경로를 제거했다. 기존 DB의 v14 migration과 역사적 table만 호환을 위해 보존한다. |
| 미리보기 cache clear | 구현 | 완료된 재생성 가능 cache만 제거하고 active/visible request와 다운로드 artifact를 보존한다. |
| 탐색 데이터 초기화 | 구현 | 명시적 확인과 단일 transaction으로 favorites/history/Auto Find 데이터만 제거한다. active run과 다운로드 DB/파일은 안전하게 보호한다. |
| E-Hentai relation | 보류 | port와 evidence type만 있고 명시적 session이 없는 production에서는 비활성이다. |
| 모바일 transfer | 보류 | 초기 완성 범위에 없다. |

## 완료 판단

현재 자동 completion gate는 통과했다. 최신 `tools/verify.ps1 -SkipInstall` 증거는 frontend 23 files/140 tests, Rust library 140 passed(외부 live smoke 1 ignored), startup 2 passed와 typecheck/build/fmt/check/clippy/whitespace, Tauri release `--no-bundle` 성공이다. 실데이터 안전 경계와 수동 검토 항목은 [KNOWN_ISSUES.md](KNOWN_ISSUES.md)에 별도로 유지한다.
