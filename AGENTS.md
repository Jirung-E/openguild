# OpenGuild — Agent 인덱스

> AI agent 가 세션 시작 시 가장 먼저 읽는 파일.
> 이 파일은 **최소 정보 + 다른 문서로의 인덱스** 역할.
> 자세한 내용은 각 문서로 들어가서 확인할 것.

## 절대 규칙

- **git commit / git push 는 사용자가 명시적으로 요청할 때만.**
  - "커밋해" / "push 해" 같은 직접 지시가 있을 때만 실행.
  - 작업 완료 후 자동 커밋 금지. amend / reset / force push 도 명시 요청 필요.
  - 변경 사항이 stage 되어 있어도 사용자가 다음 행동을 결정하게 둘 것.
- **메이저 버전 1은 사용자 명시적 승인 전까지 사용 금지.** 현재 `0.x.x`.
- **개발 작업 관리 도구**: 저장소 설계 (`docs/storage-design.md`) 구현이 거의 완료됨 (2026-05-16, F7 제외).
  앞으로 할 일 / 진행 상태 추적은 **OpenGuild 자체 (CLI) 로** 관리.
  - `.guild/quests/*.md` 가 진리원 (git tracked).
  - `openguild quest new/start/done/...` 으로 mutation — 파일 자동 갱신.
  - 외부 todo 도구 / GitHub Issues 보조 사용 X.

## 한 줄 요약

RPG 테마 프로젝트 이슈 트래커. Rust(Axum) 백엔드 + Svelte 프론트엔드 + Rust CLI (`openguild`).

## 디렉토리

```
openguild/
├── Cargo.toml      ← workspace = ["core", "cli", "server"]
├── core/           ← lib: 도메인 로직 (sqlx, 모델, migrations 포함)
├── cli/            ← bin `openguild` (HTTP/로컬 클라이언트)
├── server/         ← bin Axum API 서버
├── gui/            ← Tauri desktop (Phase 4 예정)
│   └── frontend/   ← Svelte 5 + Vite
├── justfile        ← dev/build/test 단축
└── docs/           ← 기획·설계·사용 문서
```

## 문서 인덱스

### Agent 가 OpenGuild **를 개발** 할 때 (코드 수정)

| 문서 | 내용 |
|---|---|
| `docs/architecture.md` | 시스템 구조 / API 엔드포인트 / 데이터 모델 / 안전장치 |
| `docs/architecture-refactor.md` | core 분리 + CLI 로컬 모드 등 구조 변경 이력 / 미래 계획 |
| `docs/storage-design.md` | 파일 진리원 + SQLite 캐시/저널 — 차기 저장소 설계 (구현 대기) |
| `docs/dev-plan.md` | 단계별 개발 계획 + 진행 상태 |
| `docs/planning.md` | 기획 결정 (용어, MVP 범위, 향후 기능) |
| `docs/guild-rules.md` | 개발 규칙 (커밋·브랜치·백/프론트 컨벤션) |

### Agent 가 OpenGuild **를 사용** 할 때 (도구로 작업 관리)

| 문서 | 내용 |
|---|---|
| `docs/AGENTS_OPENGUILD_USAGE.md` | CLI (`openguild`) 사용법, 워크플로 패턴, 안전장치 |

## 빠른 명령어 참조

| 영역 | 위치 | 실행 |
|---|---|---|
| 백엔드 | (repo root) | `cargo run --bin openguild-server -- host`, `cargo test --workspace` (또는 `just dev-server`). 관리: `openguild-server backup` / `info` |
| 프론트 | `gui/frontend/` | `npm run dev`, `npm run check`, `npm test` (또는 `just dev-frontend`) |
| CLI    | (repo root) | `cargo run --bin openguild -- --help` (또는 `cargo build --release` → `target/release/openguild`) |

위 문서에 있는 내용은 여기 중복하지 않는다.
