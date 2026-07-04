# restore 동작 정리

`restore` 명령으로 가능한 모든 동작과 그 결과. (임시 정리 — 추후 kb 로 이관 예정: DEV-184)

## 동작 전수 (현재 구현)

| 명령 | 스냅샷 | journal replay | journal 처리 | 결과 상태 | 가역성 |
|------|--------|---------------|-------------|----------|--------|
| `restore` | 최신 | 안 함 | 보존 | 최신 스냅샷 시점 | 가역 (journal 남아 `--at` 로 복구 가능) |
| `restore --to <YYYYMMDD-HHMMSS>` | 지정 | 안 함 | 보존 | 그 스냅샷 시점 | 가역 |
| `restore --at <과거/특정 ISO>` | 최신(항상) | 그 시각까지 | truncate | 스냅샷 + op(그 시각까지) | 비가역 (이후 op 폐기) |
| `restore --at latest` (DEV-210) | 최신(항상) | 전체 | truncate | 현재(최신) 상태 복구 — **손상 복구의 정식 진입점** | 상태는 무손실이나 journal 히스토리 사라짐 |

## 핵심 규칙

- **`--to` ↔ `--at` 상호배타**(`conflicts_with`). "특정 스냅샷 + replay" 조합은 불가 — `--at` 은 언제나 **최신 스냅샷** 위에만 replay.
- **`--at` fail-loud 거부**: 구간에 내용 op(댓글/메모 본문)·type 변경·첨부가 끼면 부분복원하지 않고 거부(full snapshot restore 안내). journal 은 감사 로그라 본문 미기록이기 때문.
- **가역성의 핵심 = journal 보존 여부**:
  - `restore` / `--to` → journal 보존 → 나중에 `--at <미래>` 로 최신까지 복구 가능.
  - `--at` → journal truncate → 비가역. (그래서 DEV-212 자동 백업이 필요.)
- 모든 경로가 내부적으로 `.pre-restore/` 롤백 슬롯(현재 소스 + index.db) 1개를 남김. 단 **임시**(다음 restore 시 덮어씀) + 백업 목록엔 안 뜸 + journal 미포함.
- 스냅샷 timestamp 는 `backup list` 로 확인 → `--to` 에 사용. 스냅샷 즉시 생성은 `backup new`.
- 복원 의도별 요약: **최신으로 복구(손상 복구)** = `restore --at latest` (= 최신 스냅샷 + journal 전체 replay — DEV-210). **특정 백업으로 되돌리기** = `restore --to <ts>`(가역).

## 계획된 변경 (미구현)

- **DEV-210**: `restore --at latest` 키워드 — `--at <미래 ISO>`(최신 복구)의 정식 진입점.
- **DEV-212**: `--at`(non-empty journal) 실행 직전 현재 상태 자동 스냅샷 → 비가역 파괴 방지.
- **BUG-102**: `restore` 후 `statuses` 목록 변동 조사(정상 복원인지 버그인지).
