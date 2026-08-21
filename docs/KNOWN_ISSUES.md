# 알려진 제한과 운영 위험

2026-08-21, DB schema v18 working tree를 기준으로 작성했다. 이 문서는 구현되지 않은 기능을 완료로 보이게 하지 않고, 자동 검증과 실데이터 검증의 경계를 기록한다.

- 전체 태그 catalog는 수동 최신화 방식이다. Explore 입력 중에는 Hitomi에 요청하지 않으며, 아직 최신화하지 않은 새 설치에서는 tag suggestion이 비어 있을 수 있다.

## 이미지 형식

- AVIF decode는 `avif-rust 0.0.6`과 `bin-rs 0.0.10`에 정확히 고정된 순수 Rust 경로다. dimension 16,384, RGBA allocation 256MiB 등 입력 제한과 panic 격리를 적용했지만 experimental이며 대표적인 AVIF 실데이터 corpus 검증은 남아 있다.
- JPEG XL 후보는 식별·진단만 한다. decoder는 없고 HTTP fetch 전에 지원 불가 diagnostic을 남긴 뒤 WebP/JPEG/PNG/AVIF fallback을 계속 시도한다. 지원 가능한 후보도 없으면 non-retryable `IMAGE_FORMAT_UNSUPPORTED`다.
- 2026-08-20 opt-in live smoke는 gallery `4113714`의 18/18 page를 WebP로 저장·다시 검증했고 선택 payload 합계는 12,396,942 bytes였다. 한 gallery의 현재 source 증거일 뿐 전체 Hitomi corpus, AVIF 또는 JXL 호환성 보증은 아니다.

## 파일 위치와 이름

- `folderNameTemplate`은 새 download artifact에만 적용된다. 이미 DB에 등록된 `download_artifacts.relative_directory`와 `root_snapshot`은 immutable trigger로 보호된다.
- 기존 artifact를 새 template에 맞춰 자동 rename/move하는 기능은 없다. 파일만 수동 이동하면 DB·manifest·root snapshot이 달라져 Review 또는 reconcile 오류가 되므로 실행하지 않는다.
- download root 설정을 바꿔도 기존 artifact 작업은 저장된 `root_snapshot`을 사용한다. 기존 root를 분리·이동하려면 별도의 revisioned migration/relocation 설계와 rollback이 먼저 필요하다.
- Windows `canonicalize()`가 만든 `\\?\` prefix는 내부 containment와 기존 `root_snapshot`에 남을 수 있다. 설정 API와 input만 안전한 drive/UNC 표시 형식으로 바꾸며 기존 snapshot, manifest, 폴더를 일괄 재작성하지 않는다.

## Source revision과 특정 gallery 장애

- gallery 4113714/4132312 다운로드 불가의 공통 원인은 unsigned source fingerprint를 SQLite signed integer revision으로 변환하던 경로였다. fingerprint가 `i64::MAX`보다 크면 page I/O 전에 `DATABASE_ERROR`로 끝날 수 있었다.
- schema v18은 remote revision을 문자열 `source_revision` identity로 저장하고 signed `galleries.revision`은 작은 내부 snapshot revision으로만 사용한다. `u64::MAX` 회귀 test로 변환 오버플로를 차단했다.
- 이 수정은 두 gallery의 당시 실패 경로를 제거하지만 외부 source가 이후 응답 형식·호스트·이미지 후보를 바꾸는 별도 장애까지 보증하지 않는다. live smoke는 계속 opt-in이며 일반 CI에서 네트워크를 사용하지 않는다.

## Auto Find 범위

- `newer_than_oldest_downloaded`는 gallery ID 순서를 source history proxy로 사용한다. 검증 완료 또는 격리된 artifact의 소유 작가 연결만 증거가 되며 source는 `verified_owned_artifact`, policyVersion은 1이다. provenance가 없으면 안전하게 전체 이력을 포함한다.
- v17의 legacy backfill은 기존 gallery의 `primary_artist`만 보수적으로 연결한다. 추가 artist가 과거 row에 없던 경우를 추측하지 않는다.
- cutoff 적용 뒤 candidate가 50,000개를 넘으면 나머지를 조회하지 않고 `candidate_limit_after_cutoff` truncation을 영속한다. 이 제한을 무제한 전체 조회로 표현하면 안 된다.

## 외부 서비스와 수동 검토

- E-Hentai relation provider는 명시적으로 제공된 적법 session이 없으면 비활성이다. session/cookie를 SQLite·manifest·로그에 저장하지 않는다.
- 과거 데이터 이전의 active UI/API/runtime 경로는 제거됐다. v14 migration과 역사적 table은 기존 DB 호환 때문에 남지만 새 이전 기능으로 사용할 수 없다.
- quarantine은 복구 기능이지 휴지통 자동 정리 기능이 아니다. 안전한 purge 계획·재확인이 없으므로 자동 영구 삭제하지 않는다.

## Rollback과 복구

- v15/v16/v17/v18은 additive migration이지만 DB schema downgrade는 지원하지 않는다. 오래된 binary가 v18 DB를 열면 `DATABASE_SCHEMA_NEWER`로 쓰기 전에 거부해야 한다.
- 실제 downgrade가 필요하면 migration 직전 자동 backup을 보존하고, 앱을 종료한 상태에서 해당 backup과 호환 binary를 함께 복원한다. 운영 DB에 수동 `ALTER`/trigger 제거를 적용하지 않는다.
- 기존 artifact path는 rollback에서도 자동 재명명하지 않는다. artifact/manifest 불일치는 시작 시 전체 검사하지 않고 사용자 명시 `app_reconcile`과 typed Review에서 확인한다. 원본과 격리 위치가 모호하면 overwrite/delete하지 않는다.
- 탐색 데이터 초기화는 다운로드·artifact rollback 수단이 아니다. favorites/history/Auto Find 데이터만 제거하며 download DB와 파일은 그대로 둔다.

## 완료 증거의 경계

최신 전체 검증은 `tools/verify.ps1 -SkipInstall`로 실행했고 `.runtime/verification/verify-20260821-011639.log`에 있다. frontend 23 files/140 tests, Rust library 140 passed/1 opt-in live ignored, startup 2 passed, typecheck/build/fmt/check/clippy/whitespace와 Tauri release `--no-bundle`이 성공했다. live gallery smoke는 일반 CI에서 의도적으로 opt-in이며 위 단일 gallery 결과를 별도 증거로 기록한다.
