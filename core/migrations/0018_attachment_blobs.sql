-- DEV-069: 첨부파일 blob 캐시 — git 없이도 snapshot 으로 복원 (사용자 댓글 #3).
--
-- 파일 (`.guild/attachments/**`) 이 진리원. 본 테이블은 백업용 캐시:
-- - ops 의 첨부 저장 시 파일 write + blob UPSERT.
-- - reindex 가 양방향 self-heal: 새/변경 파일 → blob 갱신, blob 만 있고
--   파일이 사라짐 → 파일 복원.
-- - snapshot (index.db binary copy) 에 자동 포함.
--
-- mtime 은 Unix nanoseconds (DEV-121 의 cached_mtime 과 동일 규약) —
-- 변경 감지용. rel_path 는 `.guild/` 상대 (예 'attachments/123-ab.png').
CREATE TABLE attachment_blobs (
    rel_path  TEXT PRIMARY KEY,
    bytes     BLOB    NOT NULL,
    mtime     INTEGER NOT NULL DEFAULT 0
);
