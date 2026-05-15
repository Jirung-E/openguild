# OpenGuild 소프트웨어 아키텍처

## 전체 구조

```mermaid
graph TB
    subgraph Clients["클라이언트"]
        WEB[웹 브라우저 - Svelte]
        CLI["CLI 'openguild' - agent / 자동화 (로컬·원격)"]
    end

    subgraph Frontend["gui/frontend (Svelte 5 + Vite)"]
        FE_COMP[Components - Board / List / Detail / Combobox]
        FE_API["api/ - client.ts (VITE_API_URL or relative)"]
    end

    subgraph Server["server crate (Axum HTTP)"]
        SRV_CLI["CLI: host / backup / info"]
        SRV_ROUTES["routes/ - 얇은 HTTP 어댑터"]
        SRV_MW["middleware: CORS · tracing · audit"]
        SRV_ERR["HttpError - IntoResponse wrapper"]
    end

    subgraph CliCrate["cli crate (openguild bin)"]
        CLI_BACKEND["Backend enum - Http | Local"]
        CLI_HTTP["HttpClient (reqwest blocking)"]
        CLI_LOCAL["LocalBackend (tokio rt + SqlitePool)"]
    end

    subgraph Core["core crate (lib) - 단일 진리원"]
        CORE_SVC["services/ - quests · meta"]
        CORE_DB["db.rs - sqlx pool + migrations"]
        CORE_GUILD["guild_file.rs - .guild TOML + find_from_cwd"]
        CORE_BACKUP["backup.rs - VACUUM INTO + prune"]
        CORE_ERR["AppError (plain Rust)"]
        CORE_MODELS["models/ - QuestRow / QuestDetail / requests"]
    end

    subgraph Storage["스토리지"]
        DB[(SQLite - guild.db)]
        GF["{name}.guild - TOML 마커"]
        BAK["backups/ - 시간별 스냅샷"]
        AUDIT["audit.log - mutation HTTP 기록"]
    end

    WEB --> FE_COMP --> FE_API
    FE_API -->|HTTP| SRV_MW
    CLI --> CLI_BACKEND
    CLI_BACKEND -->|--remote| CLI_HTTP -->|HTTP| SRV_MW
    CLI_BACKEND -->|.guild auto-detect| CLI_LOCAL
    SRV_MW --> SRV_ROUTES --> CORE_SVC
    CLI_LOCAL --> CORE_SVC
    CORE_SVC --> CORE_MODELS
    CORE_SVC --> CORE_DB --> DB
    CORE_GUILD --> GF
    CORE_BACKUP --> BAK
    SRV_MW -.->|mutation| AUDIT
```

**핵심 원칙**: `core` 가 모든 SQL · 검증 · 사이클 체크의 단일 진리원. server route 와 cli local 모드가 동일 함수 호출. 자세한 이전 경위는 [`architecture-refactor.md`](./architecture-refactor.md).

## Cargo Workspace 구조 (현재)

```
openguild/
├── Cargo.toml                    ← workspace, members = ["core", "cli", "server"]
├── core/                         ← lib: 도메인 로직 단일 진리원
│   ├── migrations/               ← sqlx 마이그레이션 (DB 스키마와 짝)
│   │   ├── 0001_initial.sql
│   │   ├── 0002_parent_on_delete_set_null.sql
│   │   └── 0003_soft_delete.sql
│   ├── seed.sql                  ← 개발용 시드
│   └── src/
│       ├── lib.rs
│       ├── db.rs                 ← sqlx pool, 마이그레이션 실행
│       ├── error.rs              ← AppError (plain Rust)
│       ├── guild_file.rs         ← {name}.guild TOML 파싱
│       ├── backup.rs             ← VACUUM INTO 자동 백업
│       └── models/
│           ├── meta.rs
│           └── quest.rs          ← QuestRow, QuestDetail, 요청 타입
├── cli/                          ← bin `openguild` (clap + reqwest)
│   ├── Cargo.toml                ← path = "../core"
│   └── src/main.rs
├── server/                       ← bin Axum HTTP API
│   ├── Cargo.toml                ← path = "../core"
│   ├── build.rs                  ← test.guild 자동 생성
│   └── src/
│       ├── main.rs
│       ├── audit.rs              ← mutation HTTP 요청 audit log
│       ├── error.rs              ← HttpError(AppError) — IntoResponse newtype
│       ├── tests.rs              ← 통합 테스트
│       └── routes/
│           ├── mod.rs
│           ├── meta.rs
│           └── quests.rs
├── gui/                          ← Tauri desktop (Phase 4 예정)
│   └── frontend/                 ← Svelte 5 + Vite
├── docs/
├── justfile                      ← dev/build/test 단축 명령
└── README.md
```

> **변경 이력**: 2026-05-14 — server 단일 crate → core/cli/server 분리, `backend/` 폴더 제거, frontend → gui/frontend/ 이동.
> 자세한 설계 근거는 `docs/architecture-refactor.md`.

## API 엔드포인트 (현재 구현)

| Method | Path | 설명 |
|---|---|---|
| GET    | `/health` | 서버 상태 |
| GET    | `/api/quest-types` | Quest 타입 목록 |
| GET    | `/api/quest-statuses` | Quest 상태 목록 |
| GET    | `/api/quests` | Quest 목록 (생성 역순) |
| POST   | `/api/quests` | Quest 생성 |
| GET    | `/api/quests/:id` | Quest 상세 (sub_quests, prerequisites, position 포함) |
| PATCH  | `/api/quests/:id` | Quest 수정 (title / description / urgency) |
| DELETE | `/api/quests/:id?cascade=ID,ID` | Quest 삭제 (선택적 cascade — 직계 자식 같이 삭제, 나머지는 분리) |
| GET    | `/api/quests/by/:slug` | slug 로 상세 조회 (예: `DEV-001`) |
| PATCH  | `/api/quests/:id/status` | 상태 변경 |
| PATCH  | `/api/quests/:id/parent` | 부모 변경 (`null` 로 분리) |
| GET    | `/api/quests/:id/candidates?relation=parent\|sub\|prereq` | 관계 추가 후보 (사이클 / 자기 / 이미 부모 보유 / 상호배제 / 직계부모 자동 제외) |
| POST   | `/api/quests/:id/prerequisites` | 선행 퀘스트 추가 (사이클·sub·parent 검증) |
| DELETE | `/api/quests/:id/prerequisites/:prereq_id` | 선행 퀘스트 제거 |
| PUT    | `/api/quests/:id/position` | Quest Board 노드 위치 저장 |
| GET    | `/api/quest-positions` | 모든 노드 위치 (alive quest 만) |
| GET    | `/api/quest-dependencies` | 모든 선행 관계 (양 끝 alive 만) |
| GET    | `/api/deleted-quests` | soft deleted 퀘스트 목록 |
| PATCH  | `/api/quests/:id/restore` | soft delete 취소 (alive 복원) |

## 데이터 모델 (현재 스키마)

| 테이블 | 핵심 컬럼 |
|---|---|
| `quests` | id, quest_type_id, number, title, description, status_id, urgency, parent_quest_id (FK ON DELETE SET NULL), created_at, updated_at, **deleted_at** (NULL = alive) |
| `quest_types` | id, prefix, color, description |
| `quest_statuses` | id, name_en, name_ko, color, sort_order |
| `quest_counters` | quest_type_id (PK), last_number — 타입별 자동 증가 카운터 |
| `quest_dependencies` | quest_id (FK CASCADE), prerequisite_id (FK CASCADE), PK(quest_id, prerequisite_id) |
| `quest_positions` | quest_id (PK FK CASCADE), x, y |

핵심 무결성:
- 사이클 방지: 부모 변경 / 선행 추가 시 백엔드에서 BFS 검증
- sub ↔ prereq 상호 배제: 같은 두 퀘스트가 동시에 sub + prereq 일 수 없음
- 직계 부모는 prereq 후보에서도 제외
- Soft delete: `DELETE` 요청은 `deleted_at = now()` 만 set. 모든 SELECT 가 `WHERE deleted_at IS NULL` 필터. 복구는 `PATCH /:id/restore`

## 안전장치 (agent / 자동화 대응)

| 장치 | 위치 | 효과 |
|---|---|---|
| **자동 백업** | `core/src/backup.rs` | startup + 1h 주기로 `VACUUM INTO`, `<guild>/backups/` 에 7일 보관 |
| **Audit log** | `server/src/audit.rs` | 모든 POST/PATCH/PUT/DELETE 호출을 `<guild>/audit.log` 에 timestamped tab-separated 로 기록 |
| **Soft delete** | migration 0003 `deleted_at` | 실 삭제 X, 복원 가능. 영구 삭제는 별도 (미구현) |
| **CLI `--yes` 강제** | `cli/` | `openguild quest delete` 는 `--yes` 없으면 거부 |
| **CLI `--dry-run`** | `cli/` | `delete` / `update` 의 영향 미리보기, 실제 호출 X |

## 클라이언트 비교

| | Frontend (Svelte) | CLI (`openguild`) |
|---|---|---|
| 형태 | 웹 GUI | 콘솔 stdin/stdout |
| 통신 | `fetch` (HTTP) | Backend enum: Local 모드는 `core::services::*` 직접 호출, Remote 모드는 `reqwest` blocking |
| 모델 | TypeScript types (`gui/frontend/src/lib/types/index.ts`) | `openguild_core::models::*` 재사용 |
| 서버 의존 | 필수 | 로컬 모드 = 없음 / 원격 모드 = 동일 백엔드 |
| 주 사용자 | 사람 | AI agent / 스크립트 |

Backend 추상화는 `cli/src/main.rs` 의 `Backend` enum. 로컬은 `--guild` 또는 cwd `.guild` 자동탐색
(`core::guild_file::find_from_cwd`), 원격은 `--remote URL` 또는 env `OPENGUILD_REMOTE`.

## 향후 계획 (미구현)

- 멀티유저 인증 (JWT)
- Campaign / Comment / Memo / Quest History
- 길드 다중 동시 접속 (현재 SQLite 단일 파일 가정)
- AWS EC2 배포 + GitHub Actions CI/CD
