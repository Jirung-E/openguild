# openguild — Agent 인덱스

> AI agent 가 세션 시작 시 가장 먼저 읽는 파일.
> 이 파일은 **최소 정보 + 다른 문서로의 인덱스** 역할.
> 자세한 내용은 각 문서로 들어가서 확인할 것.

## 절대 규칙

### Git 운영

- **commit / push 는 사용자가 명시적으로 요청할 때만.**
  - "커밋해" / "push 해" 같은 직접 지시가 있을 때만 실행.
  - 작업 완료 후 자동 커밋 금지. amend / reset / force push 도 명시 요청 필요.
  - 변경 사항이 stage 되어 있어도 사용자가 다음 행동을 결정하게 둘 것.

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
- `[DEV-019][server] check-drift 명령 추가`
- `[BUG-045][cli] --remote env override 무시되던 문제 수정`

### 버전 / 릴리즈

- **메이저 버전 1은 사용자 명시적 승인 전까지 사용 금지.** 현재 `0.x.x`.
- 릴리즈 = develop → master 머지 + `v0.x.y` 태그.

### Dogfood — openguild 자체로 작업 관리

저장소 설계 (`docs/storage-design.md`) 13/13 완료 (2026-05-17).
앞으로 할 일 / 진행 상태 추적은 **openguild 자체 (CLI / GUI) 로** 관리.

- `.guild/quests/*.md` 가 진리원 (git tracked).
- `openguild quest new/start/done/...` 으로 mutation — 파일 자동 갱신.
- 외부 todo 도구 / GitHub Issues 보조 사용 X.
- 새 작업 → 새 quest → 같은 quest_id 의 branch → commit 메시지에 그 ID.

> 📌 **[`docs/AGENTS_OPENGUILD_USAGE.md`](./docs/AGENTS_OPENGUILD_USAGE.md) 의
> 워크플로 / 규칙을 따를 것.** 특히 quest 상태 흐름과 testing 단계 처리 방식
> (자동 테스트 통과 시 done OK, 수동 검증 필요 시 testing 으로 보내고 본문에
> 테스트 방법 첨부) 을 숙지할 것.

### 🚨 `.guild/` 파일을 직접 편집 금지 (drift 방지)

agent (Claude / 다른 LLM) 가 `Write` / `Edit` 도구로 `.guild/quests/*.md` 의
**frontmatter (status / urgency / parent / prerequisites / deleted)** 를 직접
갈아끼우면 SQL 캐시 / 저널과 drift 발생 → GUI 와 파일 다르게 보임.

- **status / urgency / parent / prereq / delete 변경 = 반드시 CLI**
  (`openguild quest status / update / parent / prereq / delete`).
- **description 본문만** 부득이 직접 편집 가능 (BUG-001 우회). 그 경우 직후
  `openguild-server reindex` 필수.
- 자세한 표 + 우회 절차는 [`docs/AGENTS_OPENGUILD_USAGE.md` § 4](./docs/AGENTS_OPENGUILD_USAGE.md) 의
  "🚨 `.guild/` 파일을 직접 편집하지 말 것" 절 참조.

### status / type slug 는 stable identifier (DEV-042)

`.guild/statuses/*.toml` 와 `.guild/types/*.toml` 의 **파일명 prefix 가 slug**
(예: `5-returned.toml` → slug `returned`). 이 slug 는 quest_history,
`.md` frontmatter, 그리고 DB join 의 stable identifier.

- 상태/타입 **추가**: 새 파일 만들고 reindex — 안전.
- 상태/타입 **순서 변경**: 파일명의 sort_order prefix 만 변경 (slug 는 유지).
- 상태/타입 **slug rename**: 위험 — quest_history.old/new_value 와 .md
  frontmatter 의 status 필드도 동시 갱신 필요. 가급적 피하고, 어쩔 수 없다면
  helper 가 생기기 전까진 수동 일괄 변경 + reindex.

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
├── server/             ← bin `openguild-server` (HTTP + 관리 CLI)
├── gui/                ← bin `openguild-gui` (Tauri v2, DEV-001 트리)
│   ├── src/            ← Rust shell + invoke 핸들러 23개 (commands.rs)
│   ├── tauri.conf.json
│   ├── capabilities/
│   ├── icons/          ← placeholder (PowerShell 생성)
│   └── frontend/       ← Svelte 5 + SvelteKit static (HTTP / Tauri 양쪽)
├── justfile            ← dev/build/test 단축
└── docs/               ← 기획·설계·사용 문서
```

## 문서 인덱스

### Agent 가 openguild **를 개발** 할 때 (코드 수정)

| 문서 | 내용 |
|---|---|
| `docs/architecture.md` | 시스템 구조 / API 엔드포인트 / 데이터 모델 / 안전장치 |
| `docs/architecture-refactor.md` | core 분리 + CLI 로컬 모드 등 구조 변경 이력 / 미래 계획 |
| `docs/storage-design.md` | 파일 진리원 + SQLite 캐시/저널 — 차기 저장소 설계 (구현 대기) |
| `docs/dev-plan.md` | 단계별 개발 계획 + 진행 상태 |
| `docs/planning.md` | 기획 결정 (용어, MVP 범위, 향후 기능) |
| `docs/guild-rules.md` | 개발 규칙 (커밋·브랜치·백/프론트 컨벤션) |

### Agent 가 openguild **를 사용** 할 때 (도구로 작업 관리)

| 문서 | 내용 |
|---|---|
| `docs/AGENTS_OPENGUILD_USAGE.md` | CLI (`openguild`) 사용법, 워크플로 패턴, 안전장치 |

## 빠른 명령어 참조

| 영역 | 실행 |
|---|---|
| 서버 host | `cargo run --bin openguild-server -- host` |
| 서버 관리 | `openguild-server {info, snapshot, restore, reindex, migrate-to-files, check-counters, check-drift}` |
| 프론트엔드 | `cd gui/frontend && npm run dev` (또는 `just dev-frontend`) |
| CLI | `cargo run --bin openguild -- --help` (또는 `target/release/openguild`) |
| 테스트 전체 | `cargo test --workspace && (cd gui/frontend && npm test -- --run)` |

위 문서에 있는 내용은 여기 중복하지 않는다.
