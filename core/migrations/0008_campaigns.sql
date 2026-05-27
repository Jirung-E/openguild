-- DEV-011: Campaign 도입.
--
-- campaigns         — 캠페인 entity (제목 / 본문 / 기간 / 상태 / 정렬 인덱스).
-- campaign_checklists — 체크리스트 항목. 본문 markdown 의 `- [ ]` / `- [x]`
--                       에서 reindex 시 추출/동기화 (단방향: 파일 → DB).
-- campaign_quests   — Campaign ↔ Quest 다대다 연결.
-- campaign_counters — `C-NNN` 자동 numbering (quest_counters 패턴, 단일 row).
--
-- slug 형식: `C-001` (3자리 zero-pad). counter.last_number + 1 로 다음 번호 할당.
--
-- display_order — 어드민이 캠페인 목록 페이지에서 임의 순서 지정 가능.
-- 기본값 0. ORDER BY display_order ASC, created_at DESC 조합으로 정렬.

CREATE TABLE campaigns (
    id              INTEGER PRIMARY KEY,
    campaign_slug   TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    description     TEXT,
    status          TEXT NOT NULL DEFAULT 'active',
    started_at      TEXT,
    ended_at        TEXT,
    display_order   INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    deleted_at      TEXT
);

CREATE INDEX idx_campaigns_status ON campaigns(status) WHERE deleted_at IS NULL;
CREATE INDEX idx_campaigns_started_at ON campaigns(started_at) WHERE deleted_at IS NULL;
CREATE INDEX idx_campaigns_display_order ON campaigns(display_order, created_at DESC) WHERE deleted_at IS NULL;

CREATE TABLE campaign_checklists (
    id           INTEGER PRIMARY KEY,
    campaign_id  INTEGER NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    text         TEXT NOT NULL,
    checked      INTEGER NOT NULL DEFAULT 0,
    order_idx    INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_campaign_checklists_campaign_id ON campaign_checklists(campaign_id, order_idx);

CREATE TABLE campaign_quests (
    campaign_id  INTEGER NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
    quest_id     INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    PRIMARY KEY (campaign_id, quest_id)
);

CREATE INDEX idx_campaign_quests_quest_id ON campaign_quests(quest_id);

CREATE TABLE campaign_counters (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    last_number  INTEGER NOT NULL DEFAULT 0
);

INSERT INTO campaign_counters (id, last_number) VALUES (1, 0);
