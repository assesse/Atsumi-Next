# Atsumi Next

## 상태

Atsumi Next는 기존 Atsumi를 보존하면서 새 구조로 재작성하는 독립 프로젝트다.

승인된 UX prototype과 V2 계약을 기준으로 `Phase 2: Core foundation`을 완료했고, 현재 `Phase 3A`를 진행 중이다. UI는 저장 fixture 기반 검색·페이지·상세 command와 SQLite queue/list projection을 실제 typed client로 호출한다. queue snapshot과 event는 revision으로 병합하며, 실제 파일 없이 완료 상태를 만들던 production mock command는 제거했다. retry/cancel과 attempt 이력은 SQLite에 영속되고, 탐색·다운로드·상세·중복 검토의 미리보기는 하나의 전역 thumbnail coordinator에서 우선순위·중복 요청·취소·cache를 공유한다.

아직 실제 Hitomi HTTP 검색·다운로드, artifact 검증·재개·reconcile, 완료 파일 열기는 구현되지 않았다. 현재 fixture queue는 `queued -> resolving_metadata -> interrupted`까지만 진행해 원격 artifact pipeline이 없다는 사실을 명시적으로 보존한다. Auto Find의 원격 갱신, 작품 중복 판정, 내부 페이지 중복 판정도 각각 후속 계약이 확정될 때까지 fixture 또는 비활성 상태로 남긴다.

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
- Classic 저장소: 별도 보존된 로컬 `PUPIL` 저장소(개인 PC의 절대 경로는 문서화하지 않음)
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

## 기존 명세와의 관계

Classic 루트의 다음 문서는 사실 확인 자료로 유지한다.

- `PRODUCT_OVERVIEW.md`
- `USER_FLOWS.md`
- `FRONTEND_BACKEND_CONTRACT.md`
- `BACKEND_REQUIREMENTS.md`
- `DATA_MODEL.md`
- `TEST_PLAN.md`

이 문서들은 Classic의 현재 구조를 설명한다. Atsumi Next의 최종 계약으로 자동 승격하지 않는다.
