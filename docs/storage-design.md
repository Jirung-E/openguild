# openguild 저장소 설계 — 파일 진리원 + SQLite 캐시/저널

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
3. git 은 **사용자 선택사항** — 안 써도 자체 backup + journal 시스템으로 안전 보장.
4. SQLite 두 종류 분리:
   - **`index.db`** = 쿼리 캐시 (gitignored, 재구축 가능, 손실되어도 무해).
   - **`backups/journal.db` + `snapshots/*.db`** = 정식 백업 시스템 (gitignored, 시점 복원 보장).
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
    ├── campaigns/                   ← DEV-011: 진리원: 캠페인 (md, B3 형식)
    │   ├── C-001.md
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

**git tracked**: `openguild.guild`, `.guild/quests/`, `.guild/campaigns/`, `.guild/types/`, `.guild/statuses/`
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
| `desired_due` | string (optional, DEV-076) | 희망 기한 (`YYYY-MM-DD`). 정보성 — Home 임박 판단에는 사용 X. 미설정 시 키 자체 생략 |
| `required_due` | string (optional, DEV-076) | 필수 기한 (`YYYY-MM-DD`). Home "마감 임박" / "Overdue" 섹션의 기준. 미설정 시 키 자체 생략 |

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

### Campaign (`.guild/campaigns/{slug}.md`) — DEV-011

캠페인은 "다음 마일스톤" 계획서. quest 와 다대다 링크 + 자체 체크리스트 +
기간(`started_at` / `ended_at`). slug 는 `C-001` ~ `C-NNN` (3자리 zero-pad, 단일 카운터).

**파일 형식 (B3)**: TOML `+++` frontmatter + Markdown body. body 안 어디든 등장하는
GFM task list (`- [ ]` / `- [x]`) 는 단방향으로 (파일 → DB) 체크리스트 항목으로 추출 ·
동기화됨. 본문 prose 와 체크리스트가 한 파일에 공존 가능.

```markdown
+++
campaign_id = "C-001"
title = "베타 1.0.0"
status = "active"
started_at = ""
ended_at = "2026-06-30"
linked_quests = [
    "DEV-012",
    "DEV-015",
    "DEV-087",
]
display_order = 0
created_at = "2026-05-29T01:06:32+09:00"
updated_at = "2026-05-29T01:07:20+09:00"
deleted = false
+++

## 베타 1.0.0

openguild 의 첫 베타 마일스톤.

- [ ] DEV-087 홈 캠페인 배너 이미지
- [x] DEV-012 댓글/메모
- [ ] DEV-015 다국어 (영/한)
```

#### Frontmatter 필드

| 필드 | 타입 | 설명 |
|---|---|---|
| `campaign_id` | string | slug 형식 (`C-001`). 파일명과 일치. 변경 불가 |
| `title` | string | 캠페인 제목 |
| `status` | string | `active` / `done` / `planned` |
| `started_at` | string (optional) | `YYYY-MM-DD`. 빈 문자열 = 미정 (무기한 시작) |
| `ended_at` | string (optional) | `YYYY-MM-DD`. 빈 문자열 = 미정 (무기한 종료) |
| `linked_quests` | string[] | 링크된 quest_id 배열 |
| `display_order` | int | 정렬 인덱스 (수동 정렬 시 사용) |
| `created_at` / `updated_at` | RFC 3339 datetime | |
| `deleted` | bool | soft delete flag |

#### 본문 체크리스트 (단방향 sync)

- 파일이 진리원 — `campaign checklist add/check/uncheck/rm` 가 body 의 `- [ ]` 줄을 수정.
- reindex 시 body 의 모든 `- [ ]` / `- [x]` 줄을 순서대로 `campaign_checklists` 테이블에 적재.
- 본문 prose 와 자유롭게 섞을 수 있음 (heading / paragraph / list / 체크리스트 혼재 가능).
- 진행률 = `checked / total` (Home 카드 progress bar).

#### linked_quests

- 캠페인 ↔ quest 다대다 관계 (`campaign_quests` 테이블).
- 한 quest 가 여러 캠페인에 속할 수 있음. 한 캠페인은 여러 quest 링크.
- Quest Detail UI 의 "Campaigns" 섹션 (`/api/quests/:id/campaigns`) 으로 역참조 조회.

#### Counter (`campaign_counters`)

- 단일 row (`id=1`) — `last_number` 단조 증가. quest counter 패턴과 동일.
- 새 캠페인 = `last_number + 1` → `C-NNN`.

---

## SQLite 두 종류

### `index.db` (캐시)

- 스키마: quest 계열 6 테이블 + DEV-013 `quest_history` + **DEV-011 campaign 계열 4
  테이블** (`campaigns` / `campaign_checklists` / `campaign_quests` /
  `campaign_counters`) + DEV-068 `quest_tags`. migration 0001~0010.
  `set_ignore_missing(true)` 로 더 새로운 binary 의 마이그레이션을 적용 후, 옛
  binary 로 열어도 brick 안 되도록 backward compat (BUG-041).
- 역할: 빠른 쿼리 — cycle check / candidates / list 정렬 / 통계 / 캠페인 진행률 /
  Home 임박 분류 (DEV-076).
- 손실되어도 OK — 파일에서 재구축 (서버 `openguild reindex` 또는 GUI 상단
  reindex 버튼 — DEV-095).
- 시작 시 검증: `drift::detect_drift` 가 quest 본문 파일 mtime 과
  `app_meta.last_indexed_at` (마지막 reindex 의 ISO 시각) 비교. 불일치 시
  `drift::auto_resync` 가 자동 reindex (server / cli / gui 모두 Store::open 직후
  호출 — BUG-049). 마커 없거나 빈 값이면 epoch fallback → 첫 부트스트랩 reindex
  강제 (BUG-059). 이전엔 `index.db` 파일 mtime 을 임계값으로 썼는데 SQLite WAL
  checkpoint / Store::open 의 부작용으로 mtime 이 NOW 로 튀어 외부 편집을 못
  잡는 false negative 가 있었음.
- **BUG-047**: drift / reindex 가 보는 quest 본문 파일은 `{slug}.md` 만. sibling
  `{slug}.comments.md` (DEV-094) / `{slug}.memo.md` (DEV-099) 는 별개 파일 —
  현재는 DB 캐시 안 들어가므로 비교 대상 아님 (`repo::fs::list_quest_body_files`).
  DEV-102 (댓글/메모 DB 백업) 후엔 별도 `quest_comments` / `quest_memos` 테이블
  ↔ sibling 파일 비교 추가 예정.

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
  - 수동 `openguild backup new`
  - `openguild-server host` (서버) 시작 시 + 1시간 주기
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

## 구현 단계 — 완료 현황 (2026-05-16)

| 단계 | 작업 | 상태 |
|---|---|---|
| **F1** | `core::repo` 모듈 — 파일 read/write + frontmatter parser/serializer | ✅ |
| **F2** | quest / type / status 시드 파일 포맷 정의 + 시드 데이터 | ✅ |
| **F3** | `core::ops::*` (orchestration) — 파일 IO + index UPDATE + journal INSERT | ✅ |
| **F3 consumer** | server routes + CLI Backend 가 `ops::*` 호출 | ✅ |
| **F4** | auto 블록 렌더러 + 영향 범위 추적 (parent 변경 시 옛/새 부모 갱신) | ✅ |
| **F5** | `reindex` — 파일들로부터 index.db 재구축 | ✅ |
| **F6** | snapshot / restore CLI 명령 | ✅ |
| **F7** | external 편집 감지 (`core::drift`, `check drift [--resync]`) | ✅ |
| **F8** | lock 파일 메커니즘 | ✅ |
| **F9** | counter 검증 + 자동 보정 (`check counters`) | ✅ |
| **F10** | `migrate-to-files` 명령 | ✅ |
| **F11** | 기존 audit / auto-backup 코드 제거 | ✅ |
| **F12** | `openguild init` 이 `.guild/` 디렉토리 + 시드 + gitignore 생성 | ✅ |
| **F13** | 테스트 갱신 — 각 새 모듈마다 unit tests | ✅ |

**13 / 13 완료** 🎉 — 저장소 설계 모든 단계 구현 완료 (2026-05-16).

운영 시 추가 보호: `openguild check drift --resync` 로 외부 편집 / git pull 후
캐시 동기화 가능. mutation 진행 전 `LockGuard` 로 single-writer 보장.

## 자동 백업 정책 (2026-05-17)

매 mutation 직후 `core::snapshot::maybe_auto_snapshot` 자동 호출 (`core::ops::*` 내장).
정책 (`AutoSnapshotPolicy`):
- `max_ops_since_last` — journal ops 수. 기본 50.
- `max_age_hours` — 마지막 snapshot 으로부터 경과 시간. 기본 24.

**둘 중 하나라도 도달** 시 snapshot 자동 생성 + journal truncate. stderr 에 알림.
사용자별 조정: env `OPENGUILD_AUTO_BACKUP_OPS` / `OPENGUILD_AUTO_BACKUP_HOURS`.

수동 명령 (`openguild` CLI, local mode):
- `openguild backup new` — 즉시 snapshot
- `openguild backup list` / `openguild backup remove <TS>` — 목록 / 삭제
- `openguild restore [--to TS]` — snapshot 으로 복원

원격 / GUI 는 HTTP admin (`POST /api/admin/snapshot`, `GET /api/admin/snapshots`,
`DELETE /api/admin/snapshots/{ts}`, `POST /api/admin/restore`). (server = host 전용.)

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
2. **SQLite 두 역할 분리**:
   - `index.db` = 캐시 (gitignored, 손실되어도 무해, reindex 로 복구).
   - `backups/journal.db` + `snapshots/*.db` = **정식 백업** (gitignored, 시점 복원).
3. **루트 깨끗**. `openguild.guild` + `.guild/` 만.
4. **git 은 사용자 선택사항**. 사용하면 좋고, 안 써도 자체 백업으로 안전.
5. **자동 vs 수동 명확히**. `[counter]` 같은 자동 필드엔 경고. auto 블록엔 명시적 마커.
6. **사람이 읽을 수 있는 포맷**. Markdown + YAML/TOML — IDE / GitHub web / Obsidian 등에서 자연스러움.

## 댓글 / 메모 (DEV-094 / DEV-099 — 현재 file-only)

- **댓글** `.guild/quests/{slug}.comments.md` — HTML 마커 (`<!-- og-comment id=N
  ts=... author=... reply_to=N -->`). git tracked. 현재 DB 캐시 안 들어감.
- **메모** `.guild/quests/{slug}.memo.md` — plain text. gitignored. 현재 DB
  캐시 안 들어감.

### DB 캐시 + snapshot 백업 (DEV-102, 구현 완료)

- migration 0011: `quest_comments` / `quest_memos` 테이블 추가.
- file 진리원 유지 + DB 캐시 sync:
  - `ops/comments.rs` 의 mutation (`add/update/delete_comment_entry`,
    `set_memo`) 모두 file write 후 cache UPSERT.
  - `reindex` 가 sibling 파일들 (`{slug}.comments.md` / `{slug}.memo.md`) 을
    `quest_comments` / `quest_memos` 에 적재 (file mtime → updated_at 근사).
  - `drift::detect_drift` 의 `fresh_siblings` 가 sibling 파일이 캐시보다 새것일
    때 감지 → `auto_resync` 가 reindex.
- snapshot 자동 포함 — `index.db` binary copy 라 cache 테이블 그대로.
- 메모 `user_id`:
  - single-user 단계 = `0` sentinel (모든 row 동일).
  - multi-user (DEV-021 JWT) 진입 시 실제 user_id 격리 활성.
  - "사적" = 다른 사용자에게 안 보임 (multi-user), **백업 안 됨이 아님**.
- 회귀: snapshot 만든 후 cache 행 의도적 wipe → restore → 댓글/메모 살아남음
  (snapshot.rs 의 `snapshot_preserves_comments_and_memos` 테스트).

### 댓글 이모지 반응 — DEV-108 (구현 완료)

- og-comment 마커에 `reactions="👍:alice|bob,✅:carol"` attribute — 콤마로
  구분된 이모지 항목 목록. 각 항목 = `emoji` (legacy) 또는
  `emoji:author1|author2`. 빈 목록이면 attr 생략 (구 파일과 byte 동일).
- **누가 반응했는지** (DEV-108 후속): 항목마다 author 목록을 기록 → GUI 에서
  pill 호버 시 작성자 표시, 내가 단 반응은 강조. toggle 은 `(slug,id,emoji,
  author)` — author 가 이미 있으면 해제, 없으면 추가. 마지막 author 가 빠지면
  항목 제거. 빈 author 는 `(익명)` 으로 기록 (항상 1명 이상 보장).
  - `repo::comments::split_reaction` / `join_reaction` 가 인코딩 담당. 구
    `reactions="👍,✅"` (author 없는 legacy) 도 그대로 파싱 (authors 빈 목록).
- **file-only** — `quest_comments` 캐시에 컬럼 없음. 댓글 read 경로가 file
  직접 (`list_comment_entries`) 이라 무방, 캐시 재구축도 file 재파싱.
- single-user 단계 = author 이름이 곧 작성자. multi-user (DEV-021) 진입 시
  실제 user_id 와 연결로 확장.
- emoji / author 값 제약: `,` `"` `:` `|` 금지 (마커 attr / 인코딩 안전) —
  ops 에서 검증.

### 토론(discussion) 댓글 + 완료 게이트 — DEV-142 (구현 완료)

- og-comment 마커에 `discussion="true"` / `resolved="true"` attr 추가 (true 일
  때만 출력 — 구 파일 byte 호환, reactions 와 동일 방식). `CommentEntry` 에
  `discussion: bool` / `resolved: bool` 필드. **file-only** (DB 캐시 컬럼 없음).
- **완료 게이트**: `ops::quests::change_status` 가 target status 의
  `counts_as_done = true` 면, 해당 quest 의 댓글 파일을 읽어 `discussion &&
  !resolved` 인 댓글이 하나라도 있으면 `BadRequest` 로 전환 차단. CLI(`quest
  move ... done`) / GUI 공통 — core 한 곳에서 강제.
- discussion 을 끄면 resolved 도 함께 해제 (의미 없는 잔여 상태 방지).
  resolved 토글은 discussion 댓글에만 허용.
- "완료" 기준 = admin 에서 설정하는 status.counts_as_done (campaign 완료 판정과
  동일 기준, DEV-093). 즉 토론 미해결이면 캠페인 진행률에도 안 잡힘.
- UI: QuestCommentsSection 의 댓글 head 에 `💬 토론` 토글 + 미해결/해결 배지
  (quest scope 한정 — campaign 은 change_status 게이트가 없어 미노출).

### 캠페인 댓글 / 메모 — DEV-100 (구현 완료)

- quest 와 동일 entry 포맷 / 동일 기능 (답글 / 반응 / 번호):
  - `.guild/campaigns/{slug}.comments.md` — git tracked.
  - `.guild/campaigns/{slug}.memo.md` — gitignored (seed `.gitignore` 에
    `campaigns/*.memo.md` 추가).
- `repo::comments` 의 path 기반 generic IO (`read/write_entries_at`,
  `read/write_text_at`) 를 quest / campaign 이 공용.
- ops 는 `ops::campaign_comments` — journal append 포함.
- **DB 캐시 없음** (quest 의 DEV-102 와 달리) — snapshot 백업 합류는 후속
  quest 후보. 댓글 파일은 git tracked 라 손실 위험 낮음, 메모는 gitignored
  + 캐시 없음 = 백업 사각지대 (quest 메모의 DEV-102 이전과 동일 상태).

### 본문 첨부파일 — DEV-069 (구현 완료)

- 진리원 = `.guild/attachments/{nanos}-{rand}.{ext}` 실제 파일. 본문이
  `![](attachments/foo.png)` (이미지) / `<video src="attachments/x.mp4">`
  / `[name](attachments/y.pdf)` 로 참조. GUI 의 `MarkdownView.rewriteLocalMedia`
  가 `attachments/` 상대 src 를 Tauri asset URL (또는 server `/api/guild-files/`)
  로 재작성해 표시.
- **백업 대상이 아니다 — BUG-188 (admin 결정).** 첨부는 파일만 진리원이고,
  스냅샷에도 index.db 에도 담지 않는다. 보관은 git 또는 사용자 몫.
  - `ops::attachments::save_attachment` = 확장자 정규화 + journal append +
    파일 write. 경로를 아는 데스크탑은 `save_attachment_from_file` 이
    `std::fs::copy` 로 옮긴다(대용량을 메모리에 올리지 않는다).
  - 첨부 **목록**(`{slug}.attachments.json` 사이드카)은 다른 문서와 같이
    백업된다 — 복원하면 "무엇이 붙어 있었는지"는 남고 파일 바이트만 원래
    자리에 있어야 한다.
  - 어드민 > 백업 화면이 이 범위를 명시한다(`admin.backupScopeHint`).
  - <details><summary>왜 폐기했나 (2026-08, migration 0018 → 0029)</summary>

    원래는 `attachment_blobs` 테이블에 파일 바이트를 통째로 넣어 "git 없이
    스냅샷만으로 첨부까지 복원"을 노렸다. 첨부는 **크기 상한이 없는 유일한
    데이터**라 두 군데서 깨졌다:

    1. SQLite blob 상한(약 1GB) — 1.5GB 파일 첨부가 `code 18: string or blob
       too big` 으로 실패(admin 보고). 파일 write 는 이미 끝난 뒤라 참조 없는
       대용량 파일이 남고, 그 뒤로는 reindex 의 self-heal 과 스냅샷이 **계속**
       같은 에러로 실패했다.
    2. 용량 — 첨부 하나가 index.db 와 매 스냅샷을 그 크기만큼 부풀린다.

    임계값(예: 64MB 이상만 제외)도 검토했지만, "백업에 있는 것과 없는 것"이
    크기에 따라 갈리는 상태가 설명하기 어렵다는 판단으로 전면 제외를 택했다.
    </details>
- **업로드 UX** (GUI, Tauri 전용): quest / campaign 상세 편집기 (CodeMirror)
  에 `editor-attach.ts` 의 `attachmentExtension` — 클립보드 이미지 Ctrl+V
  paste + 파일 drag&drop → base64 → `save_attachment` invoke → 반환 rel 경로를
  커서 위치에 마크다운 삽입 (업로드 중 placeholder 표시). 브라우저(server)
  모드 업로드 + 첨부 삭제 UX 는 DEV-097 범위.

- migration 0014: `app_meta(key TEXT PK, value TEXT)` 단순 key-value 테이블.
- `reindex()` 가 transaction commit 직전 `('last_indexed_at', NOW_ISO)` UPSERT.
- `detect_drift` 가 그 값을 SystemTime 으로 파싱해 임계값으로 사용 — file mtime
  > last_indexed_at 이면 fresh. 마커 없거나 빈 값이면 epoch fallback → 첫
  부트스트랩 reindex 강제.
- 이전엔 `index.db` 파일 mtime 을 임계값으로 썼는데 SQLite WAL checkpoint /
  Store::open 의 부작용으로 NOW 로 튀어 외부 편집을 못 잡는 false negative
  발생. ops 의 모든 mutation 경로를 건드리지 않고 reindex 한 곳만 갱신 —
  ops 활동 후 첫 startup 은 false-positive drift 한 번 (idempotent reindex
  추가) 가능하지만 실 영향 없음.
- **DEV-121 Phase 1 이후 역할 축소**: `incremental::sync_on_open` 이 변경
  감지의 주 경로. `drift::auto_resync` 는 신규/삭제 / 다른 테이블 변경 시
  fallback. `app_meta.last_indexed_at` 자체는 그대로 유지 (다른 용도 확장
  가능).

### `quests.cached_mtime` — DEV-121 Phase 1 (구현 완료)

- migration 0015: `ALTER TABLE quests ADD COLUMN cached_mtime INTEGER NOT NULL DEFAULT 0`.
- 단위 = Unix nanoseconds — `SystemTime::duration_since(UNIX_EPOCH).as_nanos()`
  → `i64`. timezone-independent. parse 불필요 — INTEGER 직접 비교.
- `reindex()` 가 INSERT 시 `mtime_unix_nanos(path)` 함께 적재.
- `incremental::sync_changed_quest_files` 가 각 `.guild/quests/{slug}.md` 의
  `stat()` mtime 을 cached_mtime 과 비교 — 변경된 것만 re-parse + UPDATE.
  cached_mtime 도 함께 갱신해 다음 startup 부턴 비교만으로 통과.
- 신규/삭제는 본 경로가 안 처리 — `needs_full_reindex` flag 만 set → 호출자
  (`sync_on_open`) 가 `drift::auto_resync` fallback.
- **Phase 1 scope**: `quests` 테이블만. statuses / types / tags / campaigns /
  sibling 은 양이 적어 (수~수십개) 기존 reindex 경로 유지. Phase 1b 에서 확장
  가능.

#### timezone 안전성

- File mtime 획득: Rust `std::fs::metadata(path)?.modified()?` → `SystemTime`.
  epoch 기준 절대 시각 — local time / TZ / DST / 길드 이동에 무관.
- DB 비교: INTEGER nanos 직접. parse 불필요.
- naive ISO string 은 절대 사용 X — RFC 3339 with offset 만 허용 (다른 컬럼들
  `created_at` / `updated_at` 은 이미 RFC 3339 with offset).

#### 엣지 케이스

- clock skew (시계 변경): backward → 불필요 re-parse 한 번 (harmless),
  forward → 모든 row stale 처럼 → 한 번 풀 re-parse.
- FAT32 2초 정밀도: 같은 초 안 편집은 못 잡음 — admin "Reindex" 우회.
- mtime 보존 복사 (git checkout, rsync -t): 가장 위험. admin Reindex 로 우회
  필요. 추후 `cached_size` 컬럼 추가 검토.

### 상세 진입 lazy refresh — DEV-137 / DEV-121 Phase 2 (구현 완료)

- Phase 1 은 *시동 1회* sync — GUI 를 켜 둔 채 외부 편집하면 재시작/⟲ 전까진
  stale. 이를 보완하는 런타임 안전망.
- `get_quest_by_slug` (상세 read 경로) 맨 앞에서
  `incremental::refresh_quest_if_stale(slug)`:
  - 그 quest 파일 하나만 `stat()` → file mtime vs `cached_mtime` (~1ms, 내용 안
    읽음).
  - 새것이면 **그 한 행만** re-parse + UPDATE (title / description / status /
    urgency / due / created·updated·deleted + cached_mtime) 후 응답.
  - 같거나 작으면 / 파일 없음 / DB 에 없음 / unknown status → skip (기존 캐시로
    응답, panic 없음).
- **의도적 한계** (Phase 1 과 동일): prereq / parent / tag cascade 는 재계산 안
  함 — 관계 변경은 풀 reindex 영역. 신규/삭제·sibling·비-quest 도 범위 외.

### 파일 진리 ↔ 캐시 신선도 — 정책 정리 (DEV-137 논의 정리)

정책을 평가할 땐 **두 측면을 분리**해야 한다:

**(A) 권한 / 파생 구조 — 항상 성립 (sync 기능과 무관).**
- 모든 mutation(ops)은 파일 + DB **동시 기록** → DB 에만 있고 파일엔 없는
  데이터가 생기지 않음.
- `index.db` 를 통째로 지워도 `reindex` 로 파일에서 **무손실 재구축** → 순수
  파생 캐시.
- 용어 주의: `index.db` = *쿼리/검색 보조 캐시*. **백업은 별개** — `backups/`
  의 `journal.db` + `snapshots/`. 셋 다 파일에서 파생되는 gitignore 대상.
- → "파일이 진리, DB 는 파생물" 은 이미 참. (DEV-137 이 바꾸는 게 아님.)

**(B) 신선도 / 일관성 — "읽기가 외부 편집을 반영하나" — sync 지점 기반
eventually-consistent.**

| sync 지점 | 무엇을 | 언제 |
|---|---|---|
| 시동 sync (DEV-121 P1) | 변경된 quest 파일 + 신규/삭제·관계 또는 campaign 본문·types/statuses/tags 외부편집 시 drift→풀 reindex (DEV-178) | 앱 시작 / Welcome 로 길드 열 때(BUG-049 fix) |
| 상세 lazy (DEV-137 / DEV-178) | 그 quest 한 건 (DEV-137) / 그 campaign 한 건 (DEV-178) | 상세 페이지 진입 |
| 수동 ⟲ (DEV-095) | 전체 (풀 reindex) | 사용자 클릭 |

**현재 sync 지점이 안 덮는 빈틈** (= 외부 편집 후 해당 sync 지점 전까진 stale):

| 읽는 대상 | 시동 sync | 상세 lazy | ⟲ |
|---|---|---|---|
| quest 표시 필드 (title/status/…) | ✅ | ✅ | ✅ |
| quest 관계 (prereq/parent/tag) | △ full fallback 시 | ❌ | ✅ |
| 목록 / 보드 (여러 파일) | ✅ (시동) | ❌ | ✅ |
| campaign 본문 (title/desc/status/체크리스트/링크) | ✅ drift (DEV-178) | ✅ (DEV-178) | ✅ |
| types / statuses / tags 정의 | ✅ drift (DEV-178) | ❌ 목록이라 lazy 없음 | ✅ |
| 댓글·메모 (sibling) | ✅ drift (BUG-068) | ❌ | ✅ |
| rules / templates | 파일 직독 — 항상 즉시 | 항상 즉시 | 항상 즉시 |
| 신규 / 삭제 파일 | ✅ full fallback | ❌ | ✅ |

> DEV-178: campaign 본문·메타 정의는 per-row `cached_mtime` 컬럼이 없어 범용
> `file_mtime_cache`(BUG-068)로 비교. drift 는 "캐시에 있고 파일이 더 새것"일
> 때만 fresh (시드만 된 메타를 fresh 로 보면 오탐) — 모든 mutation ops 가 파일
> write 직후 `file_mtime::touch` 로 캐시를 갱신해 오탐을 막는다. 신규 campaign
> 파일 적재 자체는 reindex/⟲ 영역.

**결론**:
- "DB 는 파일에서 파생된 캐시/인덱스" (A) = 이미 완전 부합.
- "읽으면 항상 파일이 최신" (B) = quest·campaign 상세는 lazy 로 부합, 메타·댓글은
  시동 drift 로 재시작 시 반영, rules/templates 는 파일 직독이라 항상 즉시.
  목록/관계는 여전히 ⟲ 또는 재시작 필요.
- **완전 실시간 파일-진리**(어디서 읽어도 즉시 반영)는 **fs watcher**(`notify`,
  DEV-122 옵션 C) 또는 모든 read 에 lazy(목록은 N-file stat 비용) 가 필요 —
  비용 대비 가치로 **의도적 보류**. 현 모델 = "권한은 항상 파일, 신선도는 sync
  지점 기반 eventually-consistent".

---

## ⚠️ 구현 완료 후 — dogfood 전환 (필수)

본 설계가 완료되면 openguild 자체 프로젝트의 **앞으로 할 일 관리는 openguild 로**.
별도 todo 도구 / GitHub Issues 보조 사용 안 함.

전환 절차:
1. 새 저장소 모델로 마이그레이션 (`openguild migrate-to-files`) — 현 19 quests 이전.
2. 새 작업 / 버그 / 개선은 `openguild quest new --type ...` 로 추가.
3. `AGENTS.md` 의 "절대 규칙" 섹션이 이미 이 사실을 명시.
4. `docs/dev-plan.md` 의 향후 로드맵 항목들도 점진적으로 quest 로 이전 (대단위는 docs 유지, 세부는 quest).

이는 openguild 의 단순한 self-test 가 아니라 **본 도구의 차별점을 직접 체험** 하기 위한 의도적
선택. git native + 파일 기반 issue tracker 라는 가치를 매일 사용하며 검증.
