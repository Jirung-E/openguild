-- DEV-226: 캠페인 상태 변경 이력 — quest_history(0004/0007) 패턴 확장.
--
-- quest_history 와 달리 campaign_slug 를 처음부터 포함(quest 는 0007 에서
-- 후속 추가됐지만 campaign 은 신규라 바로 넣음). FK 안 함 — campaign 삭제돼도
-- history 보존, audit 가치 유지(quest_history 와 동일 정책).
CREATE TABLE campaign_history (
    id            INTEGER PRIMARY KEY,
    campaign_id   INTEGER NOT NULL,
    campaign_slug TEXT    NOT NULL,
    ts            TEXT    NOT NULL DEFAULT (datetime('now')),
    op            TEXT    NOT NULL,
    old_value     TEXT,
    new_value     TEXT,
    actor         TEXT
);

CREATE INDEX idx_campaign_history_campaign_ts ON campaign_history(campaign_id, ts DESC);
