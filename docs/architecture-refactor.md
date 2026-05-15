# OpenGuild 아키텍처 리팩토링 계획

> 작성일: 2026-05-14
> 상태: Phase 1 진행 중 (core crate 분리 완료)
>
> 기존 `architecture.md` 는 현재(레거시) 구조를 기록. 본 문서는 **목표 구조** 와 **이전 단계** 를 정의.

---

## 결정 배경

기존 구조: `server` 단일 binary + `tools/cli` 독립 crate. CLI 는 HTTP 로만 서버와 통신.

문제:
- CLI 단일 사용자에게 HTTP 서버 상시 가동이 무거움.
- 데스크톱 앱 / 웹 호스팅 / CLI 모두 같은 도메인 로직이 필요한데, 로직이 `server` 안에 묶여있어 공유 불가.
- 기능 추가 시 server / cli 양쪽 수정 필요. 한쪽 누락 가능성.

대안 검토 후 결론: **core 라이브러리 + 3 binary** 구조.
단일 binary 통합은 사용자에게 서버/GUI 까지 노출되어 거부.
3 binary 분리는 도메인 로직 중복 위험 → core crate 가 single source of truth 역할.

---

## 목표 구조 (확정, 2026-05-14 완료)

```
openguild/
├── Cargo.toml                       ← workspace root (members = core/cli/server[/gui])
├── core/                            ← lib: 도메인 로직 단일 진리원
│   ├── migrations/                  ← sqlx 마이그레이션 (DB 스키마 짝)
│   ├── seed.sql                     ← 개발용 시드
│   └── src/
│       ├── models/                  ← QuestRow, QuestDetail, 요청/응답 타입
│       ├── services/                ← Quest CRUD, 관계, 사이클 검증 (Phase 1.3 예정)
│       ├── db.rs / guild_file.rs / backup.rs / error.rs
│       └── lib.rs
├── cli/                             ← bin `openguild`: 로컬 + 원격
│   ├── Cargo.toml                   ← path = "../core"
│   └── src/main.rs                  ← 로컬: cwd `.guild` → core 직접 / 원격: `--remote` → HTTP
├── server/                          ← bin `openguild-server`: API only (호스팅)
│   ├── build.rs                     ← test.guild 자동 생성
│   └── src/                         ← core 를 HTTP 로 감싸는 얇은 레이어
├── gui/                             ← Rust crate (Tauri desktop binary, Phase 4 예정)
│   ├── Cargo.toml                   ← path = "../core" (workspace member 추가 예정)
│   ├── src/                         ← Tauri Rust (main.rs, invoke 핸들러)
│   ├── tauri.conf.json
│   └── frontend/                    ← Svelte 5 + Vite — gui 의 UI 소스
│       ├── package.json
│       ├── vite.config.ts
│       └── src/                     ← Svelte 컴포넌트 + lib/api 어댑터
├── docs/
├── justfile                         ← 단축 명령어
└── README.md
```

| 컴포넌트 | 형태 | 실행 환경 | 통신 |
|---|---|---|---|
| `core` | lib | — | (다른 crate 가 직접 의존) |
| `cli` | bin | 사용자 PC | local: core 직접 / remote: HTTP |
| `server` | bin | 호스팅 서버 | HTTP REST |
| `gui` | bin (Tauri) | 사용자 PC | invoke → core 직접 |
| `gui/frontend/` | Svelte 정적 자산 | webview / 정적 호스팅 | invoke 또는 fetch |

> `gui` Rust crate 는 Phase 4 에서 신설. 현재 workspace 멤버는 `core`, `cli`, `server`. Phase 4 진입 시 `Cargo.toml` 의 `members` 에 `"gui"` 추가.

---

## URL / 식별 규칙

- 호스팅 길드: `https://openguild.io/<user>/<guild>`
- CLI 원격: `openguild --remote https://openguild.io/alice/monitor quest list`
- CLI 로컬: cwd 가 `.guild` 가진 경로면 자동 인식. 없으면 명시 `--guild ./path`
- 데스크톱: Recent guild 목록 + 파일 더블클릭 (`.guild` 연결)

---

## 핵심 원칙

1. **core 가 진리원**: CRUD / 검증 / 상태 전이 / 백업 / audit 모두 core. cli/server/desktop 은 인터페이스만.
2. **server 는 얇게**: core 호출을 HTTP 로 감싸는 레이어. 자체 비즈니스 로직 금지.
3. **frontend 는 환경 무지**: api 어댑터가 Tauri / 브라우저를 자동 분기. 컴포넌트는 한 벌만 유지.
4. **cli 는 로컬 우선**: 단일 사용자가 서버 안 띄우고도 쓸 수 있어야 함. 원격은 `--remote` flag.

---

## 보류 결정

| 항목 | 결정 | 재검토 시점 |
|---|---|---|
| 인증 (JWT) | 초기 미구현 | server 멀티유저 단계 |
| CLI REPL 모드 | 보류 — 단발 호출 + cold start 충분 | cold start 가 느리다고 판명되면 |
| Desktop portable 설치 | 보류 — 데스크톱 골격 잡힌 후 결정 | Phase 4 진입 시 |
| `backend/` → `crates/` rename | 미정 (옵션 A/B/C) | 사용자 결정 대기 |

---

## 이전 단계 (Phases)

### Phase 1 — core crate 분리 + 디렉토리 재구성 ✅

- ✅ `core` crate 신설 (sqlx/serde/anyhow/thiserror 의존성)
- ✅ `models/` / `db.rs` / `guild_file.rs` / `backup.rs` / `error.rs` 를 server → core 로 이동
- ✅ `error.rs` 는 plain Rust 에러. server 는 `HttpError(AppError)` newtype 으로 axum `IntoResponse` 구현 (orphan rule 회피)
- ✅ server 의존성에서 sqlx / toml 제거 — `openguild-core = { path = "../core" }` 만 남김
- ✅ unit 테스트 추가: core 18 + cli 18 + server 43 = **Rust 79개 통과** (+ frontend vitest 41개 통과)
- ✅ 디렉토리 재구성 (2026-05-14):
  - `backend/{core,cli,server}` → 루트로 평탄화
  - `backend/migrations`, `backend/seed.sql` → `core/` 내부로 (DB 스키마와 짝)
  - `frontend/` → `gui/frontend/` (Tauri 와 한 폴더로 묶을 준비)
  - `backend/`, `tools/` 폴더 삭제
  - 루트 Cargo.toml 신설, `core/src/db.rs` 의 `sqlx::migrate!("./migrations")` 로 경로 갱신
  - `.gitignore` 경로 갱신, `justfile` 신설 (dev/build/test 단축)
- ✅ services 레이어 추출 (Phase 1.3, 2026-05-15) — `core::services::quests` / `core::services::meta` 신설.
  routes/quests.rs (715 라인) 의 SQL · 검증 · 사이클 체크 로직이 전부 core 로 이동.
  server routes 는 axum extractor → service → JSON 직렬화만 하는 얇은 어댑터.

### Phase 2 — CLI 로컬 모드 ✅ (2026-05-15)

- ✅ `cli` 에 `--remote URL` flag 추가, env `OPENGUILD_REMOTE` 도 지원. 기본은 로컬 모드.
- ✅ 로컬 모드: `--guild PATH` 또는 cwd 부터 `.guild` 자동 탐색 (`core::guild_file::find_from_cwd`).
  → `core::db::create_pool` + `run_migrations` → `core::services::*` 직접 호출.
- ✅ `Backend` enum (Http / Local) 으로 dispatch — call site 는 모드 인지 없이 동일 메서드 호출.
- ✅ CLI 의 local DTO 제거 → `openguild_core::models::*` 직접 사용 (중복 제거).
- ✅ 스모크 테스트: init → ping → types → list → new → show 전부 서버 없이 동작.
- ✅ 변경 사항: `--url` → `--remote`, env `OPENGUILD_URL` → `OPENGUILD_REMOTE` (pre-1.0 breaking).

### Phase 3 — Frontend api 어댑터 ⚪

- ⚪ `frontend/src/lib/api/` 에 `tauri.ts`, `http.ts` 분리
- ⚪ `client.ts` 가 `window.__TAURI__` 감지하여 한쪽 export
- ⚪ 컴포넌트는 그대로

### Phase 4 — Desktop (Tauri) 신설 ⚪

- ⚪ `gui/` 를 Tauri Rust crate 로 초기화 (`gui/src/`, `gui/Cargo.toml`, `gui/tauri.conf.json`)
- ⚪ workspace `members` 에 `"gui"` 추가
- ⚪ `tauri.conf.json` 의 `frontendDist = "./frontend/dist"`, `beforeDevCommand = "cd frontend && npm run dev"`, `beforeBuildCommand = "cd frontend && npm run build"`
- ⚪ invoke 핸들러: core 직접 호출 (HTTP X)
- ⚪ Recent guild / 파일 연결 (`.guild` 더블클릭)

### Phase 5 — 전체 검증 ⚪

- ⚪ 3 binary 모두 빌드 확인
- ⚪ frontend Tauri / 브라우저 양쪽 동작
- ⚪ `cargo test --workspace` + `npm test` 통과
- ⚪ 회귀 체크리스트 수동 검증

---

## 폴더 구조 결정 기록 (2026-05-14)

여러 후보를 거쳐 **루트 평탄화 + `gui/` 안에 Tauri Rust + Svelte 통합** 으로 확정.

기각된 안:
- `crates/{core,cli,server,desktop}` + `frontend/` — frontend 가 desktop 과 동급이 아닌 게 어색
- 루트 Cargo.toml + `frontend/` sibling — Cargo.toml 이 frontend 까지 Rust 처럼 보이게 함
- `crates/` + `frontend/` — `crates/` 라는 이름이 polyglot repo 에서 부자연스럽고 frontend 와 비대칭
- 표준 Tauri `gui/src-tauri/` + `gui/frontend/` — `src-tauri` 명이 다른 crate 의 `src/` 와 비대칭

채택 근거:
- 최상위는 역할 기반 명명 (`core` / `cli` / `server` / `gui`) — 모두 동등한 컴포넌트
- `gui` 는 그 자체로 Rust crate. Tauri Rust 가 `gui/src/`, UI 소스가 `gui/frontend/`. desktop binary 의 본체와 UI 자원이 한 폴더에 모임
- 명령어 길이는 루트 `justfile` 로 단축
