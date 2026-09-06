# openguild — Agent 인덱스

> AI agent 가 세션 시작 시 가장 먼저 읽는 파일.
> 이 파일은 **최소 정보 + 다른 문서로의 인덱스** 역할.
> 자세한 내용은 각 문서로 들어가서 확인할 것.

## 절대 규칙

### Git 운영

- **commit / merge 는 원칙적으로 사용자가 명시적으로 요청할 때만 — 단 하나의 예외.**
  - **예외(2026-08-08 확정, 같은 날 merge 포함으로 확장)**: 퀘스트 **구현을
    시작**하면(착수 당시 상태가 Open 이든 On Hold 든 무관) `In Progress` 로
    옮기고, **구현이 끝나** `Testing` 으로 옮기는 그 순간 **자동으로 커밋하고
    develop 에 `git merge --ff-only` 까지 한다** (둘 다 묻지 않음). 이 예외는
    딱 그 한 지점(Testing 전환 직후)에만 적용 — `Testing → Done` 전환, 여러
    퀘스트를 모아 처리하는 경우, chore 성 변경 등 **그 외의 모든 커밋/merge 는
    여전히 사용자에게 먼저 물어본다.**
  - push 는 이 예외에 포함되지 않음 — 여전히 명시 요청 시에만.
  - amend / reset / force push 는 항상 명시 요청 필요.

### 브랜치 전략 (2026-05-17 확정)

```
master    ─── 릴리즈 전용 (태그 v0.x.y, 직접 push 금지)
  ↑ release merge only
develop   ─── 통합 / 검증 (origin/develop 추적)
  ↑ feature merge
DEV-001, DEV-002, BUG-045, ...    ─── feature 브랜치 (quest_id 직접)
```

- **default 작업 분기는 `develop`**. 새 작업은 `git checkout develop && git checkout -b {QUEST_ID}`.
- **branch 이름 = quest_id**. `DEV-001`, `BUG-045` 직접 사용. `feature/` 같은 prefix 금지.
- **master 직접 commit / push 금지**. develop 에서 release 시점 머지만.
- **머지된 feature 브랜치 삭제 금지**. 사용자가 명시적으로 삭제 요청할 때까지 보존.
  히스토리/기록 목적 — `git branch -d` 자동 호출 금지.
- **머지는 fast-forward 허용** (2026-05-18 변경). 기본 `git merge {QUEST_ID}` —
  linear 히스토리면 FF, 충돌/분기 시에만 자동 merge commit. `--no-ff` 강제 사용 금지
  — log 가 머지 커밋으로 너무 지저분해짐. (기존 `--no-ff` 머지 커밋들은 그대로 유지.)

### Commit 메시지 형식

```
[{QUEST_ID}][{CATEGORY?}] 한 줄 요약 (70자 이내)

본문 (선택). 무엇(what) 보다 왜(why) 중심.

Co-Authored-By: ... (해당 시)
```

- `[QUEST_ID]` 필수 — 현재 branch 의 quest_id 와 일치.
- `[CATEGORY]` 선택 — 변경의 큰 분류:
  - `gui/desktop`, `gui/frontend`, `core`, `cli`, `server`, `docs`, `chore` 등.
- 다중 카테고리 변경은 별개 commit 으로 분리 권장.
- **메타 변경 일회성 예외**: 브랜치 전략 같이 quest 없는 메타 변경은 `[chore][docs] ...` 형식 허용.

예시:
- `[DEV-002][gui/frontend] Tauri 환경 감지 어댑터`
- `[DEV-019][server] check drift 명령 추가`
- `[BUG-045][cli] --remote env override 무시되던 문제 수정`

### 버전 / 릴리즈

- **메이저 버전 1은 사용자 명시적 승인 전까지 사용 금지.** 현재 `0.x.x`.
- 릴리즈 = develop → master 머지 + `v0.x.y` 태그.

### Dogfood — openguild 자체로 작업 관리

저장소 설계 13/13 완료 (2026-05-17). 근거는 길드 규칙 `file-truth-db-cache`
와 도서관 BOOK-001.
앞으로 할 일 / 진행 상태 추적은 **openguild 자체 (CLI / GUI) 로** 관리.

- `.guild/quests/*.md` 가 진리원 (git tracked).
- `openguild quest new/start/done/...` 으로 mutation — 파일 자동 갱신.
- 외부 todo 도구 / GitHub Issues 보조 사용 X.
- 새 작업 → 새 quest → 같은 quest_id 의 branch → commit 메시지에 그 ID.

> 📌 **[`.agents/skills/openguild-workflow/SKILL.md`](./.agents/skills/openguild-workflow/SKILL.md) 의
> 워크플로 / 규칙을 따를 것.** 특히 quest 상태 흐름과 testing 단계 처리 방식
> (자동 테스트 통과 시 done OK, 수동 검증 필요 시 testing 으로 보내고 본문에
> 테스트 방법 첨부) 을 숙지할 것.

### 🚨 `.guild/` 파일을 직접 편집 금지 (drift 방지)

agent (Claude / 다른 LLM) 가 `Write` / `Edit` 도구로 `.guild/quests/*.md` 의
**frontmatter (status / urgency / parent / prerequisites / deleted)** 를 직접
갈아끼우면 SQL 캐시 / 저널과 drift 발생 → GUI 와 파일 다르게 보임.

- **status / urgency / parent / prereq / delete 변경 = 반드시 CLI**
  (status 는 `openguild quest move`(`start`/`done`/`reopen` 단축 포함) —
  `quest status <slug>` 는 조회 전용, `<STATUS>`까지 주는 변경 형식은
  deprecated이지만 실제 mutation이므로 사용 금지;
  나머지는 `quest update / parent / prereq / delete`).
- **description 본문만** 부득이 직접 편집 가능 (multi-line 도 `quest update
  --description` 이 정상 저장 — BUG-001 수정됨, 가급적 CLI). 직접 편집 시 직후
  `openguild reindex` 필수 (GUI 는 BUG-049 후 시동 시 자동).
- **댓글 (`{slug}.comments.md`) / 메모 (`{slug}.memo.md`)** 도 **DEV-102 부터 DB
  캐시 (`quest_comments` / `quest_memos`) sync** + snapshot 백업. 직접 편집 후엔
  `drift::auto_resync` 가 자동 reindex (GUI 시동 + server / cli 진입 hook —
  BUG-049). 단 즉시 일관시키려면 명시적으로 `openguild quest comment add /
  edit / remove` 또는 `openguild quest memo set` 사용 권장. HTML 마커 (`<!-- og-comment
  id=N ts=... -->`) 포맷 깨면 parser 실패.
- 자세한 표 + 우회 절차는 [`.agents/skills/openguild-workflow/SKILL.md`](./.agents/skills/openguild-workflow/SKILL.md) 의
  "절대 금지" / "함정" 절 참조.

### status / type slug 는 stable identifier (DEV-042)

`.guild/statuses/*.toml` 와 `.guild/types/*.toml` 의 **파일명 prefix 가 slug**
(예: `5-returned.toml` → slug `returned`). 이 slug 는 quest_history,
`.md` frontmatter, 그리고 DB join 의 stable identifier.

- 상태/타입 **추가**: 새 파일 만들고 reindex — 안전.
- 상태/타입 **순서 변경**: 파일명의 sort_order prefix 만 변경 (slug 는 유지).
- 상태/타입 **slug rename**: `openguild statuses update <slug> --slug <new>` /
  `openguild types update <prefix> --prefix <new>` 가 rename + cascade
  (quest_history / 모든 .md frontmatter) 를 자동 처리. 파일명만 직접 바꾸면
  cascade 안 돼 drift — 반드시 CLI 사용.

## 한 줄 요약

RPG 테마 프로젝트 이슈 트래커. Rust(Axum) 백엔드 + Svelte 프론트엔드 + Rust CLI (`openguild`).

## 디렉토리

```
openguild/
├── Cargo.toml          ← workspace = ["core", "cli", "server", "gui"]
├── openguild.guild     ← 본 repo dogfood 마커
├── .guild/             ← 본 repo 의 quests / 캐시 (dogfood — 일부 gitignored)
├── core/               ← lib: 도메인 + 저장소 추상화
│                          repo / ops / store / snapshot / reindex /
│                          drift / counter / lock / migrate
├── cli/                ← bin `openguild` (Backend = Http | Local)
├── server/             ← bin `openguild-server` (HTTP host 전용 — DEV-163)
├── gui/                ← bin `openguild-gui` (Tauri v2, DEV-001 트리)
│   ├── src/            ← Rust shell + invoke 핸들러 23개 (commands.rs)
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── icons/          ← placeholder (PowerShell 생성)
│   └── frontend/       ← Svelte 5 + SvelteKit static (HTTP / Tauri 양쪽)
├── justfile            ← dev/build/test 단축
└── docs/               ← 사용자 문서 (USAGE) — 설계·규칙은 길드에
```

## 문서 인덱스

### Agent 가 openguild **를 개발** 할 때 (코드 수정)

| 문서 | 내용 |
|---|---|
| `docs/USAGE.md` | 사용자 매뉴얼 — 앱에 번들되고 `openguild docs show usage` 로도 열린다 |

**설계·규칙 문서는 저장소가 아니라 길드에 있다**(DEV-371). `docs/` 에는 앱에
번들되는 사용자 문서만 남긴다 — 규칙이 두 곳에 있으면 어긋나도 아무도 모른다.

| 찾는 것 | 어디서 |
|---|---|
| 개발 규칙 (커밋·브랜치·컨벤션·테마·배율·릴리스…) | `openguild rule list` / `rule show <slug>` |
| 아키텍처, 설계 배경, 조사 기록 | `openguild library list` / `library show BOOK-00N` |
| API 엔드포인트 | 코드가 정본 — `server/src/routes/mod.rs` |
| 진행 중인 일 / 계획 | `openguild quest list` · `campaign list` |

### Agent 가 openguild **를 사용** 할 때 (도구로 작업 관리)

CLI 사용법/워크플로 패턴/안전장치는 더 이상 별도 문서가 아니라 **스킬
패키지**(`skills/openguild-plugin/skills/openguild/SKILL.md` +
`reference/*.md`)로 제공된다 — Claude Code 에서 `/plugin marketplace add
~/.openguild/skill-marketplace` 로 등록. 이 repo 자체를 개발할 때 쓰는
`.agents/skills/openguild-workflow/`(위 섹션에서 참조하는 것)와는 별개의
스킬이니 혼동하지 말 것.

## 빠른 명령어 참조

| 영역 | 실행 |
|---|---|
| 서버 host | `cargo run --bin openguild-server -- host` |
| 정비/진단 | `openguild {info, backup new, restore, reindex, migrate-to-files, check counters, check drift, index vacuum, journal tail}` (또는 HTTP admin `/api/admin/*`) |
| 프론트엔드 | `cd gui/frontend && npm run dev` (또는 `just dev-frontend`) |
| CLI | `cargo run --bin openguild -- --help` (또는 `target/release/openguild`) |
| 테스트 전체 | **`just test`** — CI 와 같은 조합(러스트 + 프론트 + `npm run check` + 스타일 가드). 아래 둘만 돌리면 **가드를 건너뛴다** |
| 러스트만 / 프론트만 | `cargo test --workspace` / `cd gui/frontend && npm test -- --run` |

위 문서에 있는 내용은 여기 중복하지 않는다.
