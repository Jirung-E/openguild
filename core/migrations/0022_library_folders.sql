-- DEV-239: 도서관 폴더(계층) — path 컬럼 + 빈 폴더 레지스트리 캐시.
--
-- 진리원: library_docs.path 는 각 문서 .md frontmatter 의 `path` 필드,
-- library_folders 는 `.guild/library/folders.toml` (빈 폴더 포함 명시적 존재).
-- 둘 다 캐시 — 손실되어도 reindex 가 파일에서 재구축.
--
-- path 는 "" = 최상위(루트), "아키텍처" 또는 "아키텍처/서브" 형태 (구분자 `/`).

ALTER TABLE library_docs ADD COLUMN path TEXT NOT NULL DEFAULT '';

CREATE TABLE library_folders (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    path       TEXT    NOT NULL UNIQUE,
    created_at TEXT    NOT NULL,
    updated_at TEXT    NOT NULL,
    deleted_at TEXT
);
