-- DEV-087: 캠페인 배너 이미지 — `.guild/` 상대 경로 (예 'assets/C-001-banner.png').
-- 파일 frontmatter `image` 가 진리원, 본 컬럼은 캐시. NULL = 배너 없음.
ALTER TABLE campaigns ADD COLUMN image_path TEXT;
