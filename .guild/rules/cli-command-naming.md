# CLI 최상위 명령 네이밍 — 단수형 + sub 필수 원칙

## 배경 (DEV-209 → DEV-227 로 결정 번복)

`openguild <top-level>` 명사 그룹은 `list` / `new` / `show` / `update` /
`delete` 서브커맨드를 두는 패턴 — 전부 **단수형 + sub 필수**(서브커맨드
없이 bare 호출하면 에러): `quest`, `campaign`, `template`, `backup`,
`check`, `index`, `journal`, `rule`.

`type` / `status` / `rule` 세 그룹은 한때 `types`/`statuses`/`rules`
복수형으로 굳어져 있었다. DEV-209 에서 처음엔 "하위호환 비용이 이득보다
커서 유지"로 결정했으나, DEV-227 에서 재검토 후 **단수형으로 통일**하기로
번복 — canonical 이름을 단수로 바꾸고, 기존 스크립트 호환을 위해
복수형은 clap alias 로 유지.

**추가로**: `type`/`status` 는 리네임 직후에도 `sub: Option<...>` 라
bare `openguild type`(서브커맨드 없이) 가 조용히 list 로 떨어지는
DEV-062 이래의 관행이 남아있었다 — 다른 모든 그룹은 sub 가 필수라
bare 호출 시 에러나는데 이 둘만 예외였던 것. "단수형으로 통일했으면
list 도 명시해야 다른 그룹과 일관 아니냐"는 지적으로 sub 를 필수로
변경, bare 호출은 에러나도록 통일함.

GUI 라우트(`/quests`, `/campaigns`, `/rules`)와 HTTP API
(`/api/quests`, `/api/campaigns`, `/api/rules`)는 반대로 **전부
복수형으로 일관** — REST/페이지 관례라 이 규칙 대상 아님.

## 규칙

- **새 CLI top-level 명사 그룹은 반드시 단수형 + sub 필수**로 만든다
  (`quest`/`campaign`/`template`/`backup`/`check`/`index`/`journal`/
  `type`/`status`/`rule` 패턴). `list` 도 다른 서브커맨드와 동등하게
  명시적으로 호출해야 하며, sub 를 `Option<...>` 으로 두어 bare 호출을
  list 로 기본 처리하는 편의 기능을 넣지 않는다.
- 과거에 복수형으로 만든 게 있다면: **canonical 이름을 단수로 리네임
  하고, 복수형은 `#[command(alias = "...")]`로 유지** — 기존 스크립트를
  깨지 않으면서 새 이름을 정착시킨다 (DEV-227 이 이 패턴의 선례).
