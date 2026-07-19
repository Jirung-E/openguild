-- DEV-250: 이모지 반응을 quest_comments / campaign_comments 캐시에 추가.
-- file-truth 는 og-comment 마커의 `reactions="👍:a|b,✅:c"` attr — 이 컬럼은
-- 전역 comments 횡단 검색이 반응을 표시할 수 있게 하는 파생 캐시
-- (pinned 0023 / edited_at 0025 와 동일 패턴). 마커 attr 원문(콤마 구분)
-- 그대로 저장.
ALTER TABLE quest_comments ADD COLUMN reactions TEXT NOT NULL DEFAULT '';
ALTER TABLE campaign_comments ADD COLUMN reactions TEXT NOT NULL DEFAULT '';
