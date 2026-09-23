+++
book_id = "BOOK-006"
title = "저장소 — 파일 · 캐시 · journal · 스냅샷 · 잠금"
path = "아키텍처/상세"
created_at = "2026-09-23T11:49:54+09:00"
updated_at = "2026-09-23T11:50:12+09:00"
deleted = false
+++

# 저장소 — 파일 · 캐시 · journal · 스냅샷 · 잠금

> 지금 모습이다. 전체 그림은 [[BOOK-003]](개괄), 캐시가 파일에 종속되는 규칙은
> [[BOOK-001]](index.db 불변식).

## 진리원 — `.guild/` 파일

형식을 여기 옮겨 적지 않는다. 형식은 `core/src/repo/` 의 각 파일 맨 위 주석이 정본이다.

| 자료 | 위치 | 형식을 쥔 곳 |
|---|---|---|
| 길드 마커 | `{name}.guild` (길드 루트) | `guild_file.rs` |
| 퀘스트 | `quests/{slug}.md` — TOML `+++` frontmatter + 본문 + 자동 블록 | `repo/quest.rs` · `repo/auto.rs` |
| 댓글 | `quests/{slug}.comments.md` — 항목마다 HTML 마커 | `repo/comments.rs` |
| 메모 | `quests/{slug}.memo.md` — **gitignored**, 개인 것 | `repo/comments.rs` |
| 변경 이력 | `history/{slug}.jsonl` — 퀘스트·캠페인 사이드카 | `repo/history.rs` |
| 캠페인 | `campaigns/{slug}.md` (+ 댓글·메모 같은 모양) — 본문의 GFM 체크박스가 체크리스트 | `repo/campaign.rs` |
| 타입 · 상태 · 태그 | `types/{prefix}.toml` · `statuses/{order}-{slug}.toml` · `tags/{slug}.toml` | `repo/type_def.rs` · `status_def.rs` · `tag_def.rs` |
| 템플릿 | `templates/{name}.md` | `repo/template.rs` |
| 규칙 | `rules/{slug}.md` | `repo/rules.rs` |
| 도서관 | `library/{BOOK-NNN}.md` · 폴더는 `library/folders.toml` · 번호는 `library/.counter.toml` | `repo/library.rs` |
| 첨부 | `attachments/**` + 문서 옆 `{id}.attachments.json` | `ops/attachments.rs` |
| 작업 기록 | `worklog/{YYYY-MM-DD}.md` | `ops/worklog.rs` |
| 플러그인 정의 | `plugins/{name}/` — `plugin.toml` + 스크립트 | `plugins/mod.rs` |
| 보드 위치 | `positions.json` — **gitignored**, 개인 UI 상태 | `repo/positions.rs` |

**번호는 되쓰지 않는다.** 타입의 카운터(`types/{prefix}.toml` 의 `[counter]`)와 도서관의
`.counter.toml` 은 단조 증가다. 지운 번호도 다시 안 준다 — 삭제는 frontmatter 의
`deleted = true`(soft delete)이고 파일은 제자리에 남는다. 그래야 git diff 가 깨끗하고 링크가 안 깨진다.

## 캐시 — `index.db`

파일의 투영이다. 목록·검색·관계 검증을 빠르게 하려고 있다. 지우면 `reindex` 가 파일에서 다시
만든다. 테이블 구성은 `core/migrations/` 가 정본이다(지금 30개).

**언제 맞추나:**

| 상황 | 무엇이 |
|---|---|
| `ops` 로 바꿀 때 | 그 자리에서 — 흐름의 4단계 |
| 앱이 길드를 열 때 | `incremental::sync_on_open` — mtime 이 바뀐 파일만 다시 읽는다. 타입·상태·태그 정의가 바뀌었으면 해석 자체가 달라지므로 풀 `reindex` 로 넘긴다 |
| CLI · 서버 시작 | **안 맞춘다.** 밖에서 파일을 고쳤다면 `openguild check drift --resync` 또는 `reindex` |
| `git pull` · 브랜치 전환 · 복원 뒤 | `reindex` (복원은 스스로 한다) |

`drift` 는 파일과 캐시가 어긋났는지 **알려 주는** 쪽이고, `health` 는 파일 자체가 이상한지
(정의되지 않은 상태 등) 읽기만 하는 쪽이다.

## journal — `backups/journal.db`

변경의 **의도**를 먼저 적는 AOF 다(`ops` 흐름의 3단계). 스냅샷을 뜰 때마다 비운다 — 그 뒤의
변경만 쌓인다. `restore --at` 이 스냅샷 위에 journal 을 다시 돌려 시점 복원을 한다(`replay.rs`).

이벤트와 겸하지 않는다. journal 은 실패할 시도도 적고, 결과(새로 붙은 번호 등)가 없고, 복원 중에
재생되기 때문이다 — 이유는 `core/src/events/mod.rs` 맨 위.

## 스냅샷 — `backups/snapshots/{ts}.db`

**스냅샷 하나 = SQLite 파일 하나**다. 안에 든 것은 진리원 **파일의 내용**이다(캐시가 아니다).
예전엔 소스 트리를 파일 단위로 복사했는데, 파일이 1,100개를 넘으면서 느려지고 큰 클러스터 볼륨에서
용량이 수백 배로 부풀어 바꿨다(DEV-306).

| | |
|---|---|
| 담는 것 | 루트 마커 + `quests` · `campaigns` · `rules` · `tags` · `types` · `statuses` · `history` · `library` · `worklog` — 정본은 `snapshot.rs` 의 `SOURCE_SUBDIRS` |
| **안 담는 것** | `attachments` — **일부러 뺐다**(BUG-188). 크기 상한이 없는 유일한 자료라 스냅샷이 수 GB 가 되고 SQLite blob 상한에 걸린다. 보관은 git 이나 사용자 몫이고, 백업 화면이 그렇게 밝힌다 |
| 목록에 없는 것 | `templates` · `plugins` — `SOURCE_SUBDIRS` 에 없어서 스냅샷에 안 들어가고 복원 때도 안 돌아온다. 의도라고 적힌 곳은 없다(2026-09-23 확인) |
| 자동 | 변경 50번 또는 24시간 — `OPENGUILD_AUTO_BACKUP_OPS` / `_HOURS`. 앱·서버는 뒤에서 뜨고 CLI 는 그 자리에서 뜬다(DEV-299) |
| 보관 | 7개 |
| 복원 | 지금 것을 `.pre-restore/` 로 옮겨 두고 → 스냅샷 내용을 `.guild/` 로 → `reindex` |

## 잠금 — `.guild/.lock`

**프로세스 안과 프로세스 사이를 함께 막는다**(BUG-287). 앱과 에이전트의 CLI 가 같은 퀘스트를
동시에 고치면 먼저 쓴 쪽이 조용히 지워졌기 때문이다(태그 12개 중 1개 생존 — 전부 종료 코드 0).

| 층 | 무엇 |
|---|---|
| 프로세스 안 | `Store` 의 tokio Mutex — 서버의 동시 요청이 blocking 스레드를 붙잡지 않게 먼저 async 로 기다린다 |
| 프로세스 사이 | `.guild/.lock` 에 OS 권고 잠금(`File::try_lock` — unix `flock`, Windows `LockFileEx`) |

프로세스가 잠금을 쥔 채 죽어도 OS 가 기술자와 함께 푼다 — 예전 PID 파일처럼 "살아 있나" 를
추측하지 않는다. 60초를 못 얻으면 포기하고 이유를 말한다. 잠금을 지원하지 않는 파일시스템(일부
네트워크 공유)에서는 경고를 남기고 프로세스 안 보호만으로 간다.

**재진입이 안 된다.** 공개 변경 함수가 다른 공개 변경 함수를 부르면 제자리에서 멈춘다. 공유하는
몸통은 잠금 없는 내부 함수로 뺀다 — `ops::lock_coverage` 시험이 분류를 강제한다.

## 무결성 규칙

- 사이클 금지 — 부모 변경 · 선행 추가 때 `index.db` 로 BFS 검증
- 하위 ↔ 선행 상호 배제, 직계 부모는 선행 후보에서 제외
- 미해결 토론 댓글이 있으면 그 퀘스트는 완료로 못 간다
- 타입 카운터 무결성 — `openguild check counters [--fix]`

## 안전장치 한눈에

| 장치 | 어디 | 효과 |
|---|---|---|
| journal | `ops` 흐름 3단계 | 모든 변경의 의도가 남는다 |
| 자동 스냅샷 | `ops` 끝 | 50번 / 24시간 |
| 수동 스냅샷 · 복원 | `openguild backup new` · `restore [--at]`, 앱 관리 화면 | 복원 전 상태는 `.pre-restore/` |
| drift | `openguild check drift [--resync]`, 앱 관리 화면 | 파일 ↔ 캐시 어긋남 |
| 잠금 | `.guild/.lock` | 프로세스 사이 동시 쓰기 |
| soft delete | frontmatter `deleted = true` | `quest restore` 로 되살린다 |
| CLI `--yes` · `--dry-run` | `cli/` | 삭제는 확인 필수, 바꾸기 전에 미리보기 |
