-- DEV-243: 도서관 문서 태그 (frontmatter 가 진리원, 본 테이블은 캐시).
--
-- quest_tags(0010) 와 동일 패턴 — 자유 문자열 태그, tag_defs(0013) 의 색/설명
-- 정의를 공유(엔티티 무관 단일 registry).

CREATE TABLE IF NOT EXISTS library_tags (
    book_id INTEGER NOT NULL REFERENCES library_docs(id) ON DELETE CASCADE,
    tag     TEXT    NOT NULL,
    PRIMARY KEY (book_id, tag)
);

CREATE INDEX IF NOT EXISTS idx_library_tags_tag ON library_tags (tag);
