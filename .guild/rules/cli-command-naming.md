+++
created_at = "2026-07-05T20:30:42+09:00"
updated_at = "2026-07-05T20:30:42+09:00"
+++
# CLI 최상위 명령 네이밍 — 단수형 + sub 필수 원칙

## 배경 (DEV-209 → DEV-227 → DEV-231 로 결정 조정)

`openguild <top-level>` 명사 그룹은 `list` / `new` / `show` / `update` /
`delete` 서브커맨드를 두는 패턴 — 전부 **단수형 + sub 필수**(서브커맨드
없이 bare 호출하면 에러): `quest`, `campaign`, `template`, `backup`,
`check`, `index`, `journal`, `rule`.

`type` / `status` / `rule` 세 그룹은 한때 `types`/`statuses`/`rules`
복수형으로 굳어져 있었다. DEV-209 에서 처음엔 "하위호환 비용이 이득보다
커서 유지"로 결정했으나, DEV-227 에서 재검토 후 **단수형으로 통일**하기로
번복 — canonical 이름을 단수로 바꿨다.

**복수형 alias 처리는 그룹마다 다르다** (DEV-231, 사용자 결정):
- `type`/`status`: 복수형(`types`/`statuses`)을 clap alias 로 유지 —
  기존 스크립트 호환.
- `rule`: 복수형(`rules`) alias 를 **완전히 제거**. `rule` 만 유효,
  `openguild rules ...` 는 unknown subcommand 에러. `rule` 은 애초에
  bare 호출에 하위호환 관행이 없었으므로(`RulesCmd::sub` 가 처음부터
  `Option` 이 아니라 필수) 복수형을 남길 이유가 없다는 판단.

**추가로**: `type`/`status` 는 리네임 직후에도 `sub: Option<...>` 라
bare `openguild type`(서브커맨드 없이) 가 조용히 list 로 떨어지는
DEV-062 이래의 관행이 남아있었다 — 다른 모든 그룹은 sub 가 필수라
bare 호출 시 에러나는데 이 둘만 예외였던 것. "단수형으로 통일했으면
list 도 명시해야 다른 그룹과 일관 아니냐"는 지적으로 sub 를 필수로
변경, bare 호출은 에러나도록 통일함. alias(`types`/`statuses`) 로
불렀을 때만 예전처럼 bare = list 로 rewrite 되도록
`rewrite_legacy_plural_bare_invocation()` 을 따로 둠 — canonical
(`type`/`status`)은 계속 sub 필수.

GUI 라우트(`/quests`, `/campaigns`, `/rules`)와 HTTP API
(`/api/quests`, `/api/campaigns`, `/api/rules`)는 반대로 **전부
복수형으로 일관** — REST/페이지 관례라 이 규칙 대상 아님.

## 규칙

- **새 CLI top-level 명사 그룹은 반드시 단수형 + sub 필수**로 만든다
  (`quest`/`campaign`/`template`/`backup`/`check`/`index`/`journal`/
  `type`/`status`/`rule` 패턴). `list` 도 다른 서브커맨드와 동등하게
  명시적으로 호출해야 하며, sub 를 `Option<...>` 으로 두어 bare 호출을
  list 로 기본 처리하는 편의 기능을 넣지 않는다.
- 과거에 복수형으로 만든 게 있다면 canonical 이름을 단수로 리네임한다.
  **복수형을 alias 로 남길지는 케이스별로 판단** — 기존에 bare 호출로
  쓰이던 관행이 있었다면 alias 로 유지(`type`/`status`), 그런 관행이
  없었다면 alias 없이 완전히 제거해도 된다(`rule`). 무조건 다 남기는
  게 아니다.
