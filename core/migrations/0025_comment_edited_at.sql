-- DEV-182: 댓글 편집 시각(edited_at) 을 quest_comments / campaign_comments
-- 캐시에 추가. file-truth 는 og-comment 마커의 `edited_at="..."` attr, 이
-- 컬럼은 목록/정렬용 파생 캐시(pinned, 0023 과 동일 패턴).
ALTER TABLE quest_comments ADD COLUMN edited_at TEXT;
ALTER TABLE campaign_comments ADD COLUMN edited_at TEXT;
