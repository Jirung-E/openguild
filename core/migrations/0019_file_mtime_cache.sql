-- BUG-068: sibling 파일(.comments.md / .memo.md) per-file mtime 캐시.
--
-- BUG-067 은 quest 본문을 per-row cached_mtime 으로 비교해 drift 오탐 제거.
-- sibling 은 per-row mtime 컬럼이 없어 detect_drift 가 global last_indexed_at
-- 으로 비교 → ops 가 댓글/메모를 써도(파일 mtime > last_indexed_at) fresh 로
-- 오탐. 본 테이블이 "각 sibling 파일이 캐시에 반영된 시점의 mtime" 을 보관.
--
-- rel_path = `.guild/` 상대 (예 'quests/DEV-001.comments.md',
-- 'campaigns/C-001.memo.md'). mtime = Unix nanoseconds (cached_mtime 규약).
-- reindex / ops 의 sibling write / detect_drift 가 함께 사용.
CREATE TABLE file_mtime_cache (
    rel_path  TEXT PRIMARY KEY,
    mtime     INTEGER NOT NULL DEFAULT 0
);
