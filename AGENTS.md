# OpenGuild

RPG 테마 프로젝트 이슈 트래커. 세션 시작 시 이 파일을 먼저 읽을 것.

## 절대 규칙

- **git commit 은 사용자가 명시적으로 요청할 때만.**
- **메이저 버전 1은 사용자 명시적 승인 전까지 사용 금지.** 현재 `0.x.x`.

## 구조

```
openguild/
├── backend/      ← Rust + Axum 서버 (Cargo workspace)
├── frontend/     ← Svelte 5 + Vite
├── tools/cli/    ← CLI 'og' — agent / 자동화용 HTTP 클라이언트
└── docs/         ← 기획·설계 문서
```

## 명령어

| 영역 | 위치 | 실행 |
|---|---|---|
| 백엔드 | `backend/` | `cargo run -p server`, `cargo test` |
| 프론트 | `frontend/` | `npm run dev`, `npm run check`, `npm test` |
| CLI    | `tools/cli/` | `cargo build --release` → `og --help` |

CLI 는 HTTP 클라이언트. 사전에 백엔드를 띄울 것 (기본 `http://localhost:3000`, 변경: `OPENGUILD_URL` 또는 `--url`).

## Agent 안전장치

- **Soft delete**: `og quest delete` 는 실삭제 X, `deleted_at` 만 set. 복원은 `og quest restore <slug>`. 목록은 `og quest deleted`.
- **`--yes` 강제**: 삭제는 `--yes` 명시 필수. 미명시 시 거부.
- **`--dry-run`**: `og quest delete --dry-run` / `og quest update --dry-run` 로 영향 미리 확인.
- **자동 백업**: 서버가 1시간마다 `VACUUM INTO` 로 `<guild>/backups/guild.db.<timestamp>` 생성, 7일 보관.
- **Audit log**: 모든 mutation HTTP 호출이 `<guild>/audit.log` 에 timestamped 기록.

agent 권장 패턴: 삭제 전 항상 `--dry-run` → 결과 확인 → `--yes` 로 실행. 한 번에 다수 삭제 금지.

## 문서

| 파일 | 내용 |
|---|---|
| `docs/planning.md` | 기획 결정 — 용어, MVP 범위, 데이터 모델, 향후 기능 |
| `docs/architecture.md` | 시스템 구조, API 엔드포인트, 데이터 모델, 클라이언트 비교 |
| `docs/dev-plan.md` | 단계별 개발 계획 + 진행 상태 |
| `docs/guild-rules.md` | 개발 규칙 — 커밋·브랜치·백엔드/프론트 컨벤션 |

위 문서에 있는 내용은 여기 중복하지 않는다.
