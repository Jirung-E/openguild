-- DEV-068: tag 정의 캐시 — color / description.
--
-- 진리원: `.guild/tags/{slug}.toml`. 본 테이블은 캐시.
-- quest_tags 의 tag 가 def 없어도 정상 (color 기본 회색). def 가 있으면 UI 가 색.

CREATE TABLE quest_tag_defs (
    slug        TEXT PRIMARY KEY,
    color       TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT ''
);
