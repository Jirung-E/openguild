+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-11-10T02:27:11+09:00"
version = 8
+++
# 파일 진리 / DB 캐시 (저장소 불변 규칙)

`.guild/**` 의 파일이 진리원, `index.db` 는 파생 캐시 — 이 불변을 깨는 코드를 넣지 말 것.

## 1. 파일이 진리, DB 는 파생 캐시

- `.guild/` 의 `.md` / `.toml` 이 source of truth.
- `index.db` 는 언제든 `reindex` 로 파일에서 **무손실 재구축** 가능해야 함.
- → **파일에서 파생되지 않는 값을 DB 에만 저장 금지** (DB-only 권위 상태 도입 금지).

## 2. mutation 은 파일 + DB 동시 기록

ops 경로(journal append → SQL → 파일 atomic write → auto-block 재생성)를 거친다.
한쪽만 바꾸면 drift 발생.

```
// ❌ DB 만 UPDATE 하고 파일 안 씀 → reindex 하면 사라짐
// ✅ ops::quests::update_quest 처럼 파일 + DB 같이
```

**파일 쓰기가 실패하면 캐시를 파일로 되돌린다 (BUG-395).** SQL 을 먼저 바꾸고 파일을 쓰는 경로는 파일 쓰기가
실패하면 캐시만 바뀐 채 남는다 — 목록은 새 값, 파일은 옛 값이고 파일 시각이 그대로라 `check drift` 도 못 잡는다.

- 퀘스트 · 캠페인 파일은 `write_quest_file` · `write_campaign_file` 로만 쓴다. 실패하면 그 안에서
  `restore_cache_after_failed_write` 가 그 문서를 파일에서 다시 읽는다(파일이 없으면 = 새로 만들다 실패, 행을 지운다).
  퀘스트는 모든 행을 "파일을 다시 읽을 것"(`cached_mtime` 0)으로 표시하고 **그 자리에서** 퀘스트 파일 전체를 다시 옮긴다 —
  같은 작업이 먼저 바꾼 다른 행도 맞추고, 실패 직후의 `check drift` 도 깨끗하다.
- 되돌리려면 파일을 **읽을 수** 있어야 한다. 그래서 쓸 수 없는 문서(읽기 전용 · 권한 없음 · 윈도우에서 다른 프로그램이
  독점으로 잡음)는 `lock_docs` 가 잠근 직후, 캐시를 바꾸기 **전에** 거절한다(`ensure_docs_writable` — 쓰기로 열어만 본다).
  여기를 지나 실패하는 것은 파일은 읽히는데 쓰기가 안 되는 경우(폴더가 읽기 전용, 디스크 가득 참)뿐이다.
- 기록(history · 사이드카)은 파일을 쓴 **뒤**에 남긴다 — 실패한 변경의 기록 줄이 남지 않게.
- 파일을 먼저 쓰고 캐시를 고치는 경로(도서관 · 체크리스트 · 댓글)는 이 문제가 없다. 새 경로는 이 순서를 고른다.
- 실패는 `AppError::save_failed(문서 번호, e)` 로 돌려준다 — 앱 · CLI 는 짧은 원인, 서버는 오류 번호([[code-conventions]]).

## 3. 백업 ≠ 캐시

- 캐시: `index.db` (gitignore, 재생성 가능, 손실 무해).
- 백업: `backups/journal.db`(AOF) + `snapshots/*.db`(RDB).
- `index.db` 를 백업처럼 의존 금지 — 언제든 파일에서 재생성되는 파생 캐시다.
- **첨부파일(`.guild/attachments`)은 백업 대상이 아니다** (BUG-188). 예전엔 첨부
  바이트를 index.db 의 blob 으로 넣어 snapshot 에 합류시켰는데(DEV-069), 크기
  상한이 없는 데이터라 1GB 를 넘는 파일 하나로 **첨부 저장·재색인·백업이 모두**
  실패했다. 지금은 파일만 두고 보관은 git/사용자 몫 — 어드민 백업 화면이 이
  범위를 밝힌다. "큰 바이너리를 캐시에 욱여넣어 백업을 대신하게 하지 말 것".

## 4. 읽기는 eventually-consistent — 신선도는 sync 지점으로

외부 편집(에디터 / CLI / git pull) 반영은 명시적 sync 지점으로만:

| sync 지점 | 무엇을 | 언제 |
|---|---|---|
| 시동 sync (DEV-121) | 변경된 quest 파일 (cheap) + 신규/삭제·관계 또는 캠페인 본문·types/statuses/tags 외부편집 시 drift→풀 reindex (DEV-178) | 앱 시작 / Welcome 로 길드 열 때 |
| 상세 lazy (DEV-137 / DEV-178) | 그 quest 한 건 (DEV-137) / 그 campaign 한 건 (DEV-178) | 상세 진입 |
| 수동 ⟲ (DEV-095) | 전체 | 사용자 클릭 |
| CLI 명령마다 (BUG-389) | 바뀐 파일만. 바뀐 게 없으면 `stat` 뿐 (1,300개 파일에 약 7ms) | 명령 시작 (`reindex` · `index` · `check` · `restore` 는 빼고) |
| 서버 시동 (BUG-389) | 앱 시동과 같다 | `host` 시작 |
| 쓰기 직전 (BUG-389) | 잠근 문서 그 한 건 | `Store::lock_docs` — **모든 문서 쓰기** |

DB 캐시로 읽는 엔티티의 외부편집 커버리지:

| 엔티티 | 읽기 | 외부편집 반영 |
|---|---|---|
| quest 본문 | DB 캐시 | 상세 lazy(per-row cached_mtime) + 시동 drift |
| campaign 본문 | DB 캐시 | 상세 lazy + 시동 drift (DEV-178, file_mtime_cache) |
| types / statuses / tags 정의 | DB 캐시 | 시동 drift (DEV-178, file_mtime_cache) — 목록이라 lazy 없음 |
| 댓글 / 메모 (sibling) | DB 캐시 | 시동 drift (BUG-068, file_mtime_cache) |
| 도서관 본문 | DB 캐시 | 시동 drift (DEV-311, file_mtime_cache) — 쓰기마다 `touch` (BUG-396) |
| rules / templates | **파일 직독** | 항상 즉시 (캐시 없음) |

campaign 본문·메타는 per-row `cached_mtime` 컬럼이 없어 범용 `file_mtime_cache`
(BUG-068)로 비교한다. drift 는 "캐시에 있고 파일이 더 새것"일 때만 fresh —
캐시에 없는 메타(시드만 된 상태)를 fresh 로 보면 오탐(§위 #4 의 last_indexed_at
회귀와 동류). 신규 파일 적재는 reindex 가 담당. 이를 위해 모든 mutation ops 는
파일 write 직후 `file_mtime::touch` 로 캐시를 갱신한다(오탐 방지). 빠뜨리면 우리 쓰기를 바깥 편집으로 보고 다음
동기화가 전체 재색인을 돈다 — 도서관 쓰기가 그랬다(BUG-396).

**쓰기는 eventually-consistent 가 아니다 (BUG-389).** 쓰기는 DB 값으로 파일을 다시 만들기 때문에, 뒤처진 DB 로
쓰면 밖에서 고친 내용을 파일에서 지운다. 그래서:

- 문서를 고치기 전에 그 문서를 쓸 수 있는지 보고(BUG-395) 파일에서 다시 읽는다 — `lock_docs` 가 잠근 뒤 한다. 새 쓰기 경로도 이 잠금을 지날 것.
- 파일을 못 읽으면 **쓰기를 거부**한다. 무시하고 쓰면 그 파일을 덮는다.
- 타입 · 상태 이름 바꾸기처럼 DB 값으로 여러 파일을 다시 쓰는 작업은 길드 독점을 잡은 뒤 `sync_before_guild_write` 를 먼저 부른다.
- 새 번호는 번호 파일(`types/*.toml` · `campaigns/.counter.toml` · `library/.counter.toml`)과 문서 파일에서 정한다 — DB 는 하한일 뿐이다. `index.db` 가 비어 있어도 있는 번호를 다시 주면 안 된다.

**쓰기 경로는 전부 시험한다 (BUG-389).** `server/src/write_matrix_tests.rs` 가 서버의 모든 쓰기 경로를 밖에서 고친 · 깨뜨린 · 지운 파일 위에서 돌린다. 새 경로를 표에 넣지 않으면 시험이 실패한다.

신선도가 필요한 **새 read 경로**를 추가하면 "어느 sync 지점이 이걸 덮나" 를 확인.
목록/보드처럼 여러 파일을 보는 경로는 per-read lazy 가 비싸니 시동 sync / ⟲ 에 의존.

## 5. mtime 비교는 절대 시각

`cached_mtime` = Unix nanoseconds (`SystemTime::duration_since(UNIX_EPOCH)`).
TZ / DST / 길드 이동에 무관. naive ISO string 비교 금지.

## 왜

"파일이 진리, DB 는 캐시" 가 이 프로젝트의 핵심 차별점(git-native, 사람이 읽는 파일).
DB 에 권위 상태가 새거나, 파일/DB 를 따로 갱신하거나, 캐시를 백업으로 착각하면
이 가치가 무너진다. 완전 실시간 파일-진리(fs watcher)는 비용 때문에 보류 중이며,
현재는 권한=항상 파일 / 신선도=sync 지점 기반 eventually-consistent.

## 관련 문서

- [[BOOK-001]] — index.db 불변식과 DB→파일 역류 지도.
- [[BOOK-006]] § "캐시 — `index.db`" — 언제 맞추나.
- DEV-121 / DEV-137 / DEV-095 / DEV-122(fs watcher 보류).
