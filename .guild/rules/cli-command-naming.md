# CLI 최상위 명령 네이밍 — 단수형 원칙

## 배경 (DEV-209 → DEV-227 로 결정 번복)

`openguild <top-level>` 명사 그룹은 `list`(기본 동작) / `new` / `show` /
`update` / `delete` 서브커맨드를 두는 패턴 — 전부 **단수형**:
`quest`, `campaign`, `template`, `backup`, `check`, `index`, `journal`.

`type` / `status` / `rule` 세 그룹은 한때 `types`/`statuses`/`rules`
복수형으로 굳어져 있었다. DEV-209 에서 처음엔 "하위호환 비용이 이득보다
커서 유지"로 결정했으나, DEV-227 에서 재검토 후 **단수형으로 통일**하기로
번복 — canonical 이름을 단수로 바꾸고, 기존 스크립트 호환을 위해
복수형은 clap alias 로 유지(`rules` → `rule` 의 alias, 등).

GUI 라우트(`/quests`, `/campaigns`, `/rules`)와 HTTP API
(`/api/quests`, `/api/campaigns`, `/api/rules`)는 반대로 **전부
복수형으로 일관** — REST/페이지 관례라 이 규칙 대상 아님.

## 규칙

- **새 CLI top-level 명사 그룹은 반드시 단수형**을 쓴다
  (`quest`/`campaign`/`template`/`backup`/`check`/`index`/`journal`/
  `type`/`status`/`rule` 패턴을 따름). `list` 는 서브커맨드(또는 sub
  생략 시 기본 동작)로 둔다.
- 과거에 복수형으로 만든 게 있다면: **canonical 이름을 단수로 리네임
  하고, 복수형은 `#[command(alias = "...")]`로 유지** — 기존 스크립트를
  깨지 않으면서 새 이름을 정착시킨다 (DEV-227 이 이 패턴의 선례).
