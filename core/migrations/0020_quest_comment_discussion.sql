-- DEV-142 후속: 토론(discussion) / 해결(resolved) 플래그를 quest_comments 캐시에
-- 추가. 이전엔 이 두 값이 파일 마커(attr)에만 있고 DB 엔 없어, quest 목록/홈/
-- 노드/리스트에서 미해결 토론 여부를 집계할 수 없었다(change_status 게이트는
-- 매번 파일을 직접 읽어 판정). file-truth 정책: 파일이 진실, DB 는 파생 캐시 —
-- 댓글 mutation(추가/수정/토글) 시 파일과 함께 이 컬럼도 동기화한다.
--
-- 0 = false, 1 = true. 기존 row 는 기본 0 (마커에 attr 없으면 false 와 동일).
-- 다음 reindex 또는 각 댓글 토글 시 파일 마커 기준으로 정정된다.
ALTER TABLE quest_comments ADD COLUMN discussion INTEGER NOT NULL DEFAULT 0;
ALTER TABLE quest_comments ADD COLUMN resolved INTEGER NOT NULL DEFAULT 0;
