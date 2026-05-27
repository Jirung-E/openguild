-- DEV-049: quests.id 가 reindex 시 파일 정렬 순으로 재할당되어서, 캐시
-- 테이블 (quest_history / quest_positions) 의 quest_id FK 가 깨지는 버그.
--
-- 해결: slug (예 "DEV-007") 를 보조 stable identifier 로 추가 + 백필.
--
-- 새 코드는 INSERT 시 quest_slug 도 같이 채움. 읽기는 한동안 quest_id 로 진행
-- (server 가 slug → 현재 id 로 resolve 하므로 reindex 후에도 정상). 향후
-- quest_id 컬럼 제거 가능하지만 본 migration 에선 양립.
--
-- quest_dependencies 는 reindex 가 .md frontmatter 의 prereq / parent slug 에서
-- 재구축하므로 별도 변경 없음.

-- quest_history.quest_slug
ALTER TABLE quest_history ADD COLUMN quest_slug TEXT;

UPDATE quest_history
SET quest_slug = (
    SELECT qt.prefix || '-' || printf('%03d', q.number)
    FROM quests q
    JOIN quest_types qt ON q.quest_type_id = qt.id
    WHERE q.id = quest_history.quest_id
)
WHERE quest_slug IS NULL;

CREATE INDEX IF NOT EXISTS idx_quest_history_slug_ts
    ON quest_history(quest_slug, ts DESC);

-- quest_positions.quest_slug
ALTER TABLE quest_positions ADD COLUMN quest_slug TEXT;

UPDATE quest_positions
SET quest_slug = (
    SELECT qt.prefix || '-' || printf('%03d', q.number)
    FROM quests q
    JOIN quest_types qt ON q.quest_type_id = qt.id
    WHERE q.id = quest_positions.quest_id
)
WHERE quest_slug IS NULL;

CREATE INDEX IF NOT EXISTS idx_quest_positions_slug
    ON quest_positions(quest_slug);
