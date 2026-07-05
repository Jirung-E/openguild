# CLI 최상위 명령 네이밍 — 단수형 원칙

## 배경 (DEV-209)

`openguild <top-level>` 명사 그룹은 대부분 **단수형**이고, 그 아래
`list`(기본 동작) / `new` / `show` / `update` / `delete` 서브커맨드를
둔다: `quest`, `campaign`, `template`, `backup`, `check`, `index`,
`journal`.

반면 `types` / `statuses` / `rules` 세 그룹만 **복수형**으로 굳어져,
top-level 자체가 목록 명령처럼 동작한다(`openguild statuses` = list).
DEV-209 에서 전수조사 후 기존 세 그룹은 **하위호환 비용이 이득보다
커서 리네임하지 않고 유지**하기로 결정함 (사용자 스크립트/문서에
이미 박혀있는 명령).

GUI 라우트(`/quests`, `/campaigns`, `/rules`)와 HTTP API
(`/api/quests`, `/api/campaigns`, `/api/rules`)는 반대로 **전부
복수형으로 일관** — REST/페이지 관례라 이 규칙 대상 아님.

## 규칙

- **새 CLI top-level 명사 그룹을 추가할 때는 반드시 단수형**을 쓴다
  (`quest`/`campaign`/`template`/`backup`/`check`/`index`/`journal`
  패턴을 따름). `list` 는 서브커맨드(또는 sub 생략 시 기본 동작)로 둔다.
- `types`/`statuses`/`rules` 는 과거 결정으로 유지되는 **예외**이지,
  새 명령의 참고 패턴이 아니다 — 이 셋을 따라 또 복수형 명령을
  만들지 말 것.
- 리네임(복수 → 단수) 은 하지 않는다 — 기존 사용자 스크립트 깨짐
  비용이 일관성 이득보다 큼(DEV-209 결정 재확인).
