-- DEV-288: 규칙(rule)·도서관(BOOK) 변경 이력의 캐시 테이블.
--
-- 배경: DEV-288 로 규칙/BOOK mutation 이 `.guild/history/{slug}.jsonl` 사이드카에
-- 기록되기 시작했지만(파일=진리원), 작업기록(worklog)은 index.db 의 캐시 테이블만
-- 집계하므로 그 활동이 타임라인/히트맵에 안 나왔다.
--
-- BOOK-001 불변식대로 **파일 → DB 일방향 투영**만 한다: reindex 가 사이드카를
-- 읽어 이 테이블을 채우고, 이 테이블에서 파일로 되쓰는 경로는 없다. 언제든
-- 지워도 reindex 로 100% 재구축된다.
--
-- quest_history/campaign_history 와 달리 kind 컬럼으로 rule/book 을 한 테이블에
-- 담는다 — 스키마가 같고 worklog UNION 도 한 갈래로 끝난다.
CREATE TABLE doc_history (
    id         INTEGER PRIMARY KEY,
    kind       TEXT NOT NULL,   -- 'rule' | 'book'
    slug       TEXT NOT NULL,   -- rule slug 또는 BOOK-NNN
    ts         TEXT NOT NULL,   -- 로컬 ISO8601 (다른 history 와 동일)
    op         TEXT NOT NULL,   -- create | update | delete | rename
    old_value  TEXT,
    new_value  TEXT
);

CREATE INDEX idx_doc_history_slug_ts ON doc_history(slug, ts DESC);
CREATE INDEX idx_doc_history_ts ON doc_history(ts);
