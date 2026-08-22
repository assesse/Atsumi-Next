# 전달 계획

2026-08-21 현재 production 기능과 schema v18 안정화를 기준으로 한다. 이 문서의 `완료`는 아래 gate와 실제 검증 증거를 통과했다는 뜻이며, 외부 source 전체 호환성이나 자동 영구 삭제처럼 구현하지 않은 범위를 포함하지 않는다.

## 공통 completion gate

각 Phase는 다음 조건을 모두 만족해야 완료로 유지한다.

1. Rust/TypeScript DTO와 stable error code가 문서 계약과 일치한다.
2. canonical state는 SQLite이고 event/frontend state는 복원 가능한 projection이다.
3. filesystem 변경은 download root containment, 검증, revision 또는 saga 기록과 복구 경로를 가진다.
4. 취소는 표시만 바꾸지 않고 background 작업에 도달하며 늦은 완료가 최신 상태를 덮지 않는다.
5. migration은 additive하고 적용 전 backup, 순서/idempotency, future-schema write 거부 테스트가 있다.
6. fixture 기반 자동 검증과 별도의 opt-in live 증거를 구분한다.
7. 사용자가 이미 가진 artifact를 추측해서 이동·이름 변경·삭제하지 않는다.

## Phase 3 — production 수직 다운로드

상태: 완료.

Gate:

- `Explore -> Detail -> Queue -> Download -> Resume -> Complete -> Open`이 production `HitomiLiveAdapter`와 같은 pooled HTTP scheduler를 사용한다.
- source page identity, `.part`, bounded decode, lossless WebP, SHA-256, atomic rename과 manifest schema 1이 모두 맞은 뒤에만 completed가 된다.
- retry/cancel은 같은 entry/job attempt graph를 사용하고 restart는 verified checkpoint부터 재개한다.
- 전역 `ThumbnailCoordinator` 하나가 gallery/page/artifact preview를 dedupe·우선순위·취소·cache한다.

추가 안정화:

- viewport churn은 400ms orphan grace와 완료 asset 120초/256개 retention으로 같은 preview를 재사용한다.
- Hitomi image candidate diagnostic은 형식/status/content-type/retryability를 남긴다. WebP/JPEG/PNG와 experimental AVIF를 지원하고 JPEG XL은 typed unsupported다.
- 2026-08-20 opt-in live smoke는 gallery `4113714`의 18/18 page를 WebP로 검증했으며 선택 payload 합계는 12,396,942 bytes였다.

## Phase 4 — 즐겨찾기·검색 이력·Auto Find

상태: 완료(명시된 대형 작가 제한 포함).

Gate:

- 5개 namespace 즐겨찾기, 성공한 명시적 검색 이력, run/candidate/exclusion을 SQLite에서 복원한다.
- 작가 갱신은 사용자 명령으로만 시작하고 동시에 하나의 run만 허용하며 cancel/exit/interruption 뒤 부분 후보를 보존한다.
- download entry, 명시적 제외, hidden/resolved duplicate/pair 제외를 canonical record로 후보에서 제거한다.

추가 안정화:

- run마다 `include_all_history|newer_than_oldest_downloaded`를 snapshot한다.
- cutoff는 검증 소유 artifact만 사용하고 `source=verified_owned_artifact`, `policyVersion=1`을 영속한다. 증거가 없으면 cutoff하지 않는다.
- source Nozomi ID에 cutoff를 먼저 적용한 뒤 최대 50,000 candidate를 처리하며 초과는 `candidate_limit_after_cutoff`로 기록한다. 예전 작가당 250-page 상한은 폐기했다.

## Phase 5 — 작품 단위 중복 Review

상태: 완료.

Gate:

- 최신 verified local artifact만 versioned SHA/perceptual/detail/edge evidence 대상으로 삼는다.
- metadata는 전수 pair 작업 우선순위일 뿐 recall을 제한하지 않고, page matching은 monotonic one-to-one이다.
- candidate/evidence/decision/history는 SQLite에 남으며 hide/series/pair-exclude는 revision CAS transaction이다.
- Review는 live URL이 아니라 root-bound `artifactPage(entryId, sourcePage)`를 사용하고 자동 파일 삭제하지 않는다.

## Phase 6 — 앨범 내부 페이지 중복

상태: 완료.

Gate:

- exact 반복 또는 최소 2행의 단조 visual block만 Review 후보가 된다.
- original source page number와 검증 metadata를 유지하며 제거 전 plan이 revision, 파일 수와 byte 합계를 고정한다.
- apply/undo는 artifact 내부 quarantine, manifest atomic replace와 SQLite saga로 crash 후 수렴한다.
- 모호한 원본/격리 위치는 overwrite/delete하지 않는다.

## Phase 7 — 운영 안정화와 복구

상태: 완료.

Gate:

- cache와 탐색 데이터 초기화는 typed 범위·명시적 확인·단일 transaction을 사용하고 다운로드 DB/파일을 보존한다.
- migration 전 일관 backup, future-schema 거부와 과거 migration 불변을 유지한다.
- artifact 경로 변경·격리·복원은 저장된 root/path와 revision 또는 saga를 사용한다.
- 과거 데이터 이전 기능의 active UI/API/runtime 경로는 제거하고 역사적 v14 migration/table만 기존 DB 호환을 위해 보존한다.

## v15~v18 안정화 gate

- v15: 새 artifact용 folder template과 기존 relative path immutability. `{id}` 필수와 Windows path 한도를 테스트했다. 기존 artifact 자동 rename은 없다.
- v16: page candidate diagnostic과 immutable artifact root snapshot. v15 legacy path/root backfill을 보존한다.
- v17: Auto Find history mode, verified-owned artist/cutoff evidence와 truncation.
- v18: remote source revision 문자열 identity와 작은 SQLite 내부 revision을 분리한다. `u64::MAX` 회귀 test가 signed integer overflow 없이 다운로드 완료를 검증한다.
- v14→latest migration은 역사적 v14 table과 15/16/17/18 순서, legacy path/root immutability를 함께 검증한다.
- Explore: query별 settled cache 최대 5, 현재 page ±2, 인접 prefetch, page scroll restore. `search_page_cancel`은 active token과 최대 256 cancel-before-start tombstone으로 실제 backend 작업을 취소한다.
- UI: 가로 밀도형 adaptive card는 점수·날짜를 제거한다. 160/190/220/250/280/320/360px preset, 행별 최대 intrinsic cover 높이, preset typography와 2/2/3/4/5/6/7줄 태그 예산을 공유한다.

## 현재 검증 증거

`tools/verify.ps1 -SkipInstall`의 최신 성공 로그는 `.runtime/verification/verify-20260821-011639.log`다. 아래 test/type/build/fmt/check/clippy/whitespace와 Tauri release `--no-bundle` gate를 통과했다.

- frontend: 23 files, 140 tests
- Rust library: 140 passed, opt-in live 1 ignored
- startup binary: 2 passed
- typecheck, Vite production build, Rust fmt/check/clippy, Tauri release `--no-bundle`, whitespace: 통과

과거 live gallery 4113714의 18/18 WebP·12,396,942 bytes 결과는 일반 fixture CI와 별도 증거다. 이번 2026-08-21 검증은 네트워크 없이 `u64::MAX` source identity 회귀를 포함했으며 AVIF 대표 corpus나 JPEG XL decode 완료를 뜻하지 않는다.

## 다음 작업

1. 실제 사용자 download root에서 새 folder template 결과와 기존 artifact 불변을 수동 확인한다.
2. 대표 AVIF gallery corpus를 수집하지 않고도 재현 가능한 합법 fixture/opt-in test 경계를 정한다.
3. JPEG XL decoder 도입 여부를 memory/supply-chain/maintenance 평가 뒤 결정한다. 현재는 unsupported를 유지한다.
4. 기존 artifact relocation/rename이 정말 필요하면 별도의 dry-run, revisioned journal, rollback 설계를 먼저 승인한다.
5. quarantine 영구 비우기는 대상/용량/복구 불가 확인 UX가 승인되기 전까지 구현하지 않는다.

세부 제한과 rollback은 [KNOWN_ISSUES.md](KNOWN_ISSUES.md), 계약은 [API_CONTRACT_V2.md](API_CONTRACT_V2.md), 실제 인계 상태는 [IMPLEMENTATION_HANDOFF.md](IMPLEMENTATION_HANDOFF.md)를 기준으로 한다.
