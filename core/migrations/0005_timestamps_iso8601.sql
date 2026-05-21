-- DEV-041: 기존 행의 타임스탬프를 ISO 8601 UTC 형식으로 정규화.
--
-- 0001 ~ 0004 시기에 `DEFAULT (datetime('now'))` 로 저장된 행들은
-- "2026-05-21 14:00:24" 형식 (UTC, 공백 구분자, TZ 마커 없음).
--
-- 새 코드는 로컬 시각 + offset 으로 기록 ("2026-05-22T13:41:10+09:00").
-- 형식이 섞이면 lexicographic ORDER BY 가 깨지므로 legacy 행을 일괄 변환:
--   "YYYY-MM-DD HH:MM:SS" → "YYYY-MM-DDTHH:MM:SSZ" (UTC 마커).
--
-- 검출 패턴: LENGTH = 19 AND 11번째 문자가 공백.
--   (이미 ISO 형식인 행은 변환 안 함 — idempotent.)

UPDATE quests
SET created_at = REPLACE(created_at, ' ', 'T') || 'Z'
WHERE LENGTH(created_at) = 19 AND SUBSTR(created_at, 11, 1) = ' ';

UPDATE quests
SET updated_at = REPLACE(updated_at, ' ', 'T') || 'Z'
WHERE LENGTH(updated_at) = 19 AND SUBSTR(updated_at, 11, 1) = ' ';

UPDATE quests
SET deleted_at = REPLACE(deleted_at, ' ', 'T') || 'Z'
WHERE deleted_at IS NOT NULL
  AND LENGTH(deleted_at) = 19 AND SUBSTR(deleted_at, 11, 1) = ' ';

UPDATE quest_history
SET ts = REPLACE(ts, ' ', 'T') || 'Z'
WHERE LENGTH(ts) = 19 AND SUBSTR(ts, 11, 1) = ' ';
