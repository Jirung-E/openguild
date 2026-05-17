# OpenGuild 개발 순서 계획 (MVP)

진행 표기: ✅ 완료 · 🟡 진행 중 · ⚪ 대기

## 1단계 — 프로젝트 초기 설정 ✅
- ✅ GitHub 저장소 + main/develop 브랜치
- ✅ Cargo workspace 초기화 (backend)
- ✅ Svelte 프로젝트 초기화 (frontend)
- ✅ AGENTS.md / CLAUDE.md 작성
- ✅ .gitignore, README

## 2단계 — 백엔드 기반 ✅
- ✅ DB 스키마 설계 + 마이그레이션 (0001 initial, 0002 parent ON DELETE SET NULL)
- ✅ 모델 정의 (server crate `models/quest.rs`, `models/meta.rs`)
- ✅ `{name}.guild` TOML 파싱 (`server/src/guild_file.rs`)
- ✅ Axum 서버 기본 (`server/src/main.rs`, CORS, tracing)
- 📌 `core` crate 분리는 보류 — 현재 server 단일 crate

## 3단계 — 백엔드 API ✅
- ✅ Quest CRUD (`/api/quests`)
- ✅ 상태 변경 (`PATCH /api/quests/:id/status`)
- ✅ 서브퀘스트 (`parent_quest_id`, 부모 변경 별도 endpoint)
- ✅ 선행 퀘스트 (`/api/quests/:id/prerequisites`)
- ✅ 노드 위치 저장 (`/api/quests/:id/position`, `/api/quest-positions`)
- ✅ 사이클 검증 / candidates / cascade 삭제 / sub-prereq 상호배제 / 직계부모 prereq 차단

## 4단계 — 프론트엔드 기반 ✅
- ✅ Svelte 라우팅 (`/`, `/quests/[id]`)
- ✅ API client (`src/lib/api/`)
- ✅ Nav 컴포넌트, NewQuestModal

## 5단계 — Quest List ✅
- ✅ 트리 뷰 (서브퀘스트 접기/펼치기) — `QuestListItem`, `lib/utils/quest-list.ts`

## 6단계 — Quest Board ✅
- ✅ Cytoscape.js + 레인 구성
- ✅ 노드 드래그 + 위치 저장 + 다중 노드 드래그/취소 복원
- ✅ 화살표 (선행: 실선, 서브: 점선)
- ✅ 상태 변경 드래그 (확인 다이얼로그 포함)
- ✅ 그리드 스냅 토글 (G 키, dot 시각화)
- ✅ 정렬 모드 (Group / All) — toolbar + 각 lane header
- ✅ 그룹 정렬: connected components → isolated 우선 + cluster 직사각형
- ✅ undo / redo (단일 + 배치)
- ✅ 연관 퀘스트 하이라이트 + 선택 / 그룹 정렬

## 7단계 — Quest Detail ✅ (테스트 단계)
- ✅ 메타 헤더 (ID, 타입, 긴급도, 상태)
- ✅ marked.js 본문 렌더링
- ✅ CodeMirror 6 편집 모드
- ✅ 신규 퀘스트 시 상태 Open 강제
- ✅ 서브 / 선행 추가 콤보박스 (검색 가능, `QuestCombobox`)
- ✅ 상태 변경 시각 피드백 (펄스 + 체크) — 다이얼로그 X
- ✅ 삭제 모달: cascade 선택 + 전체 선택 체크박스
- ✅ slug 변경 시 detail 자동 reload
- ✅ New Quest 후 보드 머무름 + 펄스 (flashQuestId store)

## 8단계 — Agent / 자동화 인터페이스 ✅
- ✅ CLI `openguild` (`cli/`) — clap + reqwest blocking
- ✅ Quest CRUD / 상태 / 부모 / prereq 명령
- ✅ JSON 출력 옵션 (`--json`)
- ✅ slug → id 자동 변환, 상태/타입 이름 resolve

## 8.5단계 — Agent 안전장치 ✅
- ✅ DB 자동 백업 (startup + 1h 주기, 7일 보관, `VACUUM INTO`)
- ✅ Audit log middleware — 모든 mutation HTTP 요청 timestamped 기록 (`audit.log`)
- ✅ CLI delete `--yes` 강제 + `--dry-run` 미리보기
- ✅ CLI update `--dry-run`
- ✅ Soft delete (migration 0003 `deleted_at`) + `openguild quest restore` + `openguild quest deleted`

## 8.6단계 — core 분리 + CLI 로컬 모드 ✅ (2026-05-15)
- ✅ Phase 1.0: `core` crate 신설, models/db/guild_file/backup/error 이동
- ✅ Phase 1.1: 디렉토리 재구성 (`backend/` 제거, `gui/frontend/` 신설, 루트 평탄화)
- ✅ Phase 1.3: `core::services::{quests, meta}` 추출, server routes 는 얇은 HTTP 어댑터
- ✅ Phase 2: CLI Backend enum (Http/Local), `--remote URL`, cwd `.guild` 자동 탐색
  - 단일 사용자는 서버 띄울 필요 없이 `cargo run --bin openguild -- quest list` 로 바로 사용
- ✅ 테스트: Rust 99 (core 23 + cli 25 + server 51) + frontend 41 = **140 통과**
- 📌 상세 설계: `docs/architecture-refactor.md`

## 8.7단계 — 파일 진리원 + SQLite 캐시 전환 ✅ (2026-05-16 / 17)
- ✅ `.guild/quests/{slug}.md` 가 진리원, SQLite 는 index 캐시
- ✅ types/statuses 도 파일로 (`.guild/types/`, `.guild/statuses/`)
- ✅ AOF journal (`backups/journal.db`) + RDB snapshot 으로 git 모르는 사용자도 안전
- ✅ auto 블록으로 sub/parent/prereq 사람 가독성
- ✅ `core::ops` (mutation orchestration), `repo`, `store`, `snapshot`, `reindex`, `drift`, `counter`, `lock`, `migrate` 신설
- ✅ 자동 백업 정책 (`maybe_auto_snapshot` — ops 50 / 24h)
- ✅ CLI `backup` / `backups` / `restore` 명령 (Http + Local 둘 다)
- ✅ server admin endpoints (`/api/admin/*`) + frontend `/admin` UI
- ✅ 옛 audit middleware / VACUUM backup 코드 제거
- 📌 상세 설계: [`docs/storage-design.md`](./storage-design.md)
- 테스트: Rust 191 (core 110 + cli 25 + server 56) + frontend 41 = **232 통과**

## 9단계 — CI/CD + 배포 ⚪
- ⚪ GitHub Actions: PR 시 cargo check / cargo test / npm check / npm test
- ⚪ AWS EC2 배포 (백엔드)
- ⚪ 프론트엔드 정적 호스팅 (Vercel / Netlify 등)

---

## 추후 (MVP 외)

- 멀티유저 인증 (JWT)
- Campaign / Comment / Memo / Quest History UI
- Quest 타입 / 상태 커스텀
- 다국어
- 길드 규칙 (Guild Rules) 기능
- 다음 퀘스트(Successor) / 부모 퀘스트 직접 변경 UI
- core crate 분리 (server / cli 공유)

## 보류 결정 (재검토 가능)

### CLI REPL 모드 — `openguild` 진입 후 프롬프트 입력

검토 후 **보류**. 이유:

- 단발 호출 + lock/pid 파일 기반 서버 재사용으로 cold start 비용이 충분히 작을 것으로 예상
- REPL 추가 시 비용:
  - 의존성 추가 (`rustyline` 등)
  - 단발 / REPL 두 모드 분기로 코드 복잡도 ↑
  - 안전장치 (`--yes` / `--dry-run` 등) 가 REPL 안에서도 일관 동작하도록 추가 처리
- agent 호출 패턴은 대부분 "process spawn → 단발 명령 → exit code" — REPL 메리트 작음

**재검토 시점**:
- 단발 호출 cold start 가 실 사용에서 느리다고 판명되거나
- agent / 사용자가 interactive 세션을 요구하는 시나리오가 빈번해지면

### Desktop 설치 위치 / Recent guild 저장 위치 — portable app 스타일

`openguild-desktop` 의 Recent guild 목록 및 사용자별 설정을 OS 표준 위치 (`~/.config/...` 등) 가 아닌
**앱 설치 시 사용자가 선택한 디렉토리** 에 저장. portable app 형태 (USB 등에서 동작).

지금은 보류 — 데스크톱 앱 골격 잡힌 후 결정.

**재검토 시점**: Tauri desktop 빌드 / 설치관리자 작업 진입 시.
