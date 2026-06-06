-- DEV-093: 각 status 가 "완료" 로 카운트되는지 — 캠페인 진행률 계산용.
--
-- 파일 진리원: `.guild/statuses/{order}-{slug}.toml` 의 `counts_as_done` 필드.
-- 본 컬럼은 캐시. reindex 가 file → DB sync.
--
-- 기본 false. 길드의 `done` / `cancelled` slug 는 자동 backfill true (운영
-- 직관 일치). 다른 slug 는 사용자가 status 정의 화면에서 토글.

ALTER TABLE quest_statuses ADD COLUMN counts_as_done INTEGER NOT NULL DEFAULT 0;

-- backfill: 기존 길드의 done / cancelled slug 자동 true.
UPDATE quest_statuses
   SET counts_as_done = 1
 WHERE slug IN ('done', 'cancelled');
