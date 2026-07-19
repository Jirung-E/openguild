+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-06-23T01:30:44+09:00"
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

## 3. 백업 ≠ 캐시

- 캐시: `index.db` (gitignore, 재생성 가능, 손실 무해).
- 백업: `backups/journal.db`(AOF) + `snapshots/*.db`(RDB).
- `index.db` 를 백업처럼 의존 금지. 첨부 blob 같은 "파일 백업" 도 snapshot 합류로 (DEV-069).

## 4. 읽기는 eventually-consistent — 신선도는 sync 지점으로

외부 편집(에디터 / CLI / git pull) 반영은 명시적 sync 지점으로만:

| sync 지점 | 무엇을 | 언제 |
|---|---|---|
| 시동 sync (DEV-121) | 변경된 quest 파일 (cheap) + 신규/삭제·관계 또는 캠페인 본문·types/statuses/tags 외부편집 시 drift→풀 reindex (DEV-178) | 앱 시작 / Welcome 로 길드 열 때 |
| 상세 lazy (DEV-137 / DEV-178) | 그 quest 한 건 (DEV-137) / 그 campaign 한 건 (DEV-178) | 상세 진입 |
| 수동 ⟲ (DEV-095) | 전체 | 사용자 클릭 |

DB 캐시로 읽는 엔티티의 외부편집 커버리지:

| 엔티티 | 읽기 | 외부편집 반영 |
|---|---|---|
| quest 본문 | DB 캐시 | 상세 lazy(per-row cached_mtime) + 시동 drift |
| campaign 본문 | DB 캐시 | 상세 lazy + 시동 drift (DEV-178, file_mtime_cache) |
| types / statuses / tags 정의 | DB 캐시 | 시동 drift (DEV-178, file_mtime_cache) — 목록이라 lazy 없음 |
| 댓글 / 메모 (sibling) | DB 캐시 | 시동 drift (BUG-068, file_mtime_cache) |
| rules / templates | **파일 직독** | 항상 즉시 (캐시 없음) |

campaign 본문·메타는 per-row `cached_mtime` 컬럼이 없어 범용 `file_mtime_cache`
(BUG-068)로 비교한다. drift 는 "캐시에 있고 파일이 더 새것"일 때만 fresh —
캐시에 없는 메타(시드만 된 상태)를 fresh 로 보면 오탐(§위 #4 의 last_indexed_at
회귀와 동류). 신규 파일 적재는 reindex 가 담당. 이를 위해 모든 mutation ops 는
파일 write 직후 `file_mtime::touch` 로 캐시를 갱신한다(오탐 방지).

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

- `docs/storage-design.md` § "파일 진리 ↔ 캐시 신선도 정책" — 상세 + 커버리지 표.
- `docs/guild-rules.md` § 저장소 — 동일 규칙 요약.
- DEV-121 / DEV-137 / DEV-095 / DEV-122(fs watcher 보류).
