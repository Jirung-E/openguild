# Changelog

Keep a Changelog 형식. 날짜는 로컬(KST) 기준.

## 0.3.0-beta — 2026-07-02

### Added
- **단일 origin 웹 배포** — server 가 SPA(`gui/frontend/build`)와 API 를 같은
  origin 으로 서빙, SPA 딥링크 fallback. 실행 위치(repo root / `target/release`)
  와 무관하게 정적 자산을 찾도록 exe 상대경로 탐색까지 보강. (DEV-195)
- **원격 서버 모드(MVP)** — Tauri 데스크탑 GUI 가 로컬 길드 대신 원격
  openguild-server 의 HTTP API 에 접속. 연결/해제는 Welcome 화면에서, 설정
  페이지엔 현재 연결 상태만 읽기 전용 표시. 인증은 범위 밖(신뢰된 네트워크
  전용). (DEV-113)
- **브라우저/원격 모드 첨부 업로드** 지원. (DEV-152)
- **다국어 인프라(한/영 토글)** — 설정 퀵메뉴 + 설정 페이지 모두에 노출.
  GUI 전역 스윕은 후속([DEV-205](.guild/quests/DEV-205.md)). (DEV-015)
- 네이티브 창 테마(Windows 타이틀바)를 앱 테마에 동기화.
- 네트워크 공유 길드(UNC 경로) 열기 — `canonicalize` 의 verbatim prefix
  (`\\?\UNC\`) 정규화로 `.guild` 를 유효 디렉토리로 인식 못 하던 문제 수정
  + index.db journal_mode 자동 분기(WAL→DELETE). (BUG-091)

### Changed
- 보드 줌 — 트랙패드 두손가락 스크롤=pan / Ctrl+스크롤=줌, 마우스 휠=줌.
  (WebView2 가 트랙패드 pinch 를 자체 소비해 DOM 으로 안 보내므로 Ctrl+스크롤로
  대체.) (BUG-090)
- 설정 페이지 '원격 서버' 탭 폐기 — '정보' 탭에 현재 길드 위치(로컬 경로 /
  원격 URL)로 통합. (DEV-207)
- Welcome 화면 길드 열기 — 폴더 선택(생성+열기 겸용)을 주 버튼으로 복원,
  `.guild` 마커 파일 직접 선택은 보조 링크로. (DEV-204)
- 길드 열기 안내 문구 정확화 — 마커는 `이름.guild` 파일.
- 본문 편집기 상단 중복 첨부 버튼 제거.
- MIT 라이선스 적용.

### Fixed
- reindex 후 보드 grid snap 점이 표시되지 않던 stale 캐시 버그. (BUG-092)
- 상세 화면 lazy refresh 가 다른 프로세스의 편집을 놓치던 문제 — 콘텐츠
  항상 다시 읽도록 수정.
- NSIS 설치본에서 `openguild-server` 의 `/` 접속이 항상 404 였던 문제 —
  설치 번들에 frontend 정적 자산(`gui/frontend/build`)이 빠져 있었음.
  설치 시 함께 복사되도록 수정. (DEV-195)
- 브라우저/원격 모드 admin 의 타입/상태 관리가 깨지던 문제 — `/api/admin/types`
  / `/api/admin/statuses` CRUD 라우트가 server 에 없어 SPA fallback(HTML)이
  JSON 자리에 응답됨. Tauri invoke 와 1:1 대응 라우트 추가. (DEV-193)
- 웹 접속 시 탭 아이콘이 Svelte 기본 로고로 표시되던 문제 — 앱 아이콘
  favicon(ico/png) 으로 교체. (BUG-101)
- Welcome 재방문 후 설정 페이지가 이전 길드 컨텍스트를 계속 표시하던 문제 —
  세션 단위 길드 컨텍스트 플래그로 수정. (BUG-099 후속)
- 콜드 스타트 시 Welcome 에서 뒤로가기로 빈 보드에 진입되던 문제 —
  redirect 를 history 교체로 변경. (BUG-100)

### Known issues
- **원격 모드 인증 없음** (JWT 미구현, DEV-021) — 원격 서버는 신뢰된
  네트워크에서만 사용할 것.
- 다국어(한/영)는 인프라 + 일부 화면만 적용 — 전역 문자열 스윕은 후속.
  (DEV-015, DEV-205)
- 터치스크린 핀치-투-줌 미지원 — 보드 줌은 Ctrl+스크롤/마우스 휠. (DEV-208)
- UI polish / server 기능 강화 umbrella 는 계속 진행 중. (DEV-088, DEV-089)

## 0.2.1-beta — 2026-06-23

### Added
- **메모 표시 3-모드**: 접기 / 고정(고정 높이 + 내부 스크롤, 드래그로 크기 조절·영속)
  / 확장(전체 높이). 기본 확장, 고정·확장 선택은 기억(접기는 매번 펼침). (DEV-189)
- **댓글 전체 접기/펼치기** 버튼 — 모든 댓글의 답글·본문을 일괄 토글(섹션 접기와
  별개). (DEV-190)
- 설치 동봉 문서에 `AGENTS_OPENGUILD_USAGE.md` 추가 (README/USAGE/CHANGELOG와 함께).
  (DEV-098)
- **시점 복원(journal replay)** — `backup restore --at <ISO8601-UTC>` / HTTP
  `/api/admin/restore` `at`: 최신 snapshot 을 복원한 뒤 journal(AOF) 을 그 시각까지
  재적용해 "마지막 백업 이후 ~ 임의 시점" 으로 복원. id 는 slug 경유로 재매핑(reindex
  대응). 내용 op(댓글/메모 본문)·type 변경·첨부가 낀 구간은 안전을 위해 거부하고
  full snapshot restore 를 안내(fail-loud). (DEV-022)

### Changed
- 메모 편집기의 '첨부' 버튼 제거(개인 메모). 이미지·동영상은 드래그&드랍 / Ctrl+V
  로 계속 첨부 가능. (DEV-188)
- 상세 점프 버튼(댓글로/메모로)을 양방향으로 — 타겟이 아래뿐 아니라 위로 지나갔을
  때도 표시(메모 영역에서 '댓글로' 노출). 퀘스트·캠페인. (DEV-191)

### Fixed
- 상세 페이지 재진입 시 스크롤이 맨 위로 초기화되던 문제 — 떠날 때 위치를 저장하고
  뒤로가기로 복귀 시 (콘텐츠 로드 후) 애니메이션 없이 복원. 퀘스트·캠페인. (DEV-192)

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
- **코드 하이라이팅**: 본문 마크다운 코드블록 syntax highlighting — highlight.js
  로컬 번들(외부 통신 없음), 테마 토큰 매핑으로 다크/라이트 적응. (DEV-183)
- **CLI 확장**: `reindex`, `check drift/counters`, `index rebuild/vacuum`,
  `journal tail`, `info`, `backup new/list/remove`, `restore`, `template new`,
  `migrate-to-files`, `quest comment discussion/resolved`(토론 토글).
  (DEV-095/159/162/164/170/176/177/179/185)
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
- 아이콘 투명 배경 — 투명 소스로 아이콘 세트 재생성. (BUG-088)
- IncompatibleGuild 모달의 '업데이트 확인'이 무반응이던 문제 — 결과(확인 중/최신/
  새 버전/실패)를 인라인 표시. (DEV-154)
- 업데이트 확인 에러 분류 — `latest.json` 부재를 네트워크 오류로 잘못 안내하던 것
  수정. (BUG-045)
- 그 외 다수 (보드 필터 edge 디밍, 다크모드 토큰 오용, urgency clamp 경고 등).

### Known issues
- **per-comment 첨부 미구현** (DEV-181, On Hold) — 현재 댓글은 이미지/동영상
  인라인만 가능, 비미디어는 차단.
- `quest_history`가 index.db 전용이라 파일/스냅샷에 백업되지 않음 (DEV-180).
- 자동 업데이트는 릴리즈에 `latest.json` + 서명(`.sig`)이 첨부돼야 동작 —
  GitHub 서명 secret 설정 필요. (release-process.md, BUG-045)

## 0.1.0-beta

최초 베타. (이 CHANGELOG 도입 이전 — 상세 내역은 git 이력 참조.)
