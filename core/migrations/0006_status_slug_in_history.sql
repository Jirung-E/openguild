-- DEV-042: quest_statuses.slug + quest_history.old/new_value 슬러그화.
--
-- 배경 (BUG): quest_history.old_value / new_value 는 변경 당시의 status_id
-- 숫자 문자열로 저장돼있음. 사용자가 status 를 추가/삭제/순서 변경하면 ID 가
-- 재배치되면서 옛 history 가 다른 status 를 가리키게 됨.
--
-- 해결: slug 는 stable identifier 로 .md frontmatter 와 파일명에서 이미 사용 중.
-- (1) quest_statuses 에 slug 컬럼 추가. reindex 가 채움.
-- (2) 기존 quest_history 행의 old/new_value (숫자) → 현재 매핑 기준 slug 로 변환.
--     이 시점의 매핑은 0005 까지 완료된 상태와 동일 (1=open, 2=in_progress,
--     3=testing, 4=done, 5=returned, 6=cancelled, 7=on_hold).
--
-- 새 코드 (DEV-042) 는 처음부터 slug 로 INSERT — migration 이후 모든 history
-- 행이 slug 형식.

-- (1) slug 컬럼 추가. NOT NULL 제약은 reindex 가 채워야 가능하므로 우선 nullable.
--     reindex 후엔 모든 행이 slug 보유.
ALTER TABLE quest_statuses ADD COLUMN slug TEXT;

-- (2) 기존 행의 slug 는 name_en 에서 파생: lower + space/hyphen → underscore.
--     예: "Open"→"open", "In Progress"→"in_progress", "On Hold"→"on_hold".
--     사용자가 어떤 id 매핑을 갖고 있든 안전 — 행마다 자기 name_en 만 본다.
--     reindex 가 곧 호출되어도 동일 결과.
UPDATE quest_statuses
SET slug = LOWER(REPLACE(REPLACE(name_en, ' ', '_'), '-', '_'))
WHERE slug IS NULL;

-- (3) quest_history 의 숫자 값 → slug 변환.
--     이 migration 은 1회만 실행. 이후 새 INSERT 는 처음부터 slug.
UPDATE quest_history
SET old_value = (SELECT slug FROM quest_statuses WHERE id = CAST(quest_history.old_value AS INTEGER))
WHERE op = 'change_status'
  AND old_value IS NOT NULL
  AND old_value GLOB '[0-9]*'; -- 숫자 형태만 변환 (idempotent — slug 형식은 skip).

UPDATE quest_history
SET new_value = (SELECT slug FROM quest_statuses WHERE id = CAST(quest_history.new_value AS INTEGER))
WHERE op = 'change_status'
  AND new_value IS NOT NULL
  AND new_value GLOB '[0-9]*';
