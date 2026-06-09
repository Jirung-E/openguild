# openguild 소프트웨어 아키텍처

## 전체 구조

```mermaid
graph TB
    subgraph Clients["클라이언트"]
        WEB[웹 브라우저 - Svelte]
        CLI["CLI 'openguild' - agent / 자동화 (로컬·원격)"]
    end

    subgraph Frontend["gui/frontend (Svelte 5 + Vite)"]
        FE_COMP[Components - Board / List / Detail / Combobox / Admin]
        FE_API["api/ - quests · meta · admin (client.ts)"]
    end

    subgraph Server["server crate (Axum HTTP)"]
        SRV_CLI["CLI: host / info / snapshot / restore / reindex /<br/>migrate-to-files / check-counters / check-drift"]
        SRV_ROUTES["routes/ - /api/quests/* /api/admin/*"]
        SRV_ERR["HttpError - IntoResponse wrapper"]
    end

    subgraph CliCrate["cli crate (openguild bin)"]
        CLI_BACKEND["Backend enum - Http | Local"]
        CLI_HTTP["HttpClient (reqwest blocking)"]
        CLI_LOCAL["LocalBackend (Store)"]
    end

    subgraph Core["core crate (lib) - 도메인 + 저장소 추상화"]
        CORE_OPS["ops/ - mutation orchestration<br/>(검증 → journal → 파일 → index)"]
        CORE_SVC["services/ - SQL queries (read + 검증)"]
        CORE_REPO["repo/ - 파일 read/write + auto block 렌더링"]
        CORE_STORE["store - Store { index_pool, journal_pool, paths }"]
        CORE_REINDEX["reindex / drift / snapshot / lock / counter"]
        CORE_MIGRATE["migrate - legacy guild.db → 파일"]
        CORE_MODELS["models/ - QuestRow / QuestDetail / requests"]
        CORE_GUILD["guild_file.rs - {name}.guild TOML"]
    end

    subgraph DotGuild[".guild/ (per project)"]
        QF["{name}.guild - TOML 마커 (git tracked)"]
        QM[".guild/quests/{slug}.md - 진리원 (git tracked)"]
        QC[".guild/quests/{slug}.comments.md - 댓글 (git tracked, DEV-094)"]
        QMM[".guild/quests/{slug}.memo.md - 메모 (gitignored, DEV-099)"]
        TM[".guild/types/*.toml - 진리원 (git tracked)"]
        SM[".guild/statuses/*.toml - 진리원 (git tracked)"]
        RL[".guild/rules/*.md - 길드 규칙 (git tracked)"]
        IDX[".guild/index.db - 쿼리 캐시 (gitignored)"]
        JRN[".guild/backups/journal.db - AOF (gitignored)"]
        SNP[".guild/backups/snapshots/*.db - RDB 시점 백업 (gitignored)"]
        LCK[".guild/.lock - single-writer (gitignored)"]
    end

    WEB --> FE_COMP --> FE_API
    FE_API -->|HTTP| SRV_ROUTES
    CLI --> CLI_BACKEND
    CLI_BACKEND -->|--remote| CLI_HTTP -->|HTTP| SRV_ROUTES
    CLI_BACKEND -->|.guild auto-detect| CLI_LOCAL
    SRV_ROUTES -->|mutation| CORE_OPS
    SRV_ROUTES -->|read| CORE_SVC
    CLI_LOCAL -->|mutation| CORE_OPS
    CLI_LOCAL -->|read| CORE_SVC
    CORE_OPS --> CORE_REPO
    CORE_OPS --> CORE_STORE
    CORE_SVC --> CORE_STORE
    CORE_STORE --> IDX
    CORE_STORE --> JRN
    CORE_REPO --> QM
    CORE_REPO --> TM
    CORE_REPO --> SM
    CORE_REINDEX --> CORE_REPO
    CORE_REINDEX --> SNP
```

**핵심 원칙**:
- `.guild/quests/*.md` 등 **파일이 진리원**. SQLite (`.guild/index.db`) 는 쿼리 캐시 — 손실 시 `reindex` 로 복구.
- **DB 두 종류 분리**: `index.db` 는 쿼리 캐시 (gitignored, 재구축 가능). `backups/journal.db` + `snapshots/*.db` 가 **정식 백업** — git 없어도 시점 복원 보장.
- 모든 mutation 은 `core::ops::*` 통과: journal append → SQL → 파일 atomic write → auto block 갱신.
- 자동 백업: `core::snapshot::maybe_auto_snapshot` 이 ops 끝에서 정책 검사 (ops 50 / 24h).
- git 은 **사용자 선택사항** — git 없이도 snapshot/journal 로 안전 보장.
- drift 자동 복구: `Store::open` 직후 `drift::auto_resync` 가 외부 편집 감지 시 reindex (server / cli / gui 동일, BUG-049).

자세한 이전 경위 / 설계 근거: [`architecture-refactor.md`](./architecture-refactor.md), [`storage-design.md`](./storage-design.md).

## Cargo Workspace 구조 (현재)

```
openguild/
├── Cargo.toml                       ← workspace, members = ["core", "cli", "server", "gui"]
├── openguild.guild                  ← 본 repo 의 마커 (dogfood)
├── .guild/                          ← 본 repo 의 quests / campaigns / 캐시 (dogfood)
├── core/                            ← lib: 도메인 + 저장소 추상화
│   ├── migrations/                  ← sqlx 마이그레이션 (0001~0015, set_ignore_missing 로 backward compat)
│   └── src/
│       ├── lib.rs
│       ├── db.rs                    ← sqlx pool 생성
│       ├── error.rs                 ← AppError (plain Rust)
│       ├── guild_file.rs            ← {name}.guild TOML + find_from_cwd
│       ├── store.rs                 ← Store { index_pool, journal_pool, paths }
│       ├── models/                  ← QuestRow / QuestDetail / Campaign / 요청 타입
│       ├── repo/                    ← 파일 IO 진리원 ⭐
│       │   ├── quest.rs             ← .md frontmatter (TOML) + body + auto block
│       │   ├── campaign.rs          ← DEV-011: .md frontmatter + body GFM 체크리스트
│       │   ├── comments.rs          ← DEV-094/099: {slug}.comments.md (HTML 마커) + {slug}.memo.md
│       │   ├── rules.rs             ← 길드 규칙 {slug}.md (rules/)
│       │   ├── type_def.rs          ← {prefix}.toml + [counter]
│       │   ├── status_def.rs        ← {n}-{slug}.toml
│       │   ├── auto.rs              ← Parent / Sub-quests / Prerequisites 렌더
│       │   ├── seed.rs              ← 기본 시드 + seed_guild_dir
│       │   └── fs.rs                ← atomic write + list_quest_body_files (BUG-047 sibling 제외)
│       ├── services/                ← SQL queries (read 위주)
│       │   ├── quests.rs
│       │   ├── campaigns.rs         ← DEV-011: 캠페인 CRUD / 체크리스트 / 링크
│       │   └── meta.rs
│       ├── ops/                     ← mutation orchestration ⭐
│       │   ├── quests.rs            ← journal + SQL + file + auto block
│       │   ├── campaigns.rs         ← DEV-011: 캠페인 mutation orchestration
│       │   ├── meta.rs
│       │   └── counter.rs
│       ├── snapshot.rs              ← RDB + AutoSnapshotPolicy
│       ├── reindex.rs               ← 파일 → index.db 재구축
│       ├── drift.rs                 ← 외부 편집 감지 + auto_resync
│       ├── counter.rs               ← type counter 무결성 검증
│       ├── lock.rs                  ← .guild/.lock single-writer
│       ├── recents.rs               ← DEV-006: GUI 최근 길드 목록 (per-OS data dir)
│       └── migrate.rs               ← legacy guild.db → 파일
├── cli/                             ← bin `openguild`
│   └── src/main.rs                  ← Backend { Http | Local } — campaign / quest due 포함
├── server/                          ← bin `openguild-server`
│   └── src/
│       ├── main.rs                  ← host / info / snapshot / restore /
│       │                                reindex / migrate-to-files /
│       │                                check-counters / check-drift
│       ├── error.rs                 ← HttpError newtype
│       ├── tests.rs                 ← 통합 테스트
│       └── routes/
│           ├── mod.rs
│           ├── meta.rs              ← /api/quest-types · /api/quest-statuses
│           ├── quests.rs            ← /api/quests/* (CRUD / 관계 / status / type / due)
│           ├── campaigns.rs         ← DEV-011: /api/campaigns/* (CRUD / 체크리스트 / 링크)
│           └── admin.rs             ← /api/admin/{snapshot,restore,drift,reindex}
├── gui/                             ← bin `openguild-gui` — Tauri 2 desktop app
│   ├── Cargo.toml                   ← tauri / tauri-plugin-{dialog,updater,process} + core
│   ├── build.rs                     ← tauri-build (icon / capabilities ACL 생성)
│   ├── tauri.conf.json              ← productName / bundle (NSIS) / plugins.updater
│   ├── capabilities/
│   │   └── default.json             ← core / dialog / updater / process 권한
│   ├── icons/                       ← 아이콘 세트 (32 / 128 / @2x / ico / icns / ios / android)
│   ├── nsis/installer.nsi           ← DEV-034: 멀티 컴포넌트 (GUI / CLI / Server / PATH)
│   ├── gen/                         ← gitignored — tauri-build 자동 생성
│   ├── src/
│   │   ├── main.rs / lib.rs         ← winit + invoke handler 등록
│   │   └── commands.rs              ← Tauri commands (core::ops 호출)
│   └── frontend/                    ← Svelte 5 + Vite (web + Tauri 양쪽 동작)
│       └── src/
│           ├── routes/
│           │   ├── +page.svelte         ← Home / Board / List (?view)
│           │   ├── quests/[id]          ← Quest Detail
│           │   ├── campaigns/           ← Campaign 목록 / 신규 / 상세
│           │   ├── settings/            ← DEV-084: 정보 / 업데이트 등
│           │   ├── welcome/             ← DEV-052: 길드 선택 화면
│           │   └── admin/               ← 백업 / drift / reindex UI
│           └── lib/
│               ├── api/                 ← client.ts + transport (HTTP / Tauri invoke)
│               ├── components/          ← Home / QuestBoard / QuestList / Campaign* /
│               │                          QuestNodeConveyor / UpdateBanner / ...
│               └── utils/               ← datetime / quest-node-svg / quest-list /
│                                          campaign-sort
├── scripts/
│   └── seed-test-data.ps1           ← DEV-075: 테스트 데이터 자동 주입
├── docs/
├── justfile
└── README.md
```

> **변경 이력**:
> - 2026-05-14 server 단일 crate → core/cli/server 분리, `backend/` 폴더 제거.
> - 2026-05-16/17 저장소 모델 전환 — SQLite 진리원 → 파일 진리원 + SQLite 캐시.
>   `core::repo` (파일) + `core::ops` (orchestration) + `core::store` (Store) 신설.
> - 2026-05-21+ `gui/` crate 추가 — Tauri 2 desktop app (DEV-001~006).
> - 2026-05-25 DEV-011 Campaign entity 추가 — 파일 형식 B3 (frontmatter + GFM
>   task list 본문) + 4 테이블 (campaigns / campaign_checklists / campaign_quests
>   / campaign_counters) + CLI / server / GUI 전체.
> - 2026-05-26 DEV-034 멀티 컴포넌트 NSIS installer (GUI / CLI / Server / PATH).
> - 2026-05-27 DEV-076 Quest desired_due / required_due 필드 + Home 임박 / Overdue
>   섹션.
> - 2026-05-28 DEV-063 Tauri 자동 업데이트 (updater + process plugin + 서명
>   파이프라인) + DEV-084 설정 페이지 (`/settings`).
> - 2026-06-04 BUG-047 drift / reindex 가 sibling `.comments.md` / `.memo.md` 를
>   quest 본문으로 오인 — `repo::fs::list_quest_body_files` 로 좁힘.
> - 2026-06-04 BUG-048 recents 테스트 env race — `static OnceLock<Mutex<()>>`
>   직렬화 (사용자 머신 recents 오염 위험 해결).
> - 2026-06-04 BUG-049 GUI 시동 시 자동 reindex — `drift::auto_resync` 가
>   server / cli 와 동일하게 호출됨.
> - 2026-06-04 DEV-094 (댓글) / DEV-099 (메모) — file 진리원 + DEV-102 로 DB 캐시
>   (`quest_comments` / `quest_memos`) + snapshot 백업 합류 (migration 0011).
>   메모의 `user_id=0` sentinel — multi-user (DEV-021) 진입 시 격리 활성.
>   drift::detect_drift 가 sibling 파일도 fresh 감지 (auto reindex 트리거).
> - 2026-06-05 DEV-098 installer NSIS resources 에 README + USAGE 동봉
>   (사용자 친화 시작 가이드).
> - 2026-06-05 DEV-018 `openguild-server info` 에 `--brief` / `--detailed` 모드.
> - 2026-06-05 DEV-070 Quest Detail "Successors" 섹션 — quest_dependencies 역방향.
> - 2026-06-05 BUG-046 Campaign 체크리스트 클릭 시 페이지 최상단 점프 — optimistic
>   update 로 fix.
> - 2026-06-05 DEV-101 UI 크기 슬라이더 + localStorage (rem scale).
> - 2026-06-05 DEV-068 태그 풀스택 — migration 0010 + ops::set_quest_tags +
>   CLI (`quest tag add/rm/list/set`) + HTTP `/api/quests/:id/tags` + Tauri command
>   + Quest Detail tag pill UI + Quest List tag chip 필터 (AND).
> - 2026-06-06 DEV-074 다크 / 라이트 / 시스템 테마 — CSS variable token + store +
>   `<html data-theme>` + 25+ component 의 hardcoded color → var() 마이그레이션.
> - 2026-06-06 DEV-093 캠페인 quest 진행도 — migration 0012 `quest_statuses.
>   counts_as_done` + Home active 카드 progress 2 줄 + Campaign Detail 진행도
>   + Admin Statuses 의 토글 UI.
> - 2026-06-06 DEV-073 Quest Board toolbar 접기 토글 — lane 라벨 가림 해소.
> - 2026-06-06 DEV-065 QuestList Tree / List 뷰 모드 토글 (URL + localStorage).
> - 2026-06-06 DEV-077 arrangeNodesGrouped — cluster 의 y 좌표 lane 별 분리
>   (lane 안 겹침 없으면 같은 row 공유).
> - 2026-06-06 DEV-023 server CLI `vacuum` + `journal-tail` 추가.
> - 2026-06-06 BUG-054 QuestBoard.sorted → $state — long-standing npm check
>   warning 제거 (0 warnings).
> - 2026-06-07 DEV-068 `.guild/tags/{slug}.toml` 색·설명 — migration 0013
>   `quest_tag_defs` + repo TagFile + Admin UI + Quest Detail tag pill 색.
> - 2026-06-07 DEV-068 fix2: Tree 모드 + tag/type/status 필터 — child 매치 시
>   `includeAncestors` 로 부모 트리 보존 (이전엔 검색만 처리).
> - 2026-06-07 DEV-093 fix2: 캠페인 완료 판정 — 체크리스트 + 연결 quest 양쪽
>   100% 일 때만 완료.
> - 2026-06-07 DEV-074 fix2~10 라이트모드 마무리: 전역 커스텀 스크롤바
>   (`scrollbar-gutter: stable`) + `<main>` bg → `var(--bg)` + Cytoscape style
>   theme별 hex (`var()` 컴퓨팅 안 됨) + CodeMirror oneDark 조건부 +
>   `--btn-primary-* / --btn-warning-* / --card-hl-* / --scrollbar-*` 토큰
>   도입 + primary 버튼 11곳 sweep + 보드 설정 모달 체크박스 custom.
> - 2026-06-07 DEV-101 fix2~5 슬라이더 리빌드: `CustomSlider` (델타 드래그 +
>   click-jump + 즉시 적용 + 직접 숫자 입력) + `contentWidth` store +
>   `--content-max-width` 토큰 (Home/Campaigns/Rules/Quest/Admin/Settings) +
>   Nav 높이 `52px → 3.25rem` (UI scale 반영).
> - 2026-06-07 DEV-105 fix2~7 lane 헤더 정리: 보드설정 모달에 lane reorder
>   통합 + lane 헤더 zoom 무관 (UI overlay) + collapsed 영속 복원 + 긴 이름
>   위 정렬 (overflow 아래로) + 레인별 설정 `⚙` 토글 + 펼침 시
>   hideGroup/hideSolo 회귀 fix.
> - 2026-06-07 DEV-113 등록: `[gui] 원격 서버 모드` (DEV-088 하위).
> - 2026-06-07 DEV-114 등록: `[gui] 커스텀 테마` (사용자 토큰 색 자유 정의).
> - 2026-06-07 DEV-052 fix: welcome 페이지 우상단 ⚙ 설정 링크 (Nav 숨김 상태).
> - 2026-06-07 DEV-101 fix6~8: 설정 페이지 탭 분리 / 슬라이더 step 세분화 + 직접
>   숫자 입력 / Nav height rem 화 / lane 헤더 내부 폰트·padding rem.
> - 2026-06-07 DEV-074 fix11~17: 전역 체크박스 custom (`appearance:none`) /
>   `<main>` 배경 / Welcome `--content-max-width` / window 진짜 overlay
>   scrollbar (`OverlayScrollbar` 컴포넌트, html scrollbar 숨김 + transform 기반
>   GPU composite) / QuestList / CodeMirror / Combobox / UpdateBanner / Settings
>   toast / Quest 삭제 모달 list 에 overlay scrollbar.
> - 2026-06-07 DEV-073 fix2~3: Quest Board 도구바 — New Quest 상단 고정 + 나머지
>   는 그 아래 (가로) + 접을 수 있음.
> - 2026-06-07 DEV-093 fix2-test: 캠페인 완료 판정 로직 `lib/utils/campaign-progress`
>   로 추출 + vitest 회귀 10건.
> - 2026-06-07 DEV-105 fix8~14: 보드 진입 시 collapsed lane 노드 hide / 가변 폭
>   visual lane idx (collapsed 영역 클릭/드롭 회귀 fix) / drag 중 lane 강조 /
>   grid snap SVG zoom·cols 캐시 / 세로 pan 추적 transform 기반 / wrappedBgY
>   modulo cellH 로 dot 위치 정확도.
> - 2026-06-07 DEV-026: cytoscape 동적 import — board route node chunk 646KB → 45KB.
> - 2026-06-07 DEV-111: markdown 안 mermaid 다이어그램 렌더 (lazy import) — theme
>   별 dark/default 분기.
> - 2026-06-07 DEV-112: Quest Board 노드 배경 투명도 — `background-opacity: 0.92`
>   + `background-image-opacity: 0.88` (border 는 opaque).
> - 2026-06-07 DEV-115 등록 + 구현: Quest Board 의 최근 움직인 노드를 위로
>   (z-index 단조 증가, drag / undo / redo 모두).
> - 2026-06-07 DEV-109: Quest Detail 본문이 길 때 우하단 floating `↓ 댓글` 점프
>   버튼 (anchor 의 viewport 위치 추적).
> - 2026-06-07 DEV-107 fix1: 댓글 / 메모 섹션 접기 — 사용자 피드백 반영 영속
>   localStorage 제거 + 답글 단위 접기 (root 별 `collapsedRoots: Set<number>`).
> - 2026-06-07 BUG-020 fix2: arrangeNodesGrouped 의 cluster 식별을 lane-local BFS
>   가 아닌 GLOBAL `groupOf` (cross-lane 포함 전체 의존 그래프) 기반으로 변경.
>   같은 외부 그룹의 lane 멤버가 같은 cluster 직사각형 공유.
> - 2026-06-08 DEV-111 fix1: mermaid syntax error 시 body 끝 leftover bomb SVG
>   제거 — `mermaid.parse(code, {suppressErrors:true})` pre-check + 안전망 cleanup
>   + `+layout.svelte` 의 `afterNavigate` sweep (`body > svg[id^="mm-"]` /
>   `body > div[id^="dmm-"]`).
> - 2026-06-08 BUG-056 / DEV-119: 인앱 `ConfirmDialog` 컴포넌트 (Esc/Enter +
>   theme 토큰 + danger 변형) 도입 후 8 사이트의 native `window.confirm()` 교체
>   — 댓글/규칙/캠페인/체크리스트 삭제 + admin status·type rename cascade +
>   backup 복원 + rules nav-away. reindex 만 사용자 지시로 sweep 제외.
> - 2026-06-08 DEV-117: CLI 의 `recents::add` 호출 제거 — CLI 활동이 Welcome
>   '최근 연 길드' 를 오염시키던 문제. recents 의미를 GUI open 시점으로 한정.
> - 2026-06-08 DEV-118: 댓글 답글 폼 자동 focus + scrollIntoView (긴 댓글이라
>   폼이 화면 밖에 mount 되어 못 보던 케이스).
> - 2026-06-08 DEV-120: admin reindex 후 600ms 토스트 → `window.location.reload()`
>   — 모든 페이지 / store 가 fresh 데이터로.
> - 2026-06-08 BUG-058: light 테마 date picker 아이콘 흰색 — `[data-theme='light']`
>   에 `color-scheme: light` 누락. 1 line fix.
> - 2026-06-08 BUG-059: drift detection 의 시간 임계값을 `index.db` 파일 mtime
>   → `app_meta.last_indexed_at` ISO 마커로 교체 (migration 0014). SQLite WAL /
>   Store::open 의 mtime 부작용으로 외부 편집을 못 잡던 false negative 해소.
>   fix1 의 빈 마커 → fallback 이 같은 버그 경로였던 부트스트랩 결함을 fix2 에서
>   epoch fallback 으로 정정.
> - 2026-06-08 BUG-060: invalid urgency (범위 1..=4 밖) 데이터가 들어오면
>   `URGENCY_LABEL[u]` undefined → `.length` 폭발로 보드 mount 실패. `types/index.ts`
>   에 `urgencyLabel(u)` / `urgencyColor(u)` 헬퍼 (4 fallback) 추가 + QuestBoard
>   4곳 + QuestListItem + quest detail 의 bare access 교체.
> - 2026-06-09 DEV-074 fix20~22 (sweep A+B+C): semantic 토큰 (`--accent-secondary`,
>   `--orange`, `--hl-pre/sub/next` + bg, `--hl-parent-bg`, `--selected-bg`,
>   `--edge-pre`) 도입 + `theme.ts::themePalette(eff)` 단일 JS source +
>   QuestBoard / quest detail / rules / campaigns / welcome / SchemaAheadBanner /
>   QuestList / quest-node-svg 의 hex 와 `eff === 'light' ? ...` 분기 모두 정리.
>   `src` 안 색 hex 0개. 재발 방지 규칙은 `docs/guild-rules.md` / `.guild/rules/
>   frontend-theme-tokens.md` / DEV-074 본문 3곳에 명시.
> - 2026-06-09 DEV-128: 댓글 #N 표시 + anchor 점프 — CommentEntry.id 는 이미
>   있어 표시만, `<li id="comment-N">` + 답글은 `↩ #parent`.
> - 2026-06-09 DEV-127 / DEV-123: Quest Detail floating cluster — '맨 위로' / '댓글로'
>   / '메모로' 점프. DEV-109 의 단일 버튼을 `.jump-cluster` 로 refactor.
> - 2026-06-09 DEV-125: Nav 에 테마 토글 (system → light → dark 순환) — system
>   모드는 우하단 accent 도트로 표시. Settings 라디오는 그대로 유지.
> - 2026-06-09 DEV-126: 페이지 새로고침 후 스크롤 위치 유지 — sessionStorage
>   path 별 scrollY, scroll throttle 200ms + beforeunload 저장 + mount 시 rAF 2회
>   후 복원. DEV-120 admin reindex 후 reload 와 결합.
> - 2026-06-09 DEV-124: Quest Detail 의 Successors 섹션에 '+ 추가' 버튼 +
>   core::services::list_candidates 에 'succ' relation 추가 (prereq mirror —
>   has_prerequisite_path(id, c.id) 가 false). pickCandidate 는 succ 면
>   addPrerequisite(candidate, id) (방향 반대).
> - 2026-06-09 BUG-057: Quest Board 노드 흐림 — (A) cytoscape({pixelRatio:
>   clamp(devicePixelRatio,1,3)}) 명시 + (B) makeSvgUrl / quest-node-svg 의 SVG
>   width/height 를 dpr 배 px 로 발급 (viewBox 로 좌표 logical 보존). 보더 /
>   그림자 / 텍스트 모두 또렷.
> - 2026-06-09 DEV-121 Phase 1: startup incremental sync — migration 0015
>   `quests.cached_mtime` (Unix nanoseconds, timezone-independent). 신규
>   `core::incremental::sync_changed_quest_files` 가 각 .md 파일 stat() →
>   DB cached_mtime 비교 → 변경된 것만 UPDATE. 신규/삭제는 needs_full_reindex
>   flag → fallback `drift::auto_resync`. `Store::open_with_sync` helper +
>   GUI startup hook 교체 (`drift::auto_resync` → `incremental::sync_on_open`).
>   CLI / server 는 변경 없음 (CLI: stale 사용자 책임, server: DEV-122 분리).
> - 2026-06-09 DEV-122 등록 (open): server long-running 의 startup + mid-runtime
>   sync 전략. prerequisite DEV-121. (S1 startup + M1~M4 mid-runtime 옵션.)
>
> 자세한 설계 근거: `docs/architecture-refactor.md`, `docs/storage-design.md`.

## API 엔드포인트 (현재 구현)

### Quest 도메인

| Method | Path | 설명 |
|---|---|---|
| GET    | `/health` | 서버 상태 |
| GET    | `/api/quest-types` | Quest 타입 목록 |
| GET    | `/api/quest-statuses` | Quest 상태 목록 |
| GET    | `/api/quests` | Quest 목록 (생성 역순) |
| POST   | `/api/quests` | Quest 생성 |
| GET    | `/api/quests/:id` | Quest 상세 (sub_quests, prerequisites, position 포함) |
| PATCH  | `/api/quests/:id` | Quest 수정 (title / description / urgency) |
| DELETE | `/api/quests/:id?cascade=ID,ID` | Quest 삭제 (선택적 cascade) |
| GET    | `/api/quests/by/:slug` | slug 로 상세 조회 (예: `DEV-001`) |
| PATCH  | `/api/quests/:id/status` | 상태 변경 |
| PATCH  | `/api/quests/:id/parent` | 부모 변경 (`null` 로 분리) |
| GET    | `/api/quests/:id/candidates?relation=parent\|sub\|prereq` | 관계 추가 후보 |
| POST   | `/api/quests/:id/prerequisites` | 선행 퀘스트 추가 |
| DELETE | `/api/quests/:id/prerequisites/:prereq_id` | 선행 퀘스트 제거 |
| PUT    | `/api/quests/:id/position` | Quest Board 노드 위치 저장 |
| GET    | `/api/quest-positions` | 모든 노드 위치 (alive quest 만) |
| GET    | `/api/quest-dependencies` | 모든 선행 관계 (양 끝 alive 만) |
| GET    | `/api/deleted-quests` | soft deleted 퀘스트 목록 |
| PATCH  | `/api/quests/:id/restore` | soft delete 취소 (alive 복원) |
| GET    | `/api/quests/:id/history` | DEV-013: 상태 / 타입 변경 이력 |
| PATCH  | `/api/quests/:id/type` | DEV-055: type 변경 (slug 바뀜, 관련 파일 cascade) |
| PATCH  | `/api/quests/:id/due` | DEV-076: desired_due / required_due 설정·해제 |
| PATCH  | `/api/quests/:id/tags` | DEV-068: 태그 전체 교체 (body `{tags: string[]}`) |
| GET    | `/api/quests/:id/campaigns` | DEV-011: 이 quest 가 속한 캠페인 목록 |

### Campaign (DEV-011)

| Method | Path | 설명 |
|---|---|---|
| GET    | `/api/campaigns` | 목록 (옵션 `?status=active\|done\|planned`) |
| POST   | `/api/campaigns` | 새 캠페인 (자동 C-NNN slug) |
| GET    | `/api/campaigns/:slug` | 상세 (체크리스트 + linked quests 포함) |
| PATCH  | `/api/campaigns/:slug` | 메타 수정 (title / period / status / description / display_order) |
| DELETE | `/api/campaigns/:slug` | soft delete |
| POST   | `/api/campaigns/:slug/checklist` | 항목 추가 (body 끝에 `- [ ]` append) |
| PATCH  | `/api/campaigns/:slug/checklist/:idx` | 1-based 인덱스 항목 체크/언체크 |
| DELETE | `/api/campaigns/:slug/checklist/:idx` | 항목 삭제 |
| POST   | `/api/campaigns/:slug/quests/:quest_id` | quest 링크 |
| DELETE | `/api/campaigns/:slug/quests/:quest_id` | quest 링크 해제 |
| GET    | `/api/campaigns/active-summaries` | Home carousel 용 — 진행 중 캠페인 + 진행률 |

### Admin (백업 / drift)

> 인증 없음 (MVP). 멀티유저 단계에서 보호 추가.

| Method | Path | 설명 |
|---|---|---|
| POST   | `/api/admin/snapshot` | 즉시 snapshot 생성 |
| GET    | `/api/admin/snapshots` | 사용 가능 snapshot 목록 |
| POST   | `/api/admin/restore` | `{to?: TS}` snapshot 복원 (미지정 시 최신) |
| GET    | `/api/admin/drift` | 파일 vs index.db 일치성 검사 |
| POST   | `/api/admin/reindex` | 파일 → index.db 재구축 |

## 데이터 모델

### 진리원 — `.guild/` 파일

| 자료 | 위치 | 형식 |
|---|---|---|
| Quest 한 건 | `.guild/quests/{slug}.md` | TOML `+++` frontmatter + Markdown body + auto block |
| Campaign 한 건 (DEV-011) | `.guild/campaigns/{slug}.md` | TOML `+++` frontmatter + Markdown body (GFM task list `- [ ]` / `- [x]` 는 어디서든 체크리스트로 추출됨, B3 format) |
| Type 정의 | `.guild/types/{prefix}.toml` | `prefix / color / description / [counter].last_number` |
| Status 정의 | `.guild/statuses/{order}-{slug}.toml` | `sort_order / name_en / name_ko / color` |
| Board 위치 | `.guild/positions.json` | gitignored (UI 상태) |

Quest frontmatter 필드: `quest_id` / `title` / `status` (slug) / `urgency` / `parent` (slug, optional) / `prerequisites` (slug 배열) / `created_at` / `updated_at` / `deleted` / **DEV-076: `desired_due` / `required_due`** (YYYY-MM-DD, optional). Campaign frontmatter 필드: `campaign_id` (`C-NNN`) / `title` / `status` (`active` / `done` / `planned`) / `started_at` / `ended_at` / `linked_quests` (slug 배열) / `display_order` / `created_at` / `updated_at` / `deleted`. 자세한 포맷은 [`storage-design.md`](./storage-design.md).

### 캐시 — `.guild/index.db` (SQLite, gitignored)

쿼리 가속 + 사이클 검증용. 손실 시 `reindex` 로 복구.

| 테이블 | 핵심 컬럼 |
|---|---|
| `quests` | id, quest_type_id, number, title, description, status_id, urgency, parent_quest_id, created_at, updated_at, **deleted_at**, **DEV-076: `desired_due` / `required_due`** (TEXT, YYYY-MM-DD, nullable) |
| `quest_types` | id, prefix, color, description |
| `quest_statuses` | id, name_en, name_ko, color, sort_order, **slug** (DEV-046) |
| `quest_counters` | quest_type_id (PK), last_number |
| `quest_dependencies` | quest_id, prerequisite_id, PK(quest_id, prerequisite_id) |
| `quest_positions` | quest_id (PK), x, y, **quest_slug** (DEV-049) |
| `quest_history` (DEV-013) | id, quest_id, **quest_slug** (DEV-049), ts, op, old_value, new_value, actor |
| **`campaigns`** (DEV-011) | id, campaign_slug (`C-NNN`), title, description, status (`active`/`done`/`planned`), started_at, ended_at, display_order, created_at, updated_at, deleted_at |
| **`campaign_checklists`** | id, campaign_id, text, checked (bool), order_idx |
| **`campaign_quests`** | campaign_id, quest_id, PK(campaign_id, quest_id) |
| **`campaign_counters`** | id (PK, single row=1), last_number |

### Journal — `.guild/backups/journal.db` (AOF)

| 테이블 | 컬럼 |
|---|---|
| `ops` | id, ts, op, args (JSON), result (JSON) |

핵심 무결성:
- 사이클 방지: 부모 변경 / 선행 추가 시 BFS 검증 (index.db 쿼리).
- sub ↔ prereq 상호 배제.
- 직계 부모는 prereq 후보에서 제외.
- Soft delete: 파일 frontmatter `deleted: true`. SELECT 는 `WHERE deleted_at IS NULL` 필터. 파일 위치 그대로 (rename 없음 → git diff 깨끗).
- Counter: type 의 `last_number` 는 단조 증가. ID 재사용 방지 (`check-counters` 로 검증).

## 안전장치 (agent / 자동화 대응)

| 장치 | 위치 | 효과 |
|---|---|---|
| **Journal (AOF)** | `core::store::journal` | 모든 mutation 이 `.guild/backups/journal.db` 의 `ops` 테이블에 append (timestamp + op + args + result JSON). |
| **자동 snapshot** | `core::snapshot::maybe_auto_snapshot` | 매 mutation 끝에 정책 검사 — ops 50 회 또는 24h 도달 시 자동 RDB 스냅샷. 알림 stderr. env `OPENGUILD_AUTO_BACKUP_OPS` / `_HOURS` 로 조정. |
| **수동 snapshot** | CLI `openguild backup`, server `openguild-server snapshot`, GUI `/admin` | 즉시 `.guild/backups/snapshots/{ts}.db` + journal truncate. Retention 7. |
| **Restore** | CLI `openguild restore [--to TS]`, server `openguild-server restore`, GUI `/admin` | snapshot 으로 index.db 복원 (이전 상태는 `.pre-restore.db` 로 자동 백업). |
| **Drift 검사** | server `check-drift [--resync]`, GUI `/admin` | 파일 vs 캐시 불일치 검출. `--resync` 또는 `/admin reindex` 로 복구. |
| **Counter 검증** | server `check-counters [--fix]` | type 의 last_number 무결성 검증. ID 중복 방지. |
| **Single-writer lock** | `core::lock::LockGuard` (`.guild/.lock`) | 동시 mutation 방지. PID stale 자동 강탈. |
| **Soft delete** | quest frontmatter `deleted: true` | 실 삭제 X, `quest restore` 로 복원. 파일 위치 그대로 (git diff 깨끗). |
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

- 멀티유저 인증 (JWT) — DEV-021. 메모의 user_id 격리 (DEV-102) 트리거.
- ✅ 댓글 / 메모 DB 캐시 + snapshot 백업 (DEV-102) — migration 0011 + reindex /
  drift / ops 캐시 sync 완료. 메모 user_id 격리는 DEV-021 진입 시.
- ✅ 태그 (DEV-068) — frontmatter + DB cache + 풀스택 (CLI / HTTP / GUI).
- ✅ 캠페인 quest 진행도 (DEV-093) — status.counts_as_done + 모든 layer.
- ✅ 다크 / 라이트 / 시스템 테마 (DEV-074) — CSS variable backbone + 마이그레이션.
  fix2~10 에서 토큰 확장: `--btn-primary-*` (primary 액션 버튼 통일, light 명도 ↑),
  `--btn-warning-*` (admin 복원), `--card-hl-*` (Home 의 overdue / completed 카드
  그라데이션), `--scrollbar-thumb*` (전역 thin 스크롤바), `--content-max-width`
  (DEV-101 컨텐츠 폭 슬라이더). fix11~17 에서 전역 체크박스 custom +
  `OverlayScrollbar` 컴포넌트 (window / 임의 컨테이너 양쪽 지원, transform 기반
  GPU composite, `target?: HTMLElement` prop). 신규 컴포넌트는 토큰만 참조 —
  `:global([data-theme='light']) .x` 직접 override 금지.
- ✅ Quest List Tree / List 토글 (DEV-065).
- ✅ Quest Detail 후속 퀘스트 (DEV-070).
- ✅ Quest Board toolbar 접기 (DEV-073, fix2~3: New Quest 상단 고정 + 도구바 그
  아래), arrangeNodesGrouped 개선 (DEV-077 + BUG-020 fix2: GLOBAL groupOf 기반
  cluster 식별 — 같은 외부 그룹의 lane 멤버는 같은 cluster 직사각형 공유).
- ✅ 노드 시각 polish: 배경 alpha 0.92 (DEV-112) + 최근 움직인 노드 z-index ↑
  (DEV-115) + drag 중 lane 강조 (DEV-105 fix11).
- ✅ Quest Detail 댓글 / 메모 (DEV-107 fix1): 섹션 접기 + 답글 단위 접기 (영속 X).
  본문이 길 때 우하단 floating `↓ 댓글` 점프 버튼 (DEV-109).
- ✅ Markdown 안 mermaid 다이어그램 (DEV-111) — lazy import, theme dark / default.
- 캠페인 댓글 / 메모 (DEV-100) — quest 와 동일 패턴.
- 다국어 (DEV-015) — i18n backbone 부터.
- 첨부파일 (DEV-069) — 새 기능.
- ✅ 레인 접기 (DEV-105) / 레인 순서 (DEV-059) — 본 라운드 (DEV-105 fix2~7)
  에서 통합 '보드 설정' 모달 + ⚙ 토글로 헤더 정리 + collapsed 영속 + hide
  설정 회귀 fix.
- 커스텀 테마 (DEV-114) — 사용자가 토큰 색 자유 정의 + 프리셋 저장 (DEV-074
  토대 활용).
- GUI 원격 모드 (DEV-113) — `openguild-server` URL 모드. DEV-021 (JWT) 권장.
- Journal replay (DEV-022) — 시점 복원.
- 길드 다중 동시 접속 (현재 SQLite 단일 파일 가정).
- AWS EC2 배포 — CI 는 GitHub Actions 로 일부 구축됨 (`.github/workflows/check.yml`).
