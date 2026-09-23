+++
book_id = "BOOK-008"
title = "초기 아키텍처 변경 이력 (2026-05 ~ 06)"
path = "설계 기록"
created_at = "2026-09-23T11:49:54+09:00"
updated_at = "2026-09-23T11:50:12+09:00"
deleted = false
+++

# 초기 아키텍처 변경 이력 (2026-05 ~ 06)

> **과거 기록이다 — 고치지 않는다.** 옛 `docs/architecture.md`(→ BOOK-003)에 붙어 있던 변경 이력과
> "향후 계획" 을 원문 그대로 옮겼다. 지금 구조는 [[BOOK-003]](개괄) 을 본다. 여기 적힌 파일 이름·수치는
> 그때의 것이라 지금과 다를 수 있다.
>
> 계획은 이제 퀘스트가 정본이다(`openguild quest list`). 아래 "향후 계획" 의 항목들은 그때 기준이다.
>
> 이력이 가리키는 `docs/architecture-refactor.md` · `docs/storage-design.md` 는 DEV-371 에서 지워졌다.
> 원문이 필요하면 git 에서 꺼낸다.
>
> ```bash
> git show c431eb94^:docs/storage-design.md
> git show c431eb94^:docs/architecture-refactor.md
> ```

## 변경 이력

- 2026-05-14 server 단일 crate → core/cli/server 분리, `backend/` 폴더 제거.
- 2026-05-16/17 저장소 모델 전환 — SQLite 진리원 → 파일 진리원 + SQLite 캐시.
  `core::repo` (파일) + `core::ops` (orchestration) + `core::store` (Store) 신설.
- 2026-05-21+ `gui/` crate 추가 — Tauri 2 desktop app (DEV-001~006).
- 2026-05-25 DEV-011 Campaign entity 추가 — 파일 형식 B3 (frontmatter + GFM
  task list 본문) + 4 테이블 (campaigns / campaign_checklists / campaign_quests
  / campaign_counters) + CLI / server / GUI 전체.
- 2026-05-26 DEV-034 멀티 컴포넌트 NSIS installer (GUI / CLI / Server / PATH).
- 2026-05-27 DEV-076 Quest desired_due / required_due 필드 + Home 임박 / Overdue
  섹션.
- 2026-05-28 DEV-063 Tauri 자동 업데이트 (updater + process plugin + 서명
  파이프라인) + DEV-084 설정 페이지 (`/settings`).
- 2026-06-04 BUG-047 drift / reindex 가 sibling `.comments.md` / `.memo.md` 를
  quest 본문으로 오인 — `repo::fs::list_quest_body_files` 로 좁힘.
- 2026-06-04 BUG-048 recents 테스트 env race — `static OnceLock<Mutex<()>>`
  직렬화 (사용자 머신 recents 오염 위험 해결).
- 2026-06-04 BUG-049 GUI 시동 시 자동 reindex — `drift::auto_resync` 가
  server / cli 와 동일하게 호출됨.
- 2026-06-04 DEV-094 (댓글) / DEV-099 (메모) — file 진리원 + DEV-102 로 DB 캐시
  (`quest_comments` / `quest_memos`) + snapshot 백업 합류 (migration 0011).
  메모의 `user_id=0` sentinel — multi-user (DEV-021) 진입 시 격리 활성.
  drift::detect_drift 가 sibling 파일도 fresh 감지 (auto reindex 트리거).
- 2026-06-05 DEV-098 installer NSIS resources 에 README + USAGE 동봉
  (사용자 친화 시작 가이드).
- 2026-06-05 DEV-018 `openguild info` 에 `--brief` / `--detailed` 모드.
- 2026-06-05 DEV-070 Quest Detail "Successors" 섹션 — quest_dependencies 역방향.
- 2026-06-05 BUG-046 Campaign 체크리스트 클릭 시 페이지 최상단 점프 — optimistic
  update 로 fix.
- 2026-06-05 DEV-101 UI 크기 슬라이더 + localStorage (rem scale).
- 2026-06-05 DEV-068 태그 풀스택 — migration 0010 + ops::set_quest_tags +
  CLI (`quest tag add/remove/list/set`) + HTTP `/api/quests/:id/tags` + Tauri command
  + Quest Detail tag pill UI + Quest List tag chip 필터 (AND).
- 2026-06-06 DEV-074 다크 / 라이트 / 시스템 테마 — CSS variable token + store +
  `<html data-theme>` + 25+ component 의 hardcoded color → var() 마이그레이션.
- 2026-06-06 DEV-093 캠페인 quest 진행도 — migration 0012 `quest_statuses.
  counts_as_done` + Home active 카드 progress 2 줄 + Campaign Detail 진행도
  + Admin Statuses 의 토글 UI.
- 2026-06-06 DEV-073 Quest Board toolbar 접기 토글 — lane 라벨 가림 해소.
- 2026-06-06 DEV-065 QuestList Tree / List 뷰 모드 토글 (URL + localStorage).
- 2026-06-06 DEV-077 arrangeNodesGrouped — cluster 의 y 좌표 lane 별 분리
  (lane 안 겹침 없으면 같은 row 공유).
- 2026-06-06 DEV-023 server CLI `vacuum` + `journal tail` 추가.
- 2026-06-06 BUG-054 QuestBoard.sorted → $state — long-standing npm check
  warning 제거 (0 warnings).
- 2026-06-07 DEV-068 `.guild/tags/{slug}.toml` 색·설명 — migration 0013
  `quest_tag_defs` + repo TagFile + Admin UI + Quest Detail tag pill 색.
- 2026-06-07 DEV-068 fix2: Tree 모드 + tag/type/status 필터 — child 매치 시
  `includeAncestors` 로 부모 트리 보존 (이전엔 검색만 처리).
- 2026-06-07 DEV-093 fix2: 캠페인 완료 판정 — 체크리스트 + 연결 quest 양쪽
  100% 일 때만 완료.
- 2026-06-07 DEV-074 fix2~10 라이트모드 마무리: 전역 커스텀 스크롤바
  (`scrollbar-gutter: stable`) + `<main>` bg → `var(--bg)` + Cytoscape style
  theme별 hex (`var()` 컴퓨팅 안 됨) + CodeMirror oneDark 조건부 +
  `--btn-primary-* / --btn-warning-* / --card-hl-* / --scrollbar-*` 토큰
  도입 + primary 버튼 11곳 sweep + 보드 설정 모달 체크박스 custom.
- 2026-06-07 DEV-101 fix2~5 슬라이더 리빌드: `CustomSlider` (델타 드래그 +
  click-jump + 즉시 적용 + 직접 숫자 입력) + `contentWidth` store +
  `--content-max-width` 토큰 (Home/Campaigns/Rules/Quest/Admin/Settings) +
  Nav 높이 `52px → 3.25rem` (UI scale 반영).
- 2026-06-07 DEV-105 fix2~7 lane 헤더 정리: 보드설정 모달에 lane reorder
  통합 + lane 헤더 zoom 무관 (UI overlay) + collapsed 영속 복원 + 긴 이름
  위 정렬 (overflow 아래로) + 레인별 설정 `⚙` 토글 + 펼침 시
  hideGroup/hideSolo 회귀 fix.
- 2026-06-07 DEV-113 등록: `[gui] 원격 서버 모드` (DEV-088 하위).
- 2026-06-07 DEV-114 등록: `[gui] 커스텀 테마` (사용자 토큰 색 자유 정의).
- 2026-06-07 DEV-052 fix: welcome 페이지 우상단 ⚙ 설정 링크 (Nav 숨김 상태).
- 2026-06-07 DEV-101 fix6~8: 설정 페이지 탭 분리 / 슬라이더 step 세분화 + 직접
  숫자 입력 / Nav height rem 화 / lane 헤더 내부 폰트·padding rem.
- 2026-06-07 DEV-074 fix11~17: 전역 체크박스 custom (`appearance:none`) /
  `<main>` 배경 / Welcome `--content-max-width` / window 진짜 overlay
  scrollbar (`OverlayScrollbar` 컴포넌트, html scrollbar 숨김 + transform 기반
  GPU composite) / QuestList / CodeMirror / Combobox / UpdateBanner / Settings
  toast / Quest 삭제 모달 list 에 overlay scrollbar.
- 2026-06-07 DEV-073 fix2~3: Quest Board 도구바 — New Quest 상단 고정 + 나머지
  는 그 아래 (가로) + 접을 수 있음.
- 2026-06-07 DEV-093 fix2-test: 캠페인 완료 판정 로직 `lib/utils/campaign-progress`
  로 추출 + vitest 회귀 10건.
- 2026-06-07 DEV-105 fix8~14: 보드 진입 시 collapsed lane 노드 hide / 가변 폭
  visual lane idx (collapsed 영역 클릭/드롭 회귀 fix) / drag 중 lane 강조 /
  grid snap SVG zoom·cols 캐시 / 세로 pan 추적 transform 기반 / wrappedBgY
  modulo cellH 로 dot 위치 정확도.
- 2026-06-07 DEV-026: cytoscape 동적 import — board route node chunk 646KB → 45KB.
- 2026-06-07 DEV-111: markdown 안 mermaid 다이어그램 렌더 (lazy import) — theme
  별 dark/default 분기.
- 2026-06-07 DEV-112: Quest Board 노드 배경 투명도 — `background-opacity: 0.92`
  + `background-image-opacity: 0.88` (border 는 opaque).
- 2026-06-07 DEV-115 등록 + 구현: Quest Board 의 최근 움직인 노드를 위로
  (z-index 단조 증가, drag / undo / redo 모두).
- 2026-06-07 DEV-109: Quest Detail 본문이 길 때 우하단 floating `↓ 댓글` 점프
  버튼 (anchor 의 viewport 위치 추적).
- 2026-06-07 DEV-107 fix1: 댓글 / 메모 섹션 접기 — 사용자 피드백 반영 영속
  localStorage 제거 + 답글 단위 접기 (root 별 `collapsedRoots: Set<number>`).
- 2026-06-07 BUG-020 fix2: arrangeNodesGrouped 의 cluster 식별을 lane-local BFS
  가 아닌 GLOBAL `groupOf` (cross-lane 포함 전체 의존 그래프) 기반으로 변경.
  같은 외부 그룹의 lane 멤버가 같은 cluster 직사각형 공유.
- 2026-06-08 DEV-111 fix1: mermaid syntax error 시 body 끝 leftover bomb SVG
  제거 — `mermaid.parse(code, {suppressErrors:true})` pre-check + 안전망 cleanup
  + `+layout.svelte` 의 `afterNavigate` sweep (`body > svg[id^="mm-"]` /
  `body > div[id^="dmm-"]`).
- 2026-06-08 BUG-056 / DEV-119: 인앱 `ConfirmDialog` 컴포넌트 (Esc/Enter +
  theme 토큰 + danger 변형) 도입 후 8 사이트의 native `window.confirm()` 교체
  — 댓글/규칙/캠페인/체크리스트 삭제 + admin status·type rename cascade +
  backup 복원 + rules nav-away. reindex 만 사용자 지시로 sweep 제외.
- 2026-06-08 DEV-117: CLI 의 `recents::add` 호출 제거 — CLI 활동이 Welcome
  '최근 연 길드' 를 오염시키던 문제. recents 의미를 GUI open 시점으로 한정.
- 2026-06-08 DEV-118: 댓글 답글 폼 자동 focus + scrollIntoView (긴 댓글이라
  폼이 화면 밖에 mount 되어 못 보던 케이스).
- 2026-06-08 DEV-120: admin reindex 후 600ms 토스트 → `window.location.reload()`
  — 모든 페이지 / store 가 fresh 데이터로.
- 2026-06-08 BUG-058: light 테마 date picker 아이콘 흰색 — `[data-theme='light']`
  에 `color-scheme: light` 누락. 1 line fix.
- 2026-06-08 BUG-059: drift detection 의 시간 임계값을 `index.db` 파일 mtime
  → `app_meta.last_indexed_at` ISO 마커로 교체 (migration 0014). SQLite WAL /
  Store::open 의 mtime 부작용으로 외부 편집을 못 잡던 false negative 해소.
  fix1 의 빈 마커 → fallback 이 같은 버그 경로였던 부트스트랩 결함을 fix2 에서
  epoch fallback 으로 정정.
- 2026-06-08 BUG-060: invalid urgency (범위 1..=4 밖) 데이터가 들어오면
  `URGENCY_LABEL[u]` undefined → `.length` 폭발로 보드 mount 실패. `types/index.ts`
  에 `urgencyLabel(u)` / `urgencyColor(u)` 헬퍼 (4 fallback) 추가 + QuestBoard
  4곳 + QuestListItem + quest detail 의 bare access 교체.
- 2026-06-09 DEV-074 fix20~22 (sweep A+B+C): semantic 토큰 (`--accent-secondary`,
  `--orange`, `--hl-pre/sub/next` + bg, `--hl-parent-bg`, `--selected-bg`,
  `--edge-pre`) 도입 + `theme.ts::themePalette(eff)` 단일 JS source +
  QuestBoard / quest detail / rules / campaigns / welcome / SchemaAheadBanner /
  QuestList / quest-node-svg 의 hex 와 `eff === 'light' ? ...` 분기 모두 정리.
  `src` 안 색 hex 0개. 재발 방지 규칙은 `docs/guild-rules.md` / `.guild/rules/
  frontend-theme-tokens.md` / DEV-074 본문 3곳에 명시.
- 2026-06-09 DEV-128: 댓글 #N 표시 + anchor 점프 — CommentEntry.id 는 이미
  있어 표시만, `<li id="comment-N">` + 답글은 `↩ #parent`.
- 2026-06-09 DEV-127 / DEV-123: Quest Detail floating cluster — '맨 위로' / '댓글로'
  / '메모로' 점프. DEV-109 의 단일 버튼을 `.jump-cluster` 로 refactor.
- 2026-06-09 DEV-125: Nav 에 테마 토글 (system → light → dark 순환) — system
  모드는 우하단 accent 도트로 표시. Settings 라디오는 그대로 유지.
- 2026-06-09 DEV-126: 페이지 새로고침 후 스크롤 위치 유지 — sessionStorage
  path 별 scrollY, scroll throttle 200ms + beforeunload 저장 + mount 시 rAF 2회
  후 복원. DEV-120 admin reindex 후 reload 와 결합.
- 2026-06-09 DEV-124: Quest Detail 의 Successors 섹션에 '+ 추가' 버튼 +
  core::services::list_candidates 에 'succ' relation 추가 (prereq mirror —
  has_prerequisite_path(id, c.id) 가 false). pickCandidate 는 succ 면
  addPrerequisite(candidate, id) (방향 반대).
- 2026-06-09 BUG-057: Quest Board 노드 흐림 — (A) cytoscape({pixelRatio:
  clamp(devicePixelRatio,1,3)}) 명시 + (B) makeSvgUrl / quest-node-svg 의 SVG
  width/height 를 dpr 배 px 로 발급 (viewBox 로 좌표 logical 보존). 보더 /
  그림자 / 텍스트 모두 또렷.
- 2026-06-09 DEV-121 Phase 1: startup incremental sync — migration 0015
  `quests.cached_mtime` (Unix nanoseconds, timezone-independent). 신규
  `core::incremental::sync_changed_quest_files` 가 각 .md 파일 stat() →
  DB cached_mtime 비교 → 변경된 것만 UPDATE. 신규/삭제는 needs_full_reindex
  flag → fallback `drift::auto_resync`. `Store::open_with_sync` helper +
  GUI startup hook 교체 (`drift::auto_resync` → `incremental::sync_on_open`).
  CLI / server 는 변경 없음 (CLI: stale 사용자 책임, server: DEV-122 분리).
- 2026-06-09 DEV-122 등록 (open): server long-running 의 startup + mid-runtime
  sync 전략. prerequisite DEV-121. (S1 startup + M1~M4 mid-runtime 옵션.)
- 2026-06-11 DEV-123~128: Quest Detail floating cluster (위/댓글/메모 점프) /
  Successors 추가 UI (`succ` relation) / Nav 테마 토글 / 새로고침 스크롤 유지 /
  댓글 #N anchor.
- 2026-06-11 BUG-061: admin 새 status 검은색 — COLOR_PALETTE 의 CSS var 혼입
  (DEV-074 sweep 오적용) → 구체 hex 환원. status 색은 TOML 데이터라 var 불가.
- 2026-06-11 DEV-129 / DEV-130: 댓글 본문 접기 (1줄 미리보기) / 편집창 Tab =
  들여쓰기 (CodeMirror indentWithTab 4곳 + textarea tabInsert action 3곳).
- 2026-06-12 DEV-110: CLI 댓글 list 필터 — --author/--since/--top-only/
  --reply-to/--grep (AND).
- 2026-06-12 DEV-060: 퀘스트 템플릿 — `.guild/templates/{name}.md` +
  `quest new --template` (명시 옵션 > 템플릿 > 기본) + `template list/show`.
  복사 기능은 사용자 결정으로 제외.
- 2026-06-12 DEV-108: 댓글 이모지 반응 — og-comment 마커 `reactions` attr
  (file-only, 캐시 무관) + 고정 4종 토글 UI + Tauri/HTTP surface. 커스텀
  이모티콘은 DEV-132 후속.
- 2026-06-12 DEV-033 fix2: List 고급 필터 (urgency / prereq·sub tri-state /
  날짜 범위) + Board 반영 (`quest-filter` 공유 store, 미매치 노드 fdim dim).
- 2026-06-12 DEV-100: 캠페인 댓글 / 메모 — `.guild/campaigns/{slug}.comments
  .md` / `.memo.md`, ops::campaign_comments + 같은 GUI 컴포넌트 scope prop
  재사용. repo::comments 에 path 기반 generic IO.
- 2026-06-12 BUG-063: server 시간 비교 테스트의 자정 flaky —
  subtract_one_minute 의 날짜 wrap 을 chrono 연산으로 교체.

자세한 설계 근거: `docs/architecture-refactor.md`, `docs/storage-design.md`.

## 향후 계획 (그때 기준)

- 멀티유저 인증 (JWT) — DEV-021. 메모의 user_id 격리 (DEV-102) 트리거.
- ✅ 댓글 / 메모 DB 캐시 + snapshot 백업 (DEV-102) — migration 0011 + reindex /
  drift / ops 캐시 sync 완료. 메모 user_id 격리는 DEV-021 진입 시.
- ✅ 태그 (DEV-068) — frontmatter + DB cache + 풀스택 (CLI / HTTP / GUI).
- ✅ 캠페인 quest 진행도 (DEV-093) — status.counts_as_done + 모든 layer.
- ✅ 다크 / 라이트 / 시스템 테마 (DEV-074) — CSS variable backbone + 마이그레이션.
  fix2~10 에서 토큰 확장: `--btn-primary-*` (primary 액션 버튼 통일, light 명도 ↑),
  `--btn-warning-*` (admin 복원), `--card-hl-*` (Home 의 overdue / completed 카드
  그라데이션), `--scrollbar-thumb*` (전역 thin 스크롤바), `--content-max-width`
  (DEV-101 컨텐츠 폭 슬라이더). fix11~17 에서 전역 체크박스 custom +
  `OverlayScrollbar` 컴포넌트 (window / 임의 컨테이너 양쪽 지원, transform 기반
  GPU composite, `target?: HTMLElement` prop). 신규 컴포넌트는 토큰만 참조 —
  `:global([data-theme='light']) .x` 직접 override 금지.
- ✅ Quest List Tree / List 토글 (DEV-065).
- ✅ Quest Detail 후속 퀘스트 (DEV-070).
- ✅ Quest Board toolbar 접기 (DEV-073, fix2~3: New Quest 상단 고정 + 도구바 그
  아래), arrangeNodesGrouped 개선 (DEV-077 + BUG-020 fix2: GLOBAL groupOf 기반
  cluster 식별 — 같은 외부 그룹의 lane 멤버는 같은 cluster 직사각형 공유).
- ✅ 노드 시각 polish: 배경 alpha 0.92 (DEV-112) + 최근 움직인 노드 z-index ↑
  (DEV-115) + drag 중 lane 강조 (DEV-105 fix11).
- ✅ Quest Detail 댓글 / 메모 (DEV-107 fix1): 섹션 접기 + 답글 단위 접기 (영속 X).
  본문이 길 때 우하단 floating `↓ 댓글` 점프 버튼 (DEV-109).
- ✅ Markdown 안 mermaid 다이어그램 (DEV-111) — lazy import, theme dark / default.
- 캠페인 댓글 / 메모 (DEV-100) — quest 와 동일 패턴.
- 다국어 (DEV-015) — i18n backbone 부터.
- 첨부파일 (DEV-069) — 새 기능.
- ✅ 레인 접기 (DEV-105) / 레인 순서 (DEV-059) — 본 라운드 (DEV-105 fix2~7)
  에서 통합 '보드 설정' 모달 + ⚙ 토글로 헤더 정리 + collapsed 영속 + hide
  설정 회귀 fix.
- 커스텀 테마 (DEV-114) — 사용자가 토큰 색 자유 정의 + 프리셋 저장 (DEV-074
  토대 활용).
- GUI 원격 모드 (DEV-113) — `openguild-server` URL 모드. DEV-021 (JWT) 권장.
- Journal replay (DEV-022) — 시점 복원.
- 길드 다중 동시 접속 (현재 SQLite 단일 파일 가정).
- AWS EC2 배포 — CI 는 GitHub Actions 로 일부 구축됨 (`.github/workflows/check.yml`).
