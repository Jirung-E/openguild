# Changelog

Keep a Changelog 형식. 날짜는 로컬(KST) 기준.

## 0.2.0-beta — 2026-06-22

v0.1.0-beta 이후의 대규모 개편 — 저장 모델을 "파일 진리원 + index.db 캐시"로
완성하고, 첨부·댓글·캠페인 기능과 CLI/문서를 정리했다.

### Added
- **첨부파일**: quest/campaign 본문 아래 첨부 섹션, drag&drop · 클립보드 paste ·
  버튼 업로드, 미디어 인라인 임베드, 첨부 삭제(orphan 파일/blob GC). CLI
  `quest/campaign attach list/add/remove`. (DEV-069/156/170/175, BUG-084)
- **댓글**: cross-link 자동완성(caret 팝업 + 실재 ID 제안), 이모지 반응,
  토론(discussion) 플래그 + 미해결 시 완료 차단 + 홈 "토론 댓글" 섹션.
  (DEV-171/108/142/148/149/150, BUG-082)
- **캠페인**: 본문 첨부, floating 점프 버튼, 배너 이미지.
- **CLI 확장**: `reindex`, `check drift/counters`, `index rebuild/vacuum`,
  `journal tail`, `info`, `backup new/list/remove`, `restore`, `template new`,
  `migrate-to-files`. (DEV-095/159/162/164/170/176/177/179)
- **호환성**: 길드 `schema_version` + 실행파일 호환 검사 + 안내 배너,
  미저장 변경 경고(라우트 이동). (DEV-064/154/153)
- **자동 업데이트**: Tauri updater 기반 + tag push 릴리즈 워크플로. (DEV-063/071)
- DB 캐시 엔티티(campaigns/types/statuses/tags) 외부편집 반영. (DEV-178)

### Changed
- **`openguild-server` = host 전용** — 중복 정비 서브커맨드 제거, 정비/진단은
  `openguild` CLI 또는 HTTP admin(`/api/admin/*`)로 일원화. (DEV-163/165)
- **CLI 명령 체계 정리**: 생성은 `new`(quest/campaign/template/backup), 하위항목은
  `add`, 삭제는 `remove`(구 `rm`). 정비는 `check`/`index`/`journal` 그룹.
  `reindex` = `index rebuild`. (DEV-176/177/179)
- **백업/복원을 파일 기반(RDB)으로** — index.db binary 사본이 아니라 `.guild/`
  소스 파일 스냅샷 → rules/댓글/메모/첨부까지 복원. (BUG-076)
- 스냅샷 타임스탬프는 UTC로 저장하고 표시할 때만 로컬 변환. (BUG-086)
- 외부 편집 시 `updated_at`을 파일 mtime으로 보정(quest + campaign). (BUG-080)
- 살아있는 문서를 현행 CLI/구조에 맞게 일괄 갱신. (DEV-166)

### Fixed
- drift 오탐(per-row/per-file mtime 비교). (BUG-067/068)
- reindex self-heal이 미참조 orphan 첨부를 부활시키던 문제. (BUG-087)
- `attach remove`가 없는 경로에도 성공 메시지를 내던 문제. (BUG-085)
- GUI 창이 안 닫히던 회귀(닫기/새로고침 가드 제거). (BUG-075)
- 그 외 다수 (보드 필터 edge 디밍, 다크모드 토큰 오용, urgency clamp 경고 등).

### Known issues
- **per-comment 첨부 미구현** (DEV-181, On Hold) — 현재 댓글은 이미지/동영상
  인라인만 가능, 비미디어는 차단.
- `quest_history`가 index.db 전용이라 파일/스냅샷에 백업되지 않음 (DEV-180).
- 자동 업데이트는 릴리즈에 `latest.json` + 서명(`.sig`)이 첨부돼야 동작 —
  GitHub 서명 secret 설정 필요. (release-process.md, BUG-045)

## 0.1.0-beta

최초 베타. (이 CHANGELOG 도입 이전 — 상세 내역은 git 이력 참조.)
