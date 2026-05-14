-- quests.parent_quest_id 에 ON DELETE SET NULL 추가.
-- 부모 삭제 시 서브퀘스트가 자동으로 분리(parent_quest_id=NULL)되도록.
-- (명시적으로 cascade 삭제할 서브 ID 는 백엔드에서 별도 처리.)
--
-- ⚠️ SQLite 트랜잭션 안에서는 PRAGMA foreign_keys 를 끌 수 없고,
--    `PRAGMA defer_foreign_keys = ON` 은 FK CHECK 만 지연할 뿐
--    DROP TABLE 시 발동되는 ON DELETE CASCADE / SET NULL 액션은 막지 못한다.
--    따라서 단순 DROP/RENAME 패턴을 쓰면:
--      - quest_dependencies (FK ON DELETE CASCADE) → 모든 행 삭제
--      - quest_positions    (FK ON DELETE CASCADE) → 모든 행 삭제
--      - quests_new.parent_quest_id (FK ON DELETE SET NULL) → 모든 값 NULL
--    되어 데이터가 통째로 날아간다.
--
-- 회피 방법: 관련 데이터를 임시 테이블로 백업 → 재구축 → 복원.

PRAGMA defer_foreign_keys = ON;

-- 1) 영향받는 데이터 백업
CREATE TEMPORARY TABLE _quests_parent_backup AS
    SELECT id, parent_quest_id FROM quests WHERE parent_quest_id IS NOT NULL;

CREATE TEMPORARY TABLE _deps_backup AS
    SELECT quest_id, prerequisite_id FROM quest_dependencies;

CREATE TEMPORARY TABLE _positions_backup AS
    SELECT quest_id, x, y FROM quest_positions;

-- 2) 새 quests 생성 (parent_quest_id 에 ON DELETE SET NULL)
CREATE TABLE quests_new (
    id              INTEGER PRIMARY KEY,
    quest_type_id   INTEGER NOT NULL REFERENCES quest_types(id),
    number          INTEGER NOT NULL,
    title           TEXT    NOT NULL,
    description     TEXT,
    status_id       INTEGER NOT NULL REFERENCES quest_statuses(id),
    urgency         INTEGER NOT NULL DEFAULT 3,
    parent_quest_id INTEGER REFERENCES quests(id) ON DELETE SET NULL,
    created_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    updated_at      TEXT    NOT NULL DEFAULT (datetime('now')),
    UNIQUE (quest_type_id, number)
);

-- 3) 데이터 복사. parent_quest_id 는 NULL 로 둔다 — 곧 DROP 으로 SET NULL 트리거가
--    어차피 NULL 로 만들어버리기 때문에 미리 NULL 로 넣어둔 뒤 백업으로부터 복원.
INSERT INTO quests_new
        (id, quest_type_id, number, title, description, status_id, urgency,
         parent_quest_id, created_at, updated_at)
    SELECT id, quest_type_id, number, title, description, status_id, urgency,
           NULL, created_at, updated_at
    FROM quests;

-- 4) 옛 테이블 제거 → 이 시점에 quest_dependencies / quest_positions 의 FK CASCADE 로
--    해당 테이블들이 비워진다. 그래서 1) 단계에서 백업이 필수.
DROP TABLE quests;
ALTER TABLE quests_new RENAME TO quests;

-- 5) 관계 복원
UPDATE quests
   SET parent_quest_id = (SELECT b.parent_quest_id
                            FROM _quests_parent_backup b
                           WHERE b.id = quests.id)
 WHERE id IN (SELECT id FROM _quests_parent_backup);

INSERT INTO quest_dependencies (quest_id, prerequisite_id)
    SELECT quest_id, prerequisite_id FROM _deps_backup;

INSERT INTO quest_positions (quest_id, x, y)
    SELECT quest_id, x, y FROM _positions_backup;

DROP TABLE _quests_parent_backup;
DROP TABLE _deps_backup;
DROP TABLE _positions_backup;
