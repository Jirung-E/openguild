+++
created_at = "2026-07-05T01:46:48+09:00"
updated_at = "2026-08-02T16:20:26+09:00"
+++
# restore 동작 정리

`restore` 명령으로 가능한 모든 동작과 그 결과. (임시 정리 — 추후 kb 로 이관 예정: DEV-184)

## 동작 전수 (현재 구현)

| 명령 | 스냅샷 | journal replay | journal 처리 | 결과 상태 | 가역성 |
|------|--------|---------------|-------------|----------|--------|
| `restore` | 최신 | 안 함 | 보존 | 최신 스냅샷 시점 | 가역 (journal 남아 `--at` 로 복구 가능) |
| `restore --to <YYYYMMDD-HHMMSS>` | 지정 | 안 함 | 보존 | 그 스냅샷 시점 | 가역 |
| `restore --at <과거/특정 ISO>` | 최신(항상) | 그 시각까지 | truncate | 스냅샷 + op(그 시각까지) | **가역** — 실행 직전 현재 상태가 스냅샷으로 자동 백업됨(DEV-212, journal 비어있으면 스킵). 되돌리기 = `restore --to <pre_backup>` |
| `restore --at latest` (DEV-210) | 최신(항상) | 전체 | truncate | 현재(최신) 상태 복구 — **손상 복구의 정식 진입점** | 상태는 무손실이나 journal 히스토리 사라짐 |

## 핵심 규칙

- **`--to` ↔ `--at` 상호배타**(`conflicts_with`). "특정 스냅샷 + replay" 조합은 불가 — `--at` 은 언제나 **최신 스냅샷** 위에만 replay.
- **`--at` fail-loud 거부**: 구간에 내용 op(댓글/메모 본문)·type 변경·첨부가 끼면 부분복원하지 않고 거부(full snapshot restore 안내). journal 은 감사 로그라 본문 미기록이기 때문.
- **가역성의 핵심 = journal 보존 여부**:
  - `restore` / `--to` → journal 보존 → 나중에 `--at <미래>` 로 최신까지 복구 가능.
  - `--at` → journal truncate — 단 실행 직전 현재 상태가 자동 백업되어(DEV-212)
    `restore --to <pre_backup>` 으로 복귀 가능(가역화). journal 이 비어있으면 백업 스킵.
- 모든 경로가 내부적으로 `.pre-restore/` 롤백 슬롯(현재 소스 + index.db) 1개를 남김. 단 **임시**(다음 restore 시 덮어씀) + 백업 목록엔 안 뜸 + journal 미포함.
- **첨부파일은 백업/복원 대상이 아니다** (BUG-188). 스냅샷은 첨부 목록
  사이드카(`{slug}.attachments.json`)만 담고 `.guild/attachments/` 의 실제
  파일은 담지 않는다 — 복원해도 그 파일들은 **그대로 남는다**(지워지지도,
  되살아나지도 않는다). 스냅샷에 없다는 사실이 삭제를 뜻하지 않는 유일한
  예외이므로, 복원 로직에 "스냅샷에 없으면 지운다" 를 넣지 말 것.
- 스냅샷 timestamp 는 `backup list` 로 확인 → `--to` 에 사용. 스냅샷 즉시 생성은 `backup new`.
- 복원 의도별 요약: **최신으로 복구(손상 복구)** = `restore --at latest` (= 최신 스냅샷 + journal 전체 replay — DEV-210). **특정 백업으로 되돌리기** = `restore --to <ts>`(가역).

## 계획된 변경 (미구현)

- **DEV-210**: `restore --at latest` 키워드 — `--at <미래 ISO>`(최신 복구)의 정식 진입점.
- ~~DEV-212~~ 구현됨(2026-07-05) — 위 표/규칙에 반영.
- **BUG-102**: `restore` 후 `statuses` 목록 변동 조사(정상 복원인지 버그인지).
