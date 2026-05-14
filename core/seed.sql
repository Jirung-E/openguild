DELETE FROM quest_positions;
DELETE FROM quest_dependencies;
DELETE FROM quests;
UPDATE quest_counters SET last_number = 0;

-- DEV
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  1, 'Setup CI/CD pipeline', 3, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  2, 'Implement login page', 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  3, 'Design database schema', 3, 1);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  4, 'Add JWT authentication', 2, 1);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  5, 'Write API documentation', 1, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  6, 'Implement quest board UI', 1, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  7, 'Setup WebSocket connection', 1, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  8, 'Add pagination to quest list', 1, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1,  9, 'Implement file upload', 4, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 10, 'Setup logging system', 3, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 11, 'Add email notifications', 1, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 12, 'Implement search feature', 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 13, 'Add keyboard shortcuts', 4, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 14, 'Optimize DB query performance', 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (1, 15, 'Migrate to new auth provider', 5, 3);

-- BUG
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  1, 'Fix login redirect loop', 2, 1);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  2, 'Status badge color mismatch', 3, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  3, 'Memory leak on board view', 1, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  4, 'Quest list not updating after delete', 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  5, 'Drag and drop broken on Firefox', 4, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  6, 'Node position resets on refresh', 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  7, 'Filter not applying to sub-quests', 1, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  8, 'Tooltip flickers on hover', 3, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2,  9, 'Markdown rendering broken for code blocks', 1, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (2, 10, 'Date format inconsistent across views', 3, 4);

-- REQ
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  1, 'Dark mode support', 1, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  2, 'Export quests to CSV', 1, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  3, 'Bulk status change', 1, 3);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  4, 'Quest templates', 5, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  5, 'Add time tracking per quest', 4, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  6, 'Calendar view for deadlines', 1, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  7, 'Multi-language support', 5, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency) VALUES (3,  8, 'Add quest priority drag reorder', 1, 3);

-- 서브퀘스트 (DEV-2: Implement login page)
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 16, 'Design login UI mockup',     3, 3, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 17, 'Build login form component',  2, 3, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 18, 'Connect form to auth API',    1, 2, 2);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 19, 'Add remember me checkbox',    1, 4, 2);

-- 서브퀘스트 (DEV-4: Add JWT authentication)
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 20, 'Research JWT libraries',      3, 3, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 21, 'Implement token generation',   2, 2, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 22, 'Implement token refresh',      1, 2, 4);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 23, 'Add token blacklist on logout', 1, 3, 4);

-- 서브퀘스트 (DEV-6: Implement quest board UI)
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 24, 'Setup Cytoscape.js',          2, 2, 6);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 25, 'Render nodes from API',       1, 2, 6);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 26, 'Implement drag to lane',      1, 1, 6);

-- 서브퀘스트 (DEV-12: Implement search feature)
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 27, 'Add search input to nav',     2, 3, 12);
INSERT INTO quests (quest_type_id, number, title, status_id, urgency, parent_quest_id) VALUES (1, 28, 'Implement full-text search',  1, 2, 12);

-- 선행 퀘스트 관계
INSERT INTO quest_dependencies (quest_id, prerequisite_id) SELECT q1.id, q2.id FROM quests q1, quests q2 WHERE q1.number = 4  AND q1.quest_type_id = 1 AND q2.number = 2  AND q2.quest_type_id = 1;
INSERT INTO quest_dependencies (quest_id, prerequisite_id) SELECT q1.id, q2.id FROM quests q1, quests q2 WHERE q1.number = 7  AND q1.quest_type_id = 1 AND q2.number = 4  AND q2.quest_type_id = 1;
INSERT INTO quest_dependencies (quest_id, prerequisite_id) SELECT q1.id, q2.id FROM quests q1, quests q2 WHERE q1.number = 11 AND q1.quest_type_id = 1 AND q2.number = 4  AND q2.quest_type_id = 1;
INSERT INTO quest_dependencies (quest_id, prerequisite_id) SELECT q1.id, q2.id FROM quests q1, quests q2 WHERE q1.number = 6  AND q1.quest_type_id = 1 AND q2.number = 3  AND q2.quest_type_id = 1;

UPDATE quest_counters SET last_number = 28 WHERE quest_type_id = 1;
UPDATE quest_counters SET last_number = 10 WHERE quest_type_id = 2;
UPDATE quest_counters SET last_number = 8  WHERE quest_type_id = 3;
