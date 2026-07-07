-- DEV-234: 댓글 상단 고정(pin) 플래그를 quest_comments / campaign_comments
-- 캐시에 추가. discussion/resolved(0020) 와 동일 패턴 — file-truth 는 og-comment
-- 마커의 `pinned="true"` attr, 이 컬럼은 목록/정렬용 파생 캐시.
--
-- discussion 과 달리 quest 전용 게이트가 없어 campaign_comments 에도 함께 추가.
ALTER TABLE quest_comments ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE campaign_comments ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
