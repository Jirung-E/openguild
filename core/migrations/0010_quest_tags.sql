-- DEV-068: Quest 별 자유 태그 (frontmatter 가 진리원, 본 테이블은 캐시).
--
-- 한 quest 가 여러 tag (소문자 / kebab-case 권장, 자유 문자열). 검색 / 필터링용.
-- frontmatter `tags = ["foo", "bar"]` 가 변경되면 reindex 가 본 테이블을 갱신.
--
-- 향후 (이 quest 범위 외) — `.guild/tags/{name}.toml` (color / description) 도입 시
-- 별도 `tag_defs` 테이블 가능. 지금은 자유 문자열만.

CREATE TABLE IF NOT EXISTS quest_tags (
    quest_id INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    tag      TEXT    NOT NULL,
    PRIMARY KEY (quest_id, tag)
);

-- 태그 별 quest 조회 가속 (필터링용).
CREATE INDEX IF NOT EXISTS idx_quest_tags_tag ON quest_tags (tag);
