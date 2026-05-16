# OpenGuild 저장소 설계 — 파일 진리원 + SQLite 캐시/저널

> 상태: **설계 확정, 구현 대기**
> 작성: 2026-05-16
> 관련: `architecture.md`, `architecture-refactor.md`

---

## 동기

현재 (이 메모 작성 시점) 의 저장소는 SQLite (`guild.db`) 가 진리원. 한계:

- **사람이 읽기 어려움** — quest 내용을 보려면 도구가 필요. binary blob.
- **git 친화성 부족** — DB 파일 commit 해도 diff 무의미, merge 불가, blame 안 됨.
- **다른 도구로 편집 불가** — VS Code / GitHub web / Obsidian 등에서 quest 열기 불가.
- **백업 의존성** — DB 손상 시 VACUUM 스냅샷 없으면 손실.

목표 구조: **파일이 진리원, SQLite 는 캐시 + 저널 / 스냅샷 보관소**.

설계 원칙:
1. 사람이 텍스트 에디터로 quest 읽고 이해 가능 (Markdown + YAML frontmatter).
2. git 사용자는 file diff / blame / branch / PR 자연스럽게 활용.
3. git 안 쓰는 사용자는 자체 backup + journal 시스템으로 안전 보장.
4. SQLite 는 보이지 않는 캐시. 손실되어도 무해 (재구축 가능).
5. 길드 루트는 깨끗 — `.guild/` 폴더 하나에 모든 내부 자료.

---

## 디렉토리 구조

```
guild-root/
├── openguild.guild                  ← 마커 (TOML, git tracked)
└── .guild/
    ├── quests/                      ← 진리원: 사람이 읽는 quest 들 (md)
    │   ├── DEV-001.md
    │   ├── DEV-002.md
    │   └── ...
    ├── types/                       ← 진리원: quest 타입 정의 (toml)
    │   ├── DEV.toml
    │   ├── BUG.toml
    │   └── REQ.toml
    ├── statuses/                    ← 진리원: 상태 정의 (toml)
    │   ├── 1-open.toml
    │   ├── 2-in_progress.toml
    │   ├── 3-done.toml
    │   ├── 4-cancelled.toml
    │   └── 5-on_hold.toml
    ├── index.db                     ← 캐시 (gitignored)
    ├── positions.json               ← Board UI 좌표 (gitignored)
    └── backups/                     ← 독립 복구 시스템 (gitignored)
        ├── journal.db               ← AOF (현 시점까지의 ops)
        └── snapshots/               ← RDB 스냅샷 (시점별 index.db 사본)
            ├── 20260516-150000.db
            ├── 20260517-150000.db
            └── ...
```

**git tracked**: `openguild.guild`, `.guild/quests/`, `.guild/types/`, `.guild/statuses/`
**git ignored**: `.guild/index.db`, `.guild/positions.json`, `.guild/backups/`

`.guild/.gitignore` 가 자동 생성되어 위 규칙 적용.

---

## 파일 포맷

### Quest (`.guild/quests/{quest_id}.md`)

Frontmatter 는 **TOML `+++`** 형식 — 프로젝트 다른 메타 파일 (`{name}.guild`, `types/`, `statuses/`) 과 일관.

```markdown
+++
quest_id = "DEV-001"
title = "Tauri desktop 앱 (gui/ crate)"
status = "open"
urgency = 2
prerequisites = []
created_at = 2026-05-16T15:00:00Z
updated_at = 2026-05-16T15:01:00Z
deleted = false
# parent 는 키 자체 생략 = root quest
+++

Tauri Rust shell + frontend api 어댑터 + 파일 연결

<!-- openguild:auto-begin — 아래는 자동 생성. 직접 수정하지 마세요. -->
## Sub-quests
- [DEV-002](DEV-002.md) — Frontend api 어댑터 (Tauri/브라우저 분기)
- [DEV-003](DEV-003.md) — gui/ Tauri crate 초기화 + workspace 등록
- [DEV-004](DEV-004.md) — Tauri invoke 핸들러 — core 직접 호출
- [DEV-005](DEV-005.md) — .guild 파일 연결 (OS file association)
- [DEV-006](DEV-006.md) — Recent guild 목록

## Prerequisites
- (없음)
<!-- openguild:auto-end -->
```

자식 quest 예시 (선행 있음):
```markdown
+++
quest_id = "DEV-004"
title = "Tauri invoke 핸들러 — core 직접 호출"
status = "open"
urgency = 3
parent = "DEV-001"
prerequisites = ["DEV-002", "DEV-003"]
created_at = 2026-05-16T15:00:00Z
updated_at = 2026-05-16T15:01:00Z
deleted = false
+++

frontend 의 api 호출이 invoke 로 → core::services::* 직접 실행

<!-- openguild:auto-begin — 아래는 자동 생성. 직접 수정하지 마세요. -->
## Parent
[DEV-001](DEV-001.md) — Tauri desktop 앱 (gui/ crate)

## Sub-quests
- (없음)

## Prerequisites
- [DEV-002](DEV-002.md) — Frontend api 어댑터 (Tauri/브라우저 분기)
- [DEV-003](DEV-003.md) — gui/ Tauri crate 초기화 + workspace 등록
<!-- openguild:auto-end -->
```

#### Frontmatter 필드

| 필드 | 타입 | 설명 |
|---|---|---|
| `quest_id` | string | slug 형식 (`DEV-001`). 파일명과 일치. 변경 불가 |
| `title` | string | 한 줄 제목. TOML escape 규칙 적용 |
| `status` | string | status 파일명의 slug (`open` / `in_progress` / `done` / ...) |
| `urgency` | int | 1=Critical / 2=High / 3=Medium / 4=Low |
| `parent` | string (optional) | 부모 quest_id. 없으면 키 자체 생략 (root quest) |
| `prerequisites` | string[] | 선행 quest_id 배열 (비었으면 `[]`) |
| `created_at` | RFC 3339 datetime | TOML native datetime (UTC) |
| `updated_at` | RFC 3339 datetime | 마지막 mutation 시각 |
| `deleted` | bool | soft delete flag. true 면 list 에서 숨김 |

**모든 참조는 slug 사용** (`DEV-001`), numeric ID 폐지.

#### Body 의 auto 블록

- `<!-- openguild:auto-begin -->` ~ `<!-- openguild:auto-end -->` 사이는 도구가 매 mutation 시 재생성.
- 사용자가 description 은 블록 밖에 작성 (위/아래 자유).
- 자기 자신이 root quest (parent == null) 면 "Parent" 섹션 생략.
- 자식/선행/의존 없을 때 "(없음)" 표시 (또는 섹션 자체 생략 — 구현 시 결정).

#### Soft delete

- `deleted: true` frontmatter 플래그만.
- 파일 위치 변경 X — git diff 깨끗.
- list / show 명령 기본적으로 숨김.
- `openguild quest deleted` 로 목록 / `openguild quest restore` 로 복원.
- 영구 삭제 (`openguild quest purge --force`): 파일 삭제 + journal 에 `purge_quest` op 기록.

### Type (`.guild/types/{prefix}.toml`)

```toml
# ⚠️ [counter] 섹션은 자동 관리 필드 — 절대 수동으로 수정하지 마십시오.
# last_number 는 부여된 quest ID 가 재사용되지 않도록 보호하는 단조 증가 카운터.
# - 줄이면 ID 중복으로 데이터 손상 발생.
# - 늘리면 번호 건너뛰어 추적이 어려워짐.
# 시작 시 quests/ 의 실제 max 번호와 검증하여 보호.

prefix = "DEV"
color = "#4A90D9"
description = "일반 개발 작업"

[counter]
last_number = 19
```

#### Counter 보호

- TOML 헤더의 경고 문구 (위와 같이).
- 시작 시 검증:
  - `max(quests/DEV-*.md 의 번호) > last_number` → 경고 + 자동 보정 (큰 값으로 갱신) + journal 에 `counter_correction` op 기록.
  - `last_number` 가 직전 시작 때보다 작아짐 감지 → error + 복구 안내 (snapshot 에서 복원 또는 quests/ 에서 재계산).
- 자동 보정 시 stderr 에 명시.

### Status (`.guild/statuses/{sort_order}-{slug}.toml`)

파일명 형식: `{sort_order}-{slug}.toml`. 예: `1-open.toml`, `2-in_progress.toml`.
- prefix 숫자가 디렉토리 정렬 보장.
- slug 부분이 frontmatter `status` 값과 일치.

```toml
sort_order = 1
name_en = "Open"
name_ko = "게시됨"
color = "#8B95A1"
```

---

## SQLite 두 종류

### `index.db` (캐시)

- 스키마: 현재 SQLite 스키마 동일 (`quests`, `quest_types`, `quest_statuses`, `quest_dependencies`, `quest_positions`, `quest_counters`).
- 역할: 빠른 쿼리 — cycle check / candidates / list 정렬 / 통계.
- 손실되어도 OK — 파일에서 재구축 (`openguild reindex`).
- 시작 시 검증: 파일 mtime 과 index 행의 updated_at 비교 → 불일치 시 부분 reindex.

### `backups/journal.db` (AOF)

```sql
CREATE TABLE ops (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    ts        TEXT NOT NULL,        -- ISO 8601 UTC
    op        TEXT NOT NULL,        -- 'create_quest', 'change_status', 'add_prerequisite', ...
    args      TEXT NOT NULL,        -- JSON blob
    result    TEXT                  -- JSON blob (slug, etc.)
);

CREATE INDEX idx_ops_ts ON ops (ts);
```

- 모든 mutation 이 한 row INSERT.
- snapshot 만들 때 truncate.
- replay: `SELECT * FROM ops ORDER BY id` → 각 row apply.

### `backups/snapshots/{ts}.db` (RDB)

- 형식: index.db 스키마 그대로의 binary copy.
- 트리거:
  - 수동 `openguild backup` 또는 `openguild-server backup`
  - `openguild-server host` 시작 시 + 1시간 주기
  - CLI startup 시 마지막 snapshot 으로부터 N 시간 / M mutation 경과 시
- Retention: 최근 7개 보관, 그 이상은 제거.

---

## 작동 흐름

### Mutation (예: create quest)

```
1. 입력 검증 — type/status 존재, parent alive, 사이클 안 됨 (index.db 쿼리)
2. journal.db 에 INSERT:
   {ts: now, op: 'create_quest', args: {...}}
3. 새 파일 작성 (atomic):
   - .guild/quests/DEV-020.md 를 tmpfile 에 작성
   - fs::rename → 최종 위치 (POSIX/Windows 모두 atomic)
4. types/DEV.toml 의 last_number 갱신 (atomic write 같은 식)
5. 영향받는 다른 quest 파일들의 auto 블록 재생성:
   - 부모 있으면 부모 파일 (Sub-quests 목록에 추가)
6. index.db UPDATE — 캐시 동기화
7. stdout 출력
```

### Snapshot (수동 또는 자동)

```
1. .guild/index.db 의 무결성 검증 (PRAGMA integrity_check)
2. cp .guild/index.db → .guild/backups/snapshots/{now}.db
3. DELETE FROM journal.db.ops  (truncate)
4. 오래된 snapshot 제거 (7개 retention)
5. stdout 알림
```

### Restore

```bash
openguild restore                              # 최신 snapshot + 현 journal 로 재구축
openguild restore --to 20260517-150000        # 그 시점까지만
openguild restore --list                       # 사용 가능 snapshot 목록
```

알고리즘:
1. 대상 snapshot 선택 (가장 늦은 ≤ target 시각).
2. snapshot.db → temp index.db 로 복사.
3. journal.db 에서 `ts <= target` 인 ops 를 순서대로 apply.
4. `.guild/quests/`, `types/`, `statuses/` 비우고 temp index.db 에서 재출력 (frontmatter + auto 블록 포함).
5. temp → `.guild/index.db` 로 이동.
6. journal.db 절단 (이미 적용된 ops 제거 또는 truncate).

### 외부 편집 처리

사용자가 `.guild/quests/DEV-001.md` 를 직접 편집:

1. 다음 CLI 명령 시작 시 모든 quest 파일 mtime vs index.db 의 updated_at 비교.
2. 파일이 새것 → 그 quest 만 re-parse + re-validate + index 갱신.
3. 검증 실패 (잘못된 YAML / 존재하지 않는 참조 / 사이클):
   - stderr 경고
   - 그 quest 는 index 에서 제외 (다른 quest 정상 동작)
   - 복구 안내 (구문 수정 또는 `openguild restore`).
4. journal 에 `external_edit` op 기록 (선택적, 추후 결정).

### 동시 접근

`.guild/.lock` 파일로 single-writer 보장:

- mutation 시도 시 lock 파일 검사 (PID + 시작 시각 + 명령).
- 다른 프로세스 lock 발견 시:
  - PID 존재 + 살아있음 → "다른 프로세스가 mutating 중" error + 거부.
  - PID stale → lock 강탈 + 자기 PID 기록.
- `openguild-server host` 중에 CLI mutation 시도 → 친절 거부 ("서버 실행 중. server 통해 변경하세요").
- 종료 시 lock 해제 (정상 종료 + signal handler).

---

## 마이그레이션

현재 `guild.db` (이 메모 작성 시점에 19 quests) → `.guild/` 구조:

```bash
openguild migrate-to-files
```

알고리즘:
1. `guild.db` 의 SELECT 로 모든 데이터 추출.
2. `.guild/quests/`, `types/`, `statuses/` 디렉토리 생성.
3. types → `.guild/types/{prefix}.toml` (counter 정보 포함).
4. statuses → `.guild/statuses/{sort_order}-{slug}.toml`.
5. quests → `.guild/quests/{slug}.md` (frontmatter + body + auto 블록).
6. dependencies → 각 quest 의 frontmatter prerequisites 배열.
7. positions → `.guild/positions.json`.
8. `.guild/index.db` 신규 생성 후 `reindex`.
9. `.guild/backups/journal.db` 신규 빈 journal 생성.
10. `.guild/backups/snapshots/{now}.db` 에 초기 스냅샷.
11. 기존 `guild.db`, `backups/`, `audit.log` 는 그대로 두고 사용자가 확인 후 수동 삭제.

---

## 구현 단계

| 단계 | 작업 | 의존 | 소요 |
|---|---|---|---|
| **F1** | `core::repo` 모듈 — 파일 read/write + frontmatter parser/serializer | — | 반나절 |
| **F2** | quest / type / status 시드 파일 포맷 정의 + 시드 데이터 | F1 | 1시간 |
| **F3** | `core::services::*` 재작성 — 파일 IO + index UPDATE + journal INSERT | F1 | 1일 |
| **F4** | auto 블록 렌더러 + 영향 범위 추적 (parent 변경 시 옛/새 부모 갱신) | F3 | 2시간 |
| **F5** | `reindex` — 파일들로부터 index.db 재구축 | F1, F2 | 2시간 |
| **F6** | snapshot / restore CLI 명령 | F3 | 반나절 |
| **F7** | external 편집 감지 + 부분 reindex | F5 | 2시간 |
| **F8** | lock 파일 메커니즘 | F3 | 1시간 |
| **F9** | counter 검증 + 자동 보정 | F2, F5 | 1시간 |
| **F10** | `migrate-to-files` 명령 | F1~F9 | 2시간 |
| **F11** | 기존 audit / auto-backup 코드 제거 | F3, F6 | 1시간 |
| **F12** | `openguild init` 이 `.guild/` 디렉토리 + 시드 + gitignore 생성 | F2 | 1시간 |
| **F13** | 테스트 갱신 — 기존 services 테스트들이 파일 IO 도 검증하도록 | 전부 | 반나절 |

**총 3-4일** (단일 작업자 기준).

---

## 보류 / 미결 결정

| 항목 | 상태 | 재검토 시점 |
|---|---|---|
| Recent guilds 저장 위치 (OS 표준 vs portable) | 보류 — 데스크톱 단계로 묶음 | Tauri gui crate 신설 시 |
| Quest position git tracking | `positions.json` gitignored 로 결정 | 변경 시 명시 |
| File watching (양방향 자동 sync) | Phase 5 (미래) | 단방향 (DB→파일) 안정화 후 |
| auto 블록에 상태 표시 (✅ 🟡) | 옵션 — 추후 추가 | F4 안정화 후 |
| Quest body 의 markdown 렌더링 호환성 (GitHub web 등) | 표준 GFM 만 사용 → 자동 호환 | — |
| Permanent delete (purge) | `--force` 옵션 으로 가능 | F3 시 함께 |
| 다중 CLI 동시 read | lock 안 잡음 (read-only) — write 만 lock | — |

---

## 사용자 경험 — 두 트랙

### git 사용자
- `openguild.guild` + `.guild/quests/` + `.guild/types/` + `.guild/statuses/` 가 git tracked.
- `git log .guild/quests/DEV-001.md` 로 그 quest 의 모든 변경 이력.
- `git blame .guild/quests/DEV-001.md` 로 줄 단위 책임자.
- branch / PR 로 quest 변경 사항 협업 가능 (예: 큰 quest 분할 PR).

### git 모르는 사용자
- `.guild/backups/snapshots/` + `journal.db` 로 시점 복원 가능.
- 정기적 자동 snapshot 으로 안전.
- GUI / CLI 가 복원 명령 제공 — git 명령 몰라도 됨.

### 두 트랙 공통
- 파일이 진리원. SQLite 손실되어도 무해.
- 다른 도구 (VS Code / Obsidian / GitHub web) 에서 quest 읽기 가능.
- core::services 가 같은 추상화 — server / cli / gui 셋 다 동일하게 사용.

---

## 핵심 원칙 요약

1. **파일 진리원**. `.guild/quests/{slug}.md` 등이 source of truth.
2. **SQLite 는 보이지 않는 인프라**. `index.db` (캐시) + `journal.db` (AOF) + `snapshots/*.db` (RDB).
3. **루트 깨끗**. `openguild.guild` + `.guild/` 만.
4. **git 선택사항**. 사용하면 좋고, 안 써도 자체 백업으로 안전.
5. **자동 vs 수동 명확히**. `[counter]` 같은 자동 필드엔 경고. auto 블록엔 명시적 마커.
6. **사람이 읽을 수 있는 포맷**. Markdown + YAML/TOML — IDE / GitHub web / Obsidian 등에서 자연스러움.

---

## ⚠️ 구현 완료 후 — dogfood 전환 (필수)

본 설계가 완료되면 OpenGuild 자체 프로젝트의 **앞으로 할 일 관리는 OpenGuild 로**.
별도 todo 도구 / GitHub Issues 보조 사용 안 함.

전환 절차:
1. 새 저장소 모델로 마이그레이션 (`openguild migrate-to-files`) — 현 19 quests 이전.
2. 새 작업 / 버그 / 개선은 `openguild quest new --type ...` 로 추가.
3. `AGENTS.md` 의 "절대 규칙" 섹션이 이미 이 사실을 명시.
4. `docs/dev-plan.md` 의 향후 로드맵 항목들도 점진적으로 quest 로 이전 (대단위는 docs 유지, 세부는 quest).

이는 OpenGuild 의 단순한 self-test 가 아니라 **본 도구의 차별점을 직접 체험** 하기 위한 의도적
선택. git native + 파일 기반 issue tracker 라는 가치를 매일 사용하며 검증.
