-- DEV-215: 도서관(Library) 문서 캐시.
--
-- 진리원: `.guild/library/{BOOK-NNN}.md` (+++ TOML frontmatter + 본문).
-- 본 테이블은 캐시 — 손실되어도 reindex 가 파일에서 재구축.
--
-- number 는 BOOK 자체 카운터(.guild/library/.counter.toml)가 부여하는
-- 단조 증가 값 — quest_types 의 counter 와 완전히 별개 네임스페이스.

CREATE TABLE library_docs (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    number     INTEGER NOT NULL UNIQUE,
    title      TEXT    NOT NULL,
    body       TEXT    NOT NULL DEFAULT '',
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL,
    deleted_at TEXT
);
