-- BUG-059: drift detection 견고화 — `last_indexed_at` 마커.
--
-- 이전엔 `detect_drift` 가 `index.db` 의 파일 mtime 을 임계값으로 썼는데
-- SQLite WAL checkpoint / Store::open 의 초기 write / 마이그레이션 등으로
-- 인덱스 파일 mtime 이 NOW 로 튀어버리는 사례가 있어 사용자가 외부에서
-- 편집한 파일을 "fresh 아님" 으로 잘못 판정하는 false negative 발생.
--
-- 대체: `reindex()` 가 종료 시점에 자기 자신 ISO 타임스탬프를
-- 'last_indexed_at' key 로 기록. `detect_drift` 는 그 값과 file mtime 비교.
-- 마커가 비어있거나 없으면 (legacy DB) 기존 동작 (index.db mtime) 으로 fallback.
CREATE TABLE IF NOT EXISTS app_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- 신규 길드는 빈 문자열로 시작 — detect_drift 가 fallback 으로 빠지지 않게.
-- (빈 값 = "마커는 있으나 reindex 한 번도 안 됨" → 모든 파일 fresh 로 판정 → 첫
--  reindex 트리거)
INSERT OR IGNORE INTO app_meta (key, value) VALUES ('last_indexed_at', '');
