# Atsumi Next

## 상태

Atsumi Next는 기존 Atsumi를 보존하면서 새 구조로 재작성하는 독립 프로젝트다.

현재 단계는 `Phase 1: UX prototype과 V2 계약`이다. 아직 production Rust crate와 실제 Hitomi 연결은 만들지 않는다.

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
- Classic 저장소: `C:\Users\정재호\Documents\PUPIL`
- Classic 보존 commit: `3b3bedd Preserve Atsumi Classic baseline before rewrite`
- Classic 보존 tag: `atsumi-classic-baseline-2026-08-12`
- 초기 설계 보존 branch: Classic 저장소의 `codex/atsumi-next`
- 초기 설계 보존 commit: `7c0a773 Define Atsumi Next architecture and UX prototype`
- 현재 프로젝트 branch: `main`
- 현재 프로젝트는 Classic의 포크가 아니라 별도 Git 저장소이며 원격 저장소는 아직 연결하지 않았다.
- Classic 기준선은 frontend production build와 Rust 15개 unit test를 통과했다.
- Classic 코드는 참조 및 데이터 이전 입력으로만 사용하며 새 구현 코드를 Classic 저장소에 추가하지 않는다.

## 문서 지도

- [BASELINE_AUDIT.md](docs/BASELINE_AUDIT.md): 현재 코드와 상태 저장 구조 감사
- [PRODUCT_SCOPE.md](docs/PRODUCT_SCOPE.md): 새 제품의 목표, 비목표, 첫 완성 범위
- [FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md): Classic 기능의 유지, 재설계, 보류 분류
- [UX_ARCHITECTURE.md](docs/UX_ARCHITECTURE.md): 정보 구조와 주요 화면 흐름
- [SYSTEM_ARCHITECTURE.md](docs/SYSTEM_ARCHITECTURE.md): 프론트와 백엔드의 새 경계
- [DATA_MIGRATION.md](docs/DATA_MIGRATION.md): 데이터 소유권과 이전 원칙
- [INCIDENT_AND_LESSONS.md](docs/INCIDENT_AND_LESSONS.md): 문제, 원인, 해결 이력과 회귀 조건
- [DECISION_REGISTER.md](docs/DECISION_REGISTER.md): 확정 사항과 사용자 승인 대기 사항
- [DELIVERY_PLAN.md](docs/DELIVERY_PLAN.md): 단계별 산출물과 구현 진입 조건

## 기존 명세와의 관계

Classic 루트의 다음 문서는 사실 확인 자료로 유지한다.

- `PRODUCT_OVERVIEW.md`
- `USER_FLOWS.md`
- `FRONTEND_BACKEND_CONTRACT.md`
- `BACKEND_REQUIREMENTS.md`
- `DATA_MODEL.md`
- `TEST_PLAN.md`

이 문서들은 Classic의 현재 구조를 설명한다. Atsumi Next의 최종 계약으로 자동 승격하지 않는다.
