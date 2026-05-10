-- Soft delete 도입.
-- quests 에 deleted_at 컬럼 추가. NULL 이면 살아있음, NOT NULL 이면 삭제됨.
--
-- 모든 quest SELECT 는 deleted_at IS NULL 필터. 백엔드 코드에서 보장.
-- DELETE 동작은 hard delete 대신 deleted_at = datetime('now') 로 변경.
-- restore 동작 (PATCH /api/quests/:id/restore) 으로 deleted_at = NULL 복구.

ALTER TABLE quests ADD COLUMN deleted_at TEXT;

-- 조회 성능: deleted_at 인덱스 (살아있는 행만 빠르게)
CREATE INDEX idx_quests_alive ON quests(id) WHERE deleted_at IS NULL;
