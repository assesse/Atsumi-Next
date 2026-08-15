# Atsumi Next

## 상태

Atsumi Next는 기존 Atsumi를 보존하면서 새 구조로 재작성하는 독립 프로젝트다.

승인된 UX prototype과 V2 계약을 기준으로 Phase 3의 실제 다운로드 흐름, Phase 4의 영속 즐겨찾기·검색 이력·Auto Find, Phase 5의 작품 중복 Review, Phase 6의 앨범 내부 중복 페이지 검토·격리·undo, Phase 7의 Classic read-only 가져오기·rollback을 구현했다. Tauri production 경로는 실제 Hitomi 검색·상세·미리보기·페이지 다운로드 adapter를 사용하고, 브라우저 검토 모드와 자동 테스트만 저장 fixture를 사용한다. 탐색·다운로드·상세·Review의 미리보기는 하나의 전역 thumbnail coordinator를 공유하며, 검색·미리보기·다운로드·Auto Find는 같은 pooled HTTP scheduler의 host 제한·우선순위·취소·bounded retry 정책을 사용한다.

다운로드는 SQLite queue에서 자동 시작해 source page 번호별 `.part` 기록, decode, WebP 저장, SHA-256, atomic rename, versioned manifest 검증을 마친 뒤에만 `completed`가 된다. 강제 종료된 작업은 검증된 page checkpoint부터 재개하며, 시작 시와 Downloads의 수동 명령에서 DB·manifest·실제 파일을 재조정한다. 완료 파일은 Windows 기본 뷰어로 열 수 있고, 삭제 대신 download root 내부의 crash-safe quarantine으로 옮긴 뒤 복원할 수 있다. 자동 영구 삭제는 하지 않는다.

즐겨찾기는 작가·그룹·시리즈·캐릭터·태그를 SQLite에 저장하고 카드·상세·Related에서 같은 상태로 표시한다. 시리즈와 캐릭터도 실제 Hitomi namespace 검색으로 연결한다. Auto Find는 사용자가 `즐겨찾기 작가 갱신`을 명시적으로 실행할 때만 실제 source를 조회하고, 실행 진행률·취소·오류와 후보를 영속해 재시작 뒤에도 복원한다. 검색어를 입력하는 동안에는 원격 요청하지 않으며 검색 제출만 이력에 기록한다. 이미 다운로드했거나 사용자가 제외·숨김·중복 판정한 항목은 후보에서 숨긴다.

작품 중복 검사는 검증 완료된 로컬 artifact만 읽어 exact SHA-256, 64-bit perceptual hash, 1024-bit detail hash, 밝기 분산·edge gate와 단조 1:1 gap-tolerant 정렬을 versioned evidence로 저장한다. 제목·작가·그룹 metadata는 전수 비교 작업의 우선순위를 정하되 후보를 누락시키지 않는다. Review는 실제 source page 번호의 로컬 artifact preview, confidence와 판정 이력을 보여 주며 숨김·연작 연결/해제·pair 제외를 revision CAS transaction으로 적용한다. 자동 판정만으로 파일을 삭제하지 않는다. E-Hentai relation port는 명시적 적법 세션이 없는 기본 production 설정에서 비활성화된다.

앨범 내부 중복 검사는 같은 verified artifact 안에서 정확한 SHA 반복과 최소 2행의 단조 시각 장면 블록만 Review에 올린다. 사용자는 각 동기화 행에서 유지할 원본 페이지를 고르고 파일 수·용량이 고정된 revision-CAS 계획을 먼저 확인한다. 적용은 앨범 폴더의 `.atsumi-page-quarantine/<plan-id>/`로만 이동하며 manifest와 SQLite가 crash-safe saga로 조정된다. 원본 페이지 번호는 바꾸지 않고 격리 이력에서 복원할 수 있으며 자동 영구 삭제는 없다.

설정의 저장 공간에서 Classic 데이터·다운로드 폴더를 직접 고르면 먼저 읽기 전용 inventory와 충돌 보고서를 만든다. state, manifest, 실제 페이지, legacy hash provenance, 즐겨찾기·검색 이력·제외·연작·오탐 pair를 검토하고 명시적으로 승인한 안전 항목만 Next에 등록한다. Classic 페이지는 이동하지 않고 검증·WebP 변환한 복사본만 `gallery-{id}`에 만들며, rollback은 Next 복사본만 관리 quarantine으로 옮긴다. 중단된 적용도 다음 시작에서 Next 부분 복사본을 격리한다.

## 실행과 검증

### 검토용 실행 (`pnpm` 설치 불필요)

새 앱은 Classic 저장소와 분리된 이 저장소에 있다. 일상적인 검토는 탐색기에서 프로젝트 루트의 `start-app.vbs`를 더블클릭한다. 콘솔이나 브라우저 없이 실제 Tauri 앱만 열린다. 소스가 바뀐 경우 release 실행 파일을 백그라운드에서 먼저 갱신하고, 실행 출력은 `.runtime/app-launch.log`에 저장한다. 실패할 때만 로그 위치를 알려 주는 안내창이 표시된다.

`start-dev.cmd`는 빌드 출력을 화면에서 직접 확인해야 할 때 사용하는 진단용 실행기다.

```bat
start-app.vbs
```

이 실행기는 전역 `pnpm`을 요구하지 않는다. 시스템 Node.js가 없으면 Codex Desktop의 bundled Node.js를 사용하고, Rust/Cargo는 PATH 또는 `%USERPROFILE%\.cargo\bin`에서 찾는다. Tauri 앱을 새로 빌드하려면 Rust 외에 MSVC C++ Build Tools와 Windows SDK가 필요하다. 첫 실행 또는 소스 변경 뒤에는 빌드 시간만큼 앱 창이 늦게 나타날 수 있으며, 이후 실행은 만들어 둔 release 실행 파일을 바로 연다.

### 개발 및 자동 검증

권장 개발 도구는 Node.js 24.x, `pnpm` 11.16.0, Rust 1.88 이상 stable이다. `package.json`의 `packageManager`와 CI가 같은 `pnpm` 버전을 고정하므로 다른 버전으로 lockfile을 갱신하지 않는다.

```powershell
pnpm install --frozen-lockfile
pnpm run dev
pnpm run test
pnpm run typecheck
pnpm run build
rustup toolchain install stable --profile minimal --component rustfmt,clippy
./tools/verify.ps1
pnpm tauri dev
```

이 저장소의 PowerShell 실행기는 시스템 Node.js를 우선 사용하고, 없으면 Codex Desktop의 bundled Node.js를 찾는다.
일상적인 사용자 검토는 `start-app.vbs`로 앱을 직접 실행한다. `start-dev.cmd`는 오류 진단용으로 남겨 둔다. MSI/Setup 번들은 명시적인 릴리스 요청이 있을 때만 만든다.

GitHub Actions의 `Windows CI`는 push와 pull request마다 `windows-latest`에서 Node.js 24, 정확히 `pnpm` 11.16.0, Rust stable을 사용한다. 로컬과 같은 `tools/verify.ps1`로 frozen lockfile 설치, frontend test/typecheck/build, Rust fmt/check/test/clippy, release no-bundle build, whitespace 검사를 모두 통과해야 한다. CI token 권한은 저장소 읽기로 제한하고 pnpm store와 Cargo 의존성·빌드 출력만 캐시한다. 검증 로그는 Git에서 제외된 `.runtime/verification/`에 남는다.

## 절대 원칙

1. 기존 Atsumi Classic의 코드와 사용자 데이터는 Atsumi Next가 직접 수정하지 않는다.
2. 새 버전은 기존 데이터의 복사본을 읽고 변환하며, 원본을 제자리에서 마이그레이션하지 않는다.
3. 현재 동작, 최신 사용자 결정, 실제 로그와 파일, 과거 대화 순으로 요구사항의 신뢰도를 판단한다.
4. 과거 요구사항은 모두 유지 대상으로 간주하지 않는다. `유지`, `재설계`, `폐기`, `확인 필요`로 분류한다.
5. 파괴적 작업은 명시적인 사용자 결정, 실행 기록, 복구 경로를 가져야 한다.
6. 화면 시안 승인 전에 최종 UI를 구현하지 않는다.
7. 백엔드 command 계약 승인 전에 기능별 구현을 시작하지 않는다.

## Classic 기준선 주의사항

- 감사일: 2026-08-12
- Classic 저장소: 별도 보존된 로컬 `PUPIL` 저장소(개인 PC의 실제 경로는 문서화하지 않음)
- Classic 보존 commit: `3b3bedd Preserve Atsumi Classic baseline before rewrite`
- Classic 보존 tag: `atsumi-classic-baseline-2026-08-12`
- 초기 설계 보존 branch: Classic 저장소의 `codex/atsumi-next`
- 초기 설계 보존 commit: `7c0a773 Define Atsumi Next architecture and UX prototype`
- Atsumi Next 원격 저장소: [`assesse/Atsumi-Next`](https://github.com/assesse/Atsumi-Next) (`origin`)
- 기본 branch는 `main`이며, 기능 작업은 `agent/*` branch와 pull request를 거쳐 병합한다.
- Classic 기준선은 frontend production build와 Rust 15개 unit test를 통과했다.
- Classic 코드는 참조 및 데이터 이전 입력으로만 사용하며 새 구현 코드를 Classic 저장소에 추가하지 않는다.
- 앱 브랜드에는 Aluminum Classic의 `atsumi.svg`, `atsumi-256.png`, `icon.ico` 원본만 복사해 사용한다. Pupil APK 추출 자원은 사용하지 않는다.
- 국가 표시는 Aluminum Classic 릴리스가 실제 UI에서 사용한 FlagCDN `kr.png`, `jp.png`, `us.png` 바이트를 로컬 번들로 고정한다. Classic에 없던 중국어 badge는 표시하지 않는다.

## 문서 지도

- [BASELINE_AUDIT.md](docs/BASELINE_AUDIT.md): 현재 코드와 상태 저장 구조 감사
- [PRODUCT_SCOPE.md](docs/PRODUCT_SCOPE.md): 새 제품의 목표, 비목표, 첫 완성 범위
- [FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md): Classic 기능의 유지, 재설계, 보류 분류
- [UX_ARCHITECTURE.md](docs/UX_ARCHITECTURE.md): 정보 구조와 주요 화면 흐름
- [UX_INTERACTION_MATRIX.md](docs/UX_INTERACTION_MATRIX.md): 화면별 포인터·키보드 상호작용 계약
- [MULTI_SELECTION_RESEARCH.md](docs/MULTI_SELECTION_RESEARCH.md): 카드 내부 action과 충돌하지 않는 다중 선택 mode 조사·권장안
- [SYSTEM_ARCHITECTURE.md](docs/SYSTEM_ARCHITECTURE.md): 프론트와 백엔드의 새 경계
- [API_CONTRACT_V2.md](docs/API_CONTRACT_V2.md): command·event·DTO 계약
- [ERROR_CATALOG.md](docs/ERROR_CATALOG.md): 안정 오류 code와 사용자 행동
- [DATA_MIGRATION.md](docs/DATA_MIGRATION.md): 데이터 소유권과 이전 원칙
- [INCIDENT_AND_LESSONS.md](docs/INCIDENT_AND_LESSONS.md): 문제, 원인, 해결 이력과 회귀 조건
- [DECISION_REGISTER.md](docs/DECISION_REGISTER.md): 확정 사항과 사용자 승인 대기 사항
- [DELIVERY_PLAN.md](docs/DELIVERY_PLAN.md): 단계별 산출물과 구현 진입 조건
- [IMPLEMENTATION_HANDOFF.md](docs/IMPLEMENTATION_HANDOFF.md): 실제 구현·검증·복구·Git 전달 상태
- [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md): 앱에 포함된 외부 자산과 라이선스 고지

## 기존 명세와의 관계

Classic 루트의 다음 문서는 사실 확인 자료로 유지한다.

- `PRODUCT_OVERVIEW.md`
- `USER_FLOWS.md`
- `FRONTEND_BACKEND_CONTRACT.md`
- `BACKEND_REQUIREMENTS.md`
- `DATA_MODEL.md`
- `TEST_PLAN.md`

이 문서들은 Classic의 현재 구조를 설명한다. Atsumi Next의 최종 계약으로 자동 승격하지 않는다.
