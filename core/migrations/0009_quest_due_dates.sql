-- DEV-076: 퀘스트 희망 / 필수 기한.
--
-- desired_due  — 희망 기한 (사용자가 원하는 마감). YYYY-MM-DD.
-- required_due — 필수 기한 (절대 마감). YYYY-MM-DD.
--
-- 둘 다 nullable. Home 의 "마감 임박 / 지남" 섹션은 required_due 기준.
-- desired_due 는 정보성 (Quest Detail 표시용).
--
-- 기존 row 는 NULL — backfill 불필요 (파일에 필드 추가하면 reindex 시 채워짐).

ALTER TABLE quests ADD COLUMN desired_due  TEXT;
ALTER TABLE quests ADD COLUMN required_due TEXT;

-- "마감 임박" / "Overdue" Home 섹션 필터 가속 (required_due IS NOT NULL + 정렬).
-- partial index — NULL 인 row 는 인덱스에서 제외 (대부분 row).
CREATE INDEX IF NOT EXISTS idx_quests_required_due
    ON quests(required_due)
    WHERE required_due IS NOT NULL AND deleted_at IS NULL;
