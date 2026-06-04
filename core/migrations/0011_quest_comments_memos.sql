-- DEV-102: 댓글 / 메모를 index.db 캐시로 → snapshot 자동 백업.
--
-- 파일이 진리원 (`{slug}.comments.md` / `{slug}.memo.md`). 본 테이블은 캐시로,
-- 손실되어도 `reindex` 가 파일에서 재구축. 단 snapshot 백업의 대상이 되어
-- git 없이도 시점 복원 가능.
--
-- 메모의 `user_id`:
-- - single-user 단계 (현 desktop Tauri) = 모든 row 가 user_id=0 sentinel.
-- - multi-user / JWT (DEV-021) 진입 시 실제 user_id 격리.
-- - NULL 안 씀: SQLite 의 PK NULL 동등 비교 특성 (NULL ≠ NULL) 로 같은 quest 에
--   여러 NULL row 가 충돌 없이 들어가 single-user 모드에서 의도치 않은 중복 발생.

CREATE TABLE quest_comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    quest_id     INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    entry_id     INTEGER NOT NULL,
    ts           TEXT    NOT NULL DEFAULT '',
    author       TEXT    NOT NULL DEFAULT '',
    body         TEXT    NOT NULL,
    parent_id    INTEGER,
    UNIQUE (quest_id, entry_id)
);
CREATE INDEX idx_quest_comments_quest ON quest_comments(quest_id);

CREATE TABLE quest_memos (
    quest_id     INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL DEFAULT 0,
    content      TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    PRIMARY KEY (quest_id, user_id)
);
