# openguild
A project issue tracker.

> **Status: Beta 1.0.0 (in progress)** — Active milestone tracked in
> campaign `C-001`. See [`docs/dev-plan.md`](./docs/dev-plan.md) for roadmap.

## Terminology
| Term | Description |
|---|---|
| **Guild** | A single project unit. Each guild has its own quests and settings. |
| **Quest** | An individual issue or task within a guild. |
| **Sub-Quest** | A child task that belongs to a parent quest. |
| **Prerequisite Quest** | A quest that must be completed before another quest can begin. |
| **Quest Due** | Optional deadline per quest — `desired_due` (informational) and `required_due` (drives Home "imminent" / "overdue" sections). |
| **Campaign** | A planning document that groups quests toward a release or milestone. Has its own GFM task-list checklist + many-to-many quest links. |
| **Quest Board** | A node-based board where quests are arranged by status in swim lanes. |
| **Quest List** | A list view of all quests, with sub-quests shown as a collapsible tree. |
| **Home** | Dashboard: active campaigns carousel, upcoming / overdue campaigns, imminent / overdue quests, recently added or updated quests. |
| **Settings** | Per-app preferences page (`⚙` icon, top-right). Currently: app info + manual update check. |
| **Guild Master** | The administrator of a guild. |
| **Quest Holder** | The person assigned to a quest. |
| **Requester** | The person who created the quest. |
| **Urgency** | Priority level of a quest: Critical / High / Medium / Low. |


## Usage

### Opening a Guild
Each guild is stored as an independent directory. A guild is identified by a `{name}.guild` file inside the directory.

```bash
# Initialize the current directory as a guild
openguild init [--name "My Project"]

# Local mode: CLI auto-detects .guild in cwd (or ancestors)
cd ./my-project
openguild quest list
openguild --guild ./other-project quest list

# Remote mode: HTTP to a hosted server
openguild --remote https://openguild.io/alice/monitor quest list
```

The GUI (Tauri desktop app) provides directory selection and a recent guilds list. Windows installer (`openguild_{version}_x64-setup.exe`, NSIS) offers per-component selection (GUI / CLI / Server) and an optional PATH registration; Linux builds are also attached to each GitHub Release as `.deb` / `.rpm` / AppImage packages, and macOS (Apple Silicon) as a `.dmg`. Installed app auto-checks for updates on startup and every 6 hours (notification only — install requires user click).

**macOS note.** Builds are for Apple Silicon (arm64) only and are **not code-signed** — the app is not registered with Apple, so the first launch is blocked by Gatekeeper ("cannot be opened because the developer cannot be verified"). Open it once via **right-click (or Control-click) → Open → Open**, or clear the quarantine attribute:

```bash
xattr -dr com.apple.quarantine /Applications/openguild.app
```

Only the GUI ships in the `.dmg`. For the `openguild` CLI and `openguild-server` on macOS, build from source (`cargo build --release -p openguild-cli -p openguild-server`).

### Creating a Quest
Quests are created within a guild. Each quest has a type prefix and an auto-incremented ID (e.g., `DEV-001`, `BUG-003`).

Default quest types:
- `DEV` — General development task
- `BUG` — Bug report
- `REQ` — Feature request

### Quest Board
Quests are displayed as nodes arranged in swim lanes by status. Drag a node to a different lane to change its status. Arrows between nodes indicate prerequisite or sub-quest relationships.

The board has a grid-snap toggle (G key) and an Arrange action with two modes (`Group` / `All`) — both per-lane and globally. Group mode separates connected components into rectangular regions.

### Quest List
All quests are shown in a flat list. Sub-quests appear as a collapsible tree under their parent. Status cannot be changed from this view — use the Quest Board or Quest Detail page.

### CLI (`openguild`)
A console client for agents and automation. Two modes:
- **Local (default)**: auto-detects `.guild` from cwd, calls `core` services directly. No server needed.
- **Remote**: `--remote URL` or env `OPENGUILD_REMOTE` for HTTP-hosted guilds.

```bash
openguild ping                                    # confirm backend / show mode
openguild quest list --json                       # list all quests as JSON
openguild quest new --type DEV --title "..." --json
openguild quest start DEV-001                     # transition to In Progress
openguild quest done DEV-001                      # transition to Done
openguild quest show DEV-001                      # detail view (sub/prereq included)
openguild quest due DEV-001 --required 2026-06-30 # set required deadline
openguild quest history DEV-001                   # status / type change audit

# Campaigns (release / milestone planning)
openguild campaign new --title "v1.0" --end 2026-06-30
openguild campaign link C-001 DEV-001             # attach quest
openguild campaign checklist add C-001 "Smoke test installer"
openguild campaign checklist check C-001 1
```

### Agent skill (Claude Code)
The installed app (and the source checkout) ships a `skills/` directory
structured as a Claude Code plugin marketplace, synced to
`~/.openguild/skill-marketplace/` on first run — this is the full CLI
reference for agents (command catalog, workflow patterns, safety guards). To
teach an agent how to use openguild in your own project, register it from
Claude Code:
```
/plugin marketplace add ~/.openguild/skill-marketplace
/plugin install openguild-plugin@openguild
```

## Documentation

| File | Audience | Purpose |
|---|---|---|
| [`AGENTS.md`](./AGENTS.md) | AI agent | Index — points to other docs |
| `skills/openguild-plugin/skills/openguild/` | AI agent | How an agent uses openguild as a task management tool (Claude Code skill — see [Agent skill](#agent-skill-claude-code) above) |
| [`docs/architecture.md`](./docs/architecture.md) | Developers | System architecture, API endpoints, data model |
| [`docs/storage-design.md`](./docs/storage-design.md) | Developers | File-as-truth + SQLite cache + AOF/RDB design |
| [`docs/dev-plan.md`](./docs/dev-plan.md) | Developers | Roadmap, progress |
| [`docs/planning.md`](./docs/planning.md) | Developers | Design decisions, terminology, MVP scope |
| [`docs/guild-rules.md`](./docs/guild-rules.md) | Developers | Coding / commit / branch conventions |


## Development
**Backend (HTTP server + admin CLI)**
```bash
cargo run --bin openguild-server -- host            # start HTTP server (host-only)

# Maintenance/diagnostics live in the `openguild` CLI (or HTTP admin /api/admin/*):
cargo run --bin openguild -- info                   # guild meta + cache + snapshot stats
cargo run --bin openguild -- backup new             # manual backup (RDB snapshot)
cargo run --bin openguild -- backup list            # list snapshots
cargo run --bin openguild -- restore [--to TS]      # restore from a snapshot
cargo run --bin openguild -- reindex                # rebuild .guild/index.db from files (= `index rebuild`)
cargo run --bin openguild -- migrate-to-files       # one-shot: legacy guild.db → .guild/quests/*.md
cargo run --bin openguild -- check counters [--fix]
cargo run --bin openguild -- check drift [--resync]
# or: just dev-server
```

**Frontend (Svelte)**
```bash
cd gui/frontend
npm run dev
# or from repo root: just dev-frontend
```

**CLI**
```bash
cargo build --release --bin openguild   # → target/release/openguild
# or: cargo run --bin openguild -- --help
```

**Desktop app bundle (macOS, Apple Silicon)**

Requires Xcode Command Line Tools (`xcode-select --install`) and Node 20+.

```bash
cargo install tauri-cli --version '^2' --locked
cd gui && cargo tauri build          # → target/release/bundle/{dmg,macos}
```

`gui/tauri.macos.conf.json` pins the bundle targets to `app` + `dmg`, so a plain
`cargo tauri build` on macOS produces the same artifacts CI does. The build is
unsigned; see the macOS note above for the first-launch step.

### Recovery — older binary refuses to open a guild

A binary built before some migration `N` was added refuses to open a guild DB
that already has migration `N` recorded (sqlx's `VersionMissing(N)` panic).
**openguild-gui v0.1.0-beta and later** tolerate this automatically
(`set_ignore_missing(true)`), so just **update to the latest release**.

If you must use an older binary anyway, manually delete the unknown-version row:

```bash
sqlite3 .guild/index.db "DELETE FROM _sqlx_migrations WHERE version = N;"
```

(Replace `N` with the version reported in the panic message.) The schema change
itself stays in the DB but is no longer tracked — safe because the older binary
doesn't use that table.

---

# openguild
프로젝트 이슈 트래커.

> **상태: Beta 1.0.0 (진행 중)** — 활성 마일스톤은 캠페인 `C-001` 으로 추적.
> 로드맵은 [`docs/dev-plan.md`](./docs/dev-plan.md).

## 용어 설명
| 용어 | 설명 |
|---|---|
| **Guild (길드)** | 프로젝트 단위. 각 길드는 독립된 퀘스트와 설정을 가진다. |
| **Quest (퀘스트)** | 길드 내 개별 이슈 또는 작업. |
| **Sub-Quest (서브퀘스트)** | 부모 퀘스트에 속하는 하위 작업. |
| **Prerequisite Quest (선행 퀘스트)** | 특정 퀘스트를 시작하기 전에 완료되어야 하는 퀘스트. |
| **Quest Due (퀘스트 기한)** | 퀘스트별 선택 마감 — `desired_due` (정보성), `required_due` (Home 의 "마감 임박" / "Overdue" 분류 기준). |
| **Campaign (캠페인)** | 릴리즈 또는 마일스톤을 향해 퀘스트를 묶는 기획 문서. 자체 GFM task-list 체크리스트 + 퀘스트 다대다 링크. |
| **Quest Board (의뢰게시판)** | 퀘스트를 상태별 레인으로 배치하는 노드 기반 보드. |
| **Quest List (퀘스트 목록)** | 모든 퀘스트를 리스트로 보여주는 뷰. 서브퀘스트는 접기/펼치기 트리로 표시. |
| **Home (홈)** | 대시보드: 진행 중 캠페인 carousel, 곧 시작 / 마감 지난 캠페인, 마감 임박 / 지난 퀘스트, 최근 추가·수정된 퀘스트. |
| **Settings (설정)** | 앱 설정 페이지 (`⚙` 아이콘, 우상단). 현재: 앱 정보 + 수동 업데이트 확인. |
| **Guild Master (길드마스터)** | 길드 관리자. |
| **Quest Holder (담당자)** | 퀘스트를 담당하는 사람. |
| **Requester (의뢰인)** | 퀘스트를 생성한 사람. |
| **Urgency (긴급도)** | 퀘스트의 우선순위: Critical / High / Medium / Low. |


## 사용 방법

### 길드 열기
각 길드는 독립된 디렉터리로 저장된다. 디렉터리 안의 `{이름}.guild` 파일로 길드를 식별한다.

```bash
# 현재 디렉토리를 길드로 초기화
openguild init [--name "내 프로젝트"]

# 로컬 모드: cwd 또는 그 상위에서 .guild 자동 탐색
cd ./my-project
openguild quest list
openguild --guild ./other-project quest list

# 원격 모드: 호스팅된 서버에 HTTP 로 접속
openguild --remote https://openguild.io/alice/monitor quest list
```

GUI (Tauri 데스크탑 앱) 는 디렉터리 선택 + 최근 길드 목록을 제공. Windows installer(`openguild_{version}_x64-setup.exe`, NSIS)는 컴포넌트 선택 (GUI / CLI / Server) + PATH 등록 옵션 제공 — 매 GitHub Release 에 첨부됨. 리눅스 빌드도 각 Release 에 `.deb` / `.rpm` / AppImage 패키지로, macOS(Apple Silicon)는 `.dmg` 로 첨부. 설치된 앱은 시작 시 + 6시간 간격으로 업데이트 자동 확인 (알림만; 설치는 사용자 클릭).

**macOS 안내.** Apple Silicon(arm64) 전용이며 **코드 서명을 하지 않는다** — Apple 에 등록된 앱이 아니라서 첫 실행이 Gatekeeper 에 막힌다("개발자를 확인할 수 없기 때문에 열 수 없습니다"). 처음 한 번만 **우클릭(또는 Control-클릭) → 열기 → 열기** 로 실행하거나, 격리 속성을 지운다:

```bash
xattr -dr com.apple.quarantine /Applications/openguild.app
```

`.dmg` 에는 GUI 만 들어 있다. macOS 에서 `openguild`(CLI)·`openguild-server` 가 필요하면 소스에서 빌드한다(`cargo build --release -p openguild-cli -p openguild-server`).

### 퀘스트 생성
퀘스트는 길드 내에서 생성된다. 각 퀘스트는 타입 prefix와 자동 증가 ID를 가진다 (예: `DEV-001`, `BUG-003`).

기본 퀘스트 타입:
- `DEV` — 일반 개발 작업
- `BUG` — 버그 보고
- `REQ` — 기능 요청

### Quest Board
퀘스트가 노드로 표시되며, 상태별 레인으로 배치된다. 노드를 다른 레인으로 드래그하면 상태가 변경된다. 노드 간 화살표는 선행 퀘스트 또는 서브퀘스트 관계를 나타낸다.

보드에는 그리드 스냅 토글(G 키)과 정렬 액션(`Group` / `All` 모드)이 있다. Group 모드는 연관된 노드 그룹을 직사각형 영역으로 분리해서 정렬한다. 정렬은 보드 전체 단위와 레인 단위 둘 다 가능.

### Quest List
모든 퀘스트를 단일 리스트로 표시한다. 서브퀘스트는 부모 퀘스트 하위에 접기/펼치기 트리로 나타난다. 이 뷰에서는 상태를 변경할 수 없으며, Quest Board 또는 Quest Detail 페이지에서 변경한다.

### CLI (`openguild`)
agent / 자동화용 콘솔 클라이언트. 두 모드:
- **로컬 (기본)**: cwd `.guild` 자동 탐색 → core 직접 호출. 서버 불필요.
- **원격**: `--remote URL` 또는 env `OPENGUILD_REMOTE` 로 호스팅 서버 HTTP 호출.

```bash
openguild ping                                    # 서버 상태 확인
openguild quest list --json                       # 전체 퀘스트 JSON 출력
openguild quest new --type DEV --title "..." --json
openguild quest start DEV-001                     # In Progress 로
openguild quest done DEV-001                      # Done 으로
openguild quest show DEV-001                      # 상세 (서브/선행 포함)
openguild quest due DEV-001 --required 2026-06-30 # 필수 기한 설정
openguild quest history DEV-001                   # status / type 변경 audit

# 캠페인 (릴리즈 / 마일스톤 기획)
openguild campaign new --title "v1.0" --end 2026-06-30
openguild campaign link C-001 DEV-001             # quest 링크
openguild campaign checklist add C-001 "설치본 smoke test"
openguild campaign checklist check C-001 1
```

### 에이전트 스킬 (Claude Code)
설치된 앱(및 소스 체크아웃)엔 Claude Code plugin marketplace 구조로 만들어진
`skills/` 디렉토리가 포함돼 있고, 첫 실행 시 `~/.openguild/skill-marketplace/`
로 동기화된다 — 이게 에이전트용 전체 CLI 가이드(명령어 카탈로그, 워크플로
패턴, 안전장치)다. 여러분의 프로젝트에서 openguild 사용법을 에이전트에게
가르치려면 Claude Code 에서 다음을 실행해 등록한다:
```
/plugin marketplace add ~/.openguild/skill-marketplace
/plugin install openguild-plugin@openguild
```

## 문서

| 파일 | 대상 | 내용 |
|---|---|---|
| [`AGENTS.md`](./AGENTS.md) | AI agent | 인덱스 — 다른 문서로 가는 진입점 |
| `skills/openguild-plugin/skills/openguild/` | AI agent | agent 가 openguild 를 작업 관리 도구로 사용하는 방법 (Claude Code 스킬 — 위 [에이전트 스킬](#에이전트-스킬-claude-code) 참고) |
| [`docs/architecture.md`](./docs/architecture.md) | 개발자 | 시스템 구조, API 엔드포인트, 데이터 모델 |
| [`docs/storage-design.md`](./docs/storage-design.md) | 개발자 | 파일 진리원 + SQLite 캐시 + AOF/RDB 설계 |
| [`docs/dev-plan.md`](./docs/dev-plan.md) | 개발자 | 단계별 개발 계획 + 진행 상태 |
| [`docs/planning.md`](./docs/planning.md) | 개발자 | 기획 결정, 용어, MVP 범위 |
| [`docs/guild-rules.md`](./docs/guild-rules.md) | 개발자 | 커밋·브랜치·코드 컨벤션 |


## 개발 환경 실행
모든 명령은 repo root 에서 실행. `justfile` 의 단축 명령 사용 가능 (`just --list`).

**백엔드 (HTTP 서버 + 관리 CLI)**
```bash
cargo run --bin openguild-server -- host            # 서버 시작 (host 전용)

# 정비/진단은 `openguild` CLI (또는 HTTP admin /api/admin/*):
cargo run --bin openguild -- info                   # 길드 / 캐시 / snapshot 현황
cargo run --bin openguild -- backup new             # 즉시 백업 (RDB snapshot)
cargo run --bin openguild -- backup list            # 백업 목록
cargo run --bin openguild -- restore [--to TS]      # snapshot 으로 복원
cargo run --bin openguild -- reindex                # 파일 → index.db 재구축 (= `index rebuild`)
cargo run --bin openguild -- migrate-to-files       # 1회: legacy guild.db → .guild/quests/*.md
cargo run --bin openguild -- check counters [--fix]
cargo run --bin openguild -- check drift [--resync]
# or: just dev-server
```

**프론트엔드 (Svelte)**
```bash
cd gui/frontend && npm run dev
# or: just dev-frontend
```

**CLI**
```bash
cargo build --release --bin openguild   # → target/release/openguild
# or: cargo run --bin openguild -- --help
```

**데스크탑 앱 번들 (macOS, Apple Silicon)**

Xcode Command Line Tools (`xcode-select --install`) 와 Node 20+ 필요.

```bash
cargo install tauri-cli --version '^2' --locked
cd gui && cargo tauri build          # → target/release/bundle/{dmg,macos}
```

`gui/tauri.macos.conf.json` 이 번들 타깃을 `app` + `dmg` 로 고정하므로 맥에서
그냥 `cargo tauri build` 만 해도 CI 와 같은 산출물이 나온다. 서명은 하지 않으니
첫 실행 절차는 위 macOS 안내 참고.

### 복구 — 이전 binary 가 길드를 못 열 때

이전 release 의 binary 가 (그 시점에 없던) migration `N` 이 적용된 길드 DB 를
열 때 sqlx 가 `VersionMissing(N)` panic 으로 거부. **v0.1.0-beta 이후 빌드**
는 자동으로 통과 (`set_ignore_missing(true)`) — 그냥 **최신 release 로 업데이트**
권장.

부득이하게 옛 binary 로 열어야 한다면 unknown version row 를 수동 삭제:

```bash
sqlite3 .guild/index.db "DELETE FROM _sqlx_migrations WHERE version = N;"
```

(`N` 은 panic 메시지의 version 으로 교체.) Schema 변경 자체는 DB 에 남되 추적이
끊기는 것 — 옛 binary 는 그 테이블을 사용하지 않으니 안전.

## License

MIT License © 2026 Jirung-E. 자세한 내용은 [LICENSE](LICENSE) 참조.
