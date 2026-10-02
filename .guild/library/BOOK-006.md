+++
book_id = "BOOK-006"
title = "저장소 — 파일 · 캐시 · journal · 스냅샷 · 잠금"
path = "아키텍처/상세"
created_at = "2026-09-23T11:49:54+09:00"
updated_at = "2026-10-02T13:06:15+09:00"
deleted = false
version = 2
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

**문서 번호**(DEV-433). 퀘스트 · 캠페인 · 도서관 문서 · 규칙 파일은 frontmatter 에 `version = N`, 퀘스트 ·
캠페인 댓글 파일은 첫 줄 `<!-- og-comments version="N" -->` — openguild 가 그 파일을 몇 번 고쳤나. 본문과
댓글은 **따로** 센다(댓글을 달 때 본문 파일을 다시 쓰지 않으려고). 쓸 때 내용이 그대로면 쓰지 않고 번호도
그대로다. 이벤트에 실려 늦게 온 알림을 거르는 데 쓴다 — "바뀌었나" 판단에는 안 쓴다(밖에서 고치면 안 맞는다).
번호는 글자 수준에서만 다룬다(`repo/version.rs`) — 파일 모델은 번호를 몰라서 옛 openguild 도 새 파일을 읽는다.
타입을 바꿔 이름이 바뀌면(DEV-1 → BUG-1) 파일을 옮긴 뒤 써서 번호가 이어진다.

**같은 본문 동시 편집**(DEV-435, 위키백과 방식). 잠금은 "읽기와 쓰기 사이 끼어들기" 만 막는다 — 편집기를 몇 분
열어 둔 동안 남이 저장한 것은 못 막는다. 그래서 본문 저장(퀘스트 · 도서관 · 규칙 본문, 댓글 수정)은 **편집을 시작할
때 본 것**을 함께 받는다.

| 받는 것 | 누가 | 다르면 |
|---|---|---|
| 시작할 때 본 본문(`base_description` · `base_body` · `base_content`) | 앱 · 웹 | 3방향 병합(`merge.rs`, 줄 단위) — 겹치지 않으면 합쳐 저장, 같은 줄이면 `AppError::EditConflict`(구간 목록) |
| 시작할 때 본 파일 번호(`base_version`, DEV-433) | CLI · 에이전트(`--base-version`) | 합치지 않고 거부 — 지금 내용을 돌려준다 |
| 편집하던 퀘스트 번호(`expected_id`) | 앱 | 타입이 바뀌었으면(`renamed`, 새 번호) · 지워졌으면(`deleted`) 거부 |

병합은 잠금 안에서 한다(그 순간의 최신이 "지금" 이다). journal 에는 합친 결과를 적는다. 충돌 표시를 글자에 박지
않는다. 서버는 409 와 충돌 내용을, 앱은 `EDIT_CONFLICT::` 꼬리표 + JSON 을 돌려준다. 단일값(제목 · 긴급도 · 기한 ·
상태)은 나중 값이 남는다.

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
변경만 쌓인다. 비우는 것은 **스냅샷이 길드를 읽은 시점의 마지막 op 까지**다(BUG-340) — 스냅샷 파일을
쓰는 동안 들어온 변경은 남는다. `restore --at` 이 스냅샷 위에 journal 을 다시 돌려 시점 복원을 한다(`replay.rs`).

이벤트와 겸하지 않는다. journal 은 실패할 시도도 적고, 결과(새로 붙은 번호 등)가 없고, 복원 중에
재생되기 때문이다 — 이유는 `core/src/events/mod.rs` 맨 위.

## 스냅샷 — `backups/snapshots/{ts}.db`

**스냅샷 하나 = SQLite 파일 하나**다. 안에 든 것은 진리원 **파일의 내용**이다(캐시가 아니다).
예전엔 소스 트리를 파일 단위로 복사했는데, 파일이 1,100개를 넘으면서 느려지고 큰 클러스터 볼륨에서
용량이 수백 배로 부풀어 바꿨다(DEV-306).

| | |
|---|---|
| 담는 것 | 루트 마커 + `quests` · `campaigns` · `rules` · `tags` · `types` · `statuses` · `history` · `library` · `worklog` · `templates` · `plugins` — 정본은 `snapshot.rs` 의 `SOURCE_SUBDIRS`. 무엇을 담았는지 스냅샷의 `meta.subdirs` 에 적는다 |
| **안 담는 것** | `attachments` — **일부러 뺐다**(BUG-188). 크기 상한이 없는 유일한 자료라 스냅샷이 수 GB 가 되고 SQLite blob 상한에 걸린다. 보관은 git 이나 사용자 몫이고, 백업 화면이 그렇게 밝힌다 |
| 자동 | 변경 50번 또는 24시간 — `OPENGUILD_AUTO_BACKUP_OPS` / `_HOURS`. 앱·서버는 뒤에서 뜨고 CLI 는 그 자리에서 뜬다(DEV-299). 변경 함수가 **잠금을 푼 뒤** 부른다 — 쥔 채 부르면 제 잠금을 기다린다 |
| 잠금 | **두 단계**(BUG-340) — 길드 **독점**으로 파일 내용을 메모리로 읽고 journal 위치를 적은 뒤 푼다(이 길드 기준 0.02초). 스냅샷 파일 쓰기는 잠금 밖이라 그동안에도 길드를 고칠 수 있다. 이름은 빈 파일을 `create_new` 로 먼저 만들어 잡는다 |
| 보관 | 7개 |
| 복원 | 지금 것을 `.pre-restore/` 로 옮겨 두고 → **그 스냅샷이 담은 폴더만** 지우고 내용을 되붙인다 → `reindex`. 처음부터 끝까지 길드 **독점** |

**복원은 스냅샷이 담은 폴더만 지운다**(BUG-337). 목록에 폴더를 더하면 그 전에 뜬 스냅샷에는 그
폴더가 없다 — 그걸로 복원하면서 목록 전체를 지우면 지금 것이 사라지고 되붙일 것이 없다.
`meta.subdirs` 가 없는 옛 DB 스냅샷은 그때의 아홉 폴더(`LEGACY_DB_SUBDIRS`)만 담은 것으로 본다.
**목록에 폴더를 더할 때 `LEGACY_DB_SUBDIRS` 는 건드리지 않는다.**

## 잠금 — `.guild/.lock` + `.guild/.locks/`

**프로세스 안과 프로세스 사이를 함께 막는다**(BUG-287). 앱과 에이전트의 CLI 가 같은 퀘스트를
동시에 고치면 먼저 쓴 쪽이 조용히 지워졌기 때문이다(태그 12개 중 1개 생존 — 전부 종료 코드 0).

**두 겹이다**(DEV-430). 길드 잠금은 **공유 / 독점** 둘이고, 그 밑에 **문서 잠금**이 있다.

| 무엇을 고치나 | 잡는 것 | 예 |
|---|---|---|
| 문서 몇 개 | 길드 **공유** + 그 문서들 **독점** (`Store::lock_docs`) | 댓글 · 메모 · 본문 · 상태 · 태그 · 부모 · 선행 · 첨부 · 규칙 · 도서관 문서 · 일지 |
| 길드 전체 | 길드 **독점** (`Store::lock_guild`) | 타입 · 상태 · 태그 정의, 타입 바꾸기(번호가 바뀐다), 규칙 이름 바꾸기, 도서관 폴더 옮기기 · 지우기, 번호 맞추기, 백업 읽기 · 복원, 다시 색인하기 · 여는 순간의 따라잡기(이 길드 기준 0.4초) |

서로 다른 문서를 고치는 변경끼리는 기다리지 않는다. 같은 문서면 줄 선다.

| 층 | 무엇 |
|---|---|
| 프로세스 안 | 길드는 tokio `RwLock`, 문서마다 tokio `Mutex` — 서버의 동시 요청이 blocking 스레드를 붙잡지 않게 먼저 async 로 기다린다 |
| 프로세스 사이 | `.guild/.lock` 에 OS 권고 잠금(공유는 `try_lock_shared`, 독점은 `try_lock`), 문서는 `.guild/.locks/{종류}~{id}` 에 독점. `.locks/` 는 안의 `.gitignore`(`*`)로 폴더째 git 에서 빠진다 |

**문서 열쇠.** 사람이 보는 한 덩이가 한 문서다 — 퀘스트 DEV-1 은 본문 · 댓글 · 메모 · 이력 · 첨부 목록을
한 문서로 본다. `quest:DEV-001` · `campaign:C-001` · `book:BOOK-001` · `rule:{slug}` · `worklog:{날짜}`, 그리고
길드에 하나뿐인 것 — 번호(`counter:DEV`, `counter:campaign`, `counter:book`), 보드 위치(`positions`), 도서관
폴더 목록(`library-folders`), 옛 규칙 파일(`rules`), 부모 · 선행 관계(`quest-tree`). 관계를 바꾸는 변경은 모두
`quest-tree` 를 함께 잡는다 — 각자 제 문서만 잠그면, 따로는 괜찮은 두 변경이 합쳐 고리를 만드는 것을 못 막는다.

**잡는 순서를 부르는 쪽이 못 고른다.** 문서 목록은 한 번에 넘기고 순서(이름 순)는 `lock.rs` 가 정한다. 쥔
채 또 잠그면 오류다 — "하나 잡고 나중에 하나 더" 가 엇갈림의 유일한 길이다.

**번호만 받는 변경**(퀘스트 행 id)은 잠그기 전에 어느 문서인지 읽어야 한다 — `ops::lock_resolved` 가 알아내
잠근 뒤 **다시 알아내** 같은지 본다. 다르면 풀고 다시, 네 번을 넘기면 길드 독점으로 간다.

**캐시 DB 쓰기 트랜잭션은 `BEGIN IMMEDIATE`**(`db::begin_write`). 문서 잠금이면 서로 다른 문서의 변경이
캐시 DB 에 동시에 쓴다 — 보통 `BEGIN` 은 읽다가 쓸 때 남이 먼저 썼으면 기다리지 않고 바로 실패한다.

**플러그인의 "기다리는" 줄은 잠금이 풀린 뒤에 돈다**(BUG-339) — 모은 작업(tokio task)별로 두어, 먼저 끝난
변경이 아직 잠금을 쥔 남의 줄을 돌리지 않는다. **자동 백업**도 잠금이 풀린 뒤다(BUG-340).

프로세스가 잠금을 쥔 채 죽어도 OS 가 기술자와 함께 푼다 — 예전 PID 파일처럼 "살아 있나" 를
추측하지 않는다. 60초를 못 얻으면 포기하고 이유를 말한다. 잠금을 지원하지 않는 파일시스템(일부
네트워크 공유)에서는 경고를 남기고 프로세스 안 보호만으로 간다.

**재진입이 안 된다.** 공개 변경 함수가 다른 공개 변경 함수를 부르면 오류다. 공유하는 몸통은 잠금 없는
내부 함수로 뺀다. `ops::lock_coverage` 시험이 함수마다 **길드 독점 / 문서 잠금** 분류를 적게 하고, 적은
대로 잠그는지 확인한다.

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
| 잠금 | `.guild/.lock` + `.guild/.locks/` | 같은 문서 동시 쓰기 · 길드 전체 변경 |
| soft delete | frontmatter `deleted = true` | `quest restore` 로 되살린다 |
| CLI `--yes` · `--dry-run` | `cli/` | 삭제는 확인 필수, 바꾸기 전에 미리보기 |
