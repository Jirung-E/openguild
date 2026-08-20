-- REQ-008: cross-link(`[[...]]`) 역인덱스 — "이 문서를 참조하는 문서" 조회용.
--
-- cross-link 는 단방향이다. A 가 `[[B]]` 를 걸면 A→B 는 본문만 보면 알 수 있지만,
-- B 를 보고 있을 때 "누가 나를 참조하는가" 를 알려면 전체 문서 본문을 뒤져야 한다.
-- 문서가 늘수록 그 비용이 커지므로 역방향을 미리 색인해 둔다.
--
-- BOOK-001 불변식대로 **파일 → DB 일방향 투영**만 한다: reindex 가 각 문서 본문을
-- 파싱해 이 테이블을 채우고, 이 테이블에서 파일로 되쓰는 경로는 없다. 언제든
-- 지워도 reindex 로 100% 재구축된다.
--
-- 링크는 **색인 시점에 해석해서** 확정된 대상만 담는다(깨진 링크는 제외).
-- 그래서 dst_kind 는 NULL 이 될 수 없다 — 조회 측이 우선순위 규칙을 다시
-- 구현하지 않아도 되고, 같은 ID 가 네임스페이스를 넘나들 때(DEV-219) 생기는
-- 오탐도 여기서 한 번만 정리된다.
CREATE TABLE doc_links (
    src_kind TEXT NOT NULL,   -- 'quest' | 'campaign' | 'rule' | 'book'
    src_id   TEXT NOT NULL,   -- quest_id / campaign_slug / rule slug / BOOK-NNN
    dst_kind TEXT NOT NULL,
    dst_id   TEXT NOT NULL,
    -- 같은 문서가 같은 대상을 여러 번 참조해도 backlink 는 한 줄이면 된다.
    PRIMARY KEY (src_kind, src_id, dst_kind, dst_id)
) WITHOUT ROWID;

-- 조회는 언제나 "이 문서를 가리키는 것들" 방향이다.
CREATE INDEX idx_doc_links_dst ON doc_links (dst_kind, dst_id);
