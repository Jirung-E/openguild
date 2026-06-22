-- DEV-134: 캠페인 댓글 / 메모 캐시 — DEV-102 (quest) 의 미러.
--
-- 파일이 진리원 (`.guild/campaigns/{slug}.comments.md` / `{slug}.memo.md`).
-- 본 테이블은 캐시 — 손실 시 reindex 가 파일에서 재구축. snapshot (index.db
-- binary copy) 백업 대상이 되어 gitignored 인 메모도 시점 복원 가능.
--
-- user_id sentinel 정책은 quest_memos 와 동일 (0 = single-user).

CREATE TABLE campaign_comments (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    campaign_id  INTEGER NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    entry_id     INTEGER NOT NULL,
    ts           TEXT    NOT NULL DEFAULT '',
    author       TEXT    NOT NULL DEFAULT '',
    body         TEXT    NOT NULL,
    parent_id    INTEGER,
    UNIQUE (campaign_id, entry_id)
);
CREATE INDEX idx_campaign_comments_campaign ON campaign_comments(campaign_id);

CREATE TABLE campaign_memos (
    campaign_id  INTEGER NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    user_id      INTEGER NOT NULL DEFAULT 0,
    content      TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    PRIMARY KEY (campaign_id, user_id)
);
