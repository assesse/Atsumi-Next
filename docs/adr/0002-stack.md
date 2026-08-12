# ADR-0002: Tauri 2, Rust, TypeScript와 React

- 상태: 승인
- 결정일: 2026-08-12

## 맥락

앱은 Windows 전용이어도 기존 Rust의 다운로드, 이미지와 파일 처리 지식을 재사용할 가치가 있다. Classic의 Vanilla JavaScript 단일 파일은 화면과 도메인 상태가 섞여 확장 비용이 커졌다.

## 결정

- Desktop shell은 Tauri 2를 유지한다.
- backend core는 Rust로 작성한다.
- frontend는 TypeScript와 React component로 작성한다.
- 외부 도구는 향후 sidecar adapter로 격리한다.

## 결과

- 기존 실험과 Rust fixture를 활용할 수 있다.
- UI 상태와 렌더링 경계를 component 단위로 나눌 수 있다.
- React가 domain 상태의 주인이 되지 않도록 typed backend projection을 사용해야 한다.
