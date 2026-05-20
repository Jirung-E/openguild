-- DEV-013: Quest 변경 이력 기록 (status 변경부터; 다른 op 는 후속 quest).
--
-- 설계:
-- - FK 안 함 (CASCADE 안 함): quest 삭제돼도 history 보존, audit 가치 유지.
-- - reindex 가 quest PK 재할당 시 history 의 quest_id 와 끊김 가능 — slug 기록 권장
--   이지만 단순화 위해 number+type_id 보존 (별도 join 필요 시).
-- - 일단 quest_id 그대로 두고, 후속에서 slug 필드 추가 검토.
--
-- op 명: 'change_status' (현 마일스톤), 후속: 'create', 'update_title',
-- 'update_description', 'update_urgency', 'change_parent', 'delete', 'restore',
-- 'add_prereq', 'remove_prereq'.

CREATE TABLE quest_history (
    id          INTEGER PRIMARY KEY,
    quest_id    INTEGER NOT NULL,
    ts          TEXT    NOT NULL DEFAULT (datetime('now')),
    op          TEXT    NOT NULL,
    old_value   TEXT,
    new_value   TEXT,
    actor       TEXT  -- 멀티유저 도입 후 사용자 식별자. 현재는 NULL.
);

CREATE INDEX idx_quest_history_quest_ts ON quest_history(quest_id, ts DESC);
