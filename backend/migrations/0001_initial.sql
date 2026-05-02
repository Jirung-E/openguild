-- Quest types (MVP: DEV / BUG / REQ 하드코딩)
CREATE TABLE quest_types (
    id          INTEGER PRIMARY KEY,
    prefix      TEXT    NOT NULL UNIQUE,
    color       TEXT    NOT NULL,
    description TEXT
);

-- Quest statuses (MVP: 5개 하드코딩)
CREATE TABLE quest_statuses (
    id         INTEGER PRIMARY KEY,
    name_en    TEXT    NOT NULL,
    name_ko    TEXT    NOT NULL,
    color      TEXT    NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0
);

-- Quests
CREATE TABLE quests (
    id              INTEGER PRIMARY KEY,
    quest_type_id   INTEGER NOT NULL REFERENCES quest_types(id),
    number          INTEGER NOT NULL,           -- 타입별 순번 (DEV-001의 001)
    title           TEXT    NOT NULL,
    description     TEXT,                       -- 마크다운 본문
    status_id       INTEGER NOT NULL REFERENCES quest_statuses(id),
    urgency         INTEGER NOT NULL DEFAULT 3, -- 1=Critical 2=High 3=Medium 4=Low
    parent_quest_id INTEGER REFERENCES quests(id),
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (quest_type_id, number)
);

-- Quest 타입별 순번 카운터
CREATE TABLE quest_counters (
    quest_type_id INTEGER NOT NULL PRIMARY KEY REFERENCES quest_types(id),
    last_number   INTEGER NOT NULL DEFAULT 0
);

-- 선행 퀘스트 관계
CREATE TABLE quest_dependencies (
    quest_id        INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    prerequisite_id INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    PRIMARY KEY (quest_id, prerequisite_id)
);

-- Quest Board 노드 위치
CREATE TABLE quest_positions (
    quest_id INTEGER NOT NULL PRIMARY KEY REFERENCES quests(id) ON DELETE CASCADE,
    x        REAL    NOT NULL DEFAULT 0,
    y        REAL    NOT NULL DEFAULT 0
);

-- 기본 Quest 타입 시드
INSERT INTO quest_types (prefix, color) VALUES
    ('DEV', '#4A90D9'),
    ('BUG', '#E94F4F'),
    ('REQ', '#7BB87F');

-- 기본 Quest 상태 시드
INSERT INTO quest_statuses (name_en, name_ko, color, sort_order) VALUES
    ('Open',        '게시됨',  '#8B95A1', 0),
    ('In Progress', '진행 중', '#4A90D9', 1),
    ('Done',        '완료',    '#7BB87F', 2),
    ('Cancelled',   '취소됨',  '#E94F4F', 3),
    ('On Hold',     '보류',    '#F5A623', 4);

-- 카운터 초기화
INSERT INTO quest_counters (quest_type_id, last_number)
SELECT id, 0 FROM quest_types;
