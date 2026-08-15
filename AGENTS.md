# Atsumi Next 작업 규칙

이 저장소는 Atsumi Classic과 분리된 독립 재작성이다.
상세 구현 근거와 현재 상태는 `docs/IMPLEMENTATION_HANDOFF.md`를 먼저 확인한다.

## 영구 불변식

1. Classic 원본 저장소와 Classic 사용자 데이터는 수정하지 않는다.
2. Classic 입력은 read-only snapshot 또는 명시적 export 파일로만 읽는다.
3. SQLite만 영속 상태의 canonical source로 사용한다.
4. frontend state와 event는 SQLite snapshot의 projection일 뿐이다.
5. 실제 파일·manifest·hash가 검증된 artifact만 `completed`가 될 수 있다.
6. 원본 source page number는 immutable하며 배열 index와 혼용하지 않는다.
7. Explore·Downloads·Detail·Review는 전역 `ThumbnailCoordinator` 하나를 공유한다.
8. coordinator 밖에서 화면별 이미지 worker를 만들지 않는다.
9. 검색·썸네일·다운로드는 공용 HTTP budget과 host cooldown을 공유한다.
10. raw source URL, cookie, session token과 cache 경로를 frontend에 노출하지 않는다.
11. download root 밖의 canonical path는 거부한다.
12. 자동 중복 판정만으로 사용자 파일을 영구 삭제하지 않는다.
13. 파일 제거는 quarantine과 undo를 우선한다.
14. quarantine 영구 삭제는 사용자의 명시적 명령으로만 수행한다.
15. 새 migration 전에는 일관된 DB backup을 만든다.
16. 지원 버전보다 새로운 DB schema는 아무 변경 전에 거부한다.
17. 적용된 migration 순서·version·name을 바꾸지 않는다.
18. 오류 문자열 parsing으로 상태·retry·Review 대상을 결정하지 않는다.
19. URL query, 사용자 경로와 비밀정보를 로그에 남기지 않는다.
20. manifest, HashProfile과 parser 형식에는 명시적 version을 둔다.

## 변경과 검증

- 관련 코드와 계약을 먼저 좁게 조사하고 관련 테스트를 우선 실행한다.
- fixture와 local mock server를 기본 CI에 사용하고 live smoke는 opt-in으로 분리한다.
- generated output, `.runtime`, DB, 다운로드 이미지, `target`, `dist`, `node_modules`를 commit하지 않는다.
- 사용자 변경을 폐기하거나 `git reset --hard`, 무검토 `git clean`, force push를 하지 않는다.
- milestone마다 구현·테스트·문서 갱신 뒤 논리적인 독립 commit을 만든다.
- `main` 직접 push, PR merge, release/tag 생성은 사용자 최종 검토 전 금지한다.
- 세부 recovery, schema, command와 Git 상태는 `docs/IMPLEMENTATION_HANDOFF.md`에 누적한다.
