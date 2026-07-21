# Changelog

Keep a Changelog 형식. 날짜는 로컬(KST) 기준.

## Unreleased

### Added
- **최근 본 문서 목록** — 타이틀바 검색 pill 옆(웹은 메뉴바 우측)의 시계
  버튼으로 최근 방문한 퀘스트/캠페인/규칙/도서관 문서를 바로 열 수 있다.
  뒤로가기는 한 칸씩만 가므로 "아까 그 문서"로 점프하는 수단. 목록/보드/
  설정 같은 탐색 화면은 기록하지 않고, 최대 12개까지 세션 단위로 유지.
  (DEV-276)
- **크로스링크 호버 미리보기** — 본문/댓글/규칙/도서관에 렌더된
  `[[DEV-001]]` 류 크로스링크에 마우스를 잠시 올리면 검색 팔레트와 같은
  구성(종류 칩 + 제목 + 본문 미리보기 + 새창/페이지이동 버튼)의 팝업이
  링크 옆에 뜬다. 링크→팝업으로 마우스를 옮겨도 유지, 벗어나면 잠시 후
  닫힘. (DEV-256)
- **메뉴바 overflow 메뉴** — 창 폭이 좁아 메뉴바 링크가 다 안 들어가면
  오른쪽(우선순위 낮은) 항목부터 자동으로 숨기고, 데스크탑은 타이틀바
  ☰ 메뉴 상단(구분선 구획)으로, 웹은 메뉴바의 "⋯" 드롭다운으로 옮긴다.
  창이 넓어지면 자동 복귀, 활성 페이지 하이라이트도 유지 — 브라우저
  툴바의 priority+ navigation 패턴. (DEV-260)
- **알림 동시 다발 처리** — 알림 스택에 표시 상한(5개)을 두고 넘치면
  "+N개 더 보기" 칩으로 축약(클릭 시 전체 펼침). error 알림과 지속형
  카드(업데이트/스키마 경고)는 축약 대상에서 제외, 지속형은 항상 코너쪽에
  고정. 같은 문구의 토스트가 연발하면 새 카드 대신 ×N 뱃지로 갱신 + 수명
  리셋(중복 억제). dev 모드 한정 콘솔 트리거(`__ogNotify`)로 임의 알림
  조합 재현 가능. (DEV-266)

### Changed
- **연관 퀘스트 선택 팝업·검색 팔레트의 기본 정렬을 최근 갱신순으로** —
  생성 역순이라 오래된 퀘스트를 고르려면 목록을 뒤져야 했다. CLI
  `quest list` 의 기본 정렬은 그대로(필요하면 `--sort updated`). (DEV-277)

### Fixed
- **`openguild init` 이 만드는 `.gitignore` 에 openguild 자체 개발용
  이슈 번호와 한국어 주석이 들어가던 문제** — 사용자 저장소에 생성돼
  팀과 공유되는 파일이라 영어로 교체. (BUG-161)
- **데스크탑 앱에서 퀘스트 목록 API 의 정렬/필터 파라미터가 무시되던
  문제** — Tauri 경로가 query string 을 버리고 있었다(서버 모드는 정상).
  (DEV-277 작업 중 발견)
- **연관 퀘스트/캠페인 연결 팝업이 바깥 클릭으로 닫히지 않고 너무 좁던
  문제** — 퀘스트 상세와 캠페인 상세의 팝업 5곳에 백드롭 클릭 닫기를
  추가(삭제/타입변경 확인 모달은 진행 중이면 닫히지 않도록 가드). 콤보
  팝업은 목록이 본문인데 확인 모달과 같은 폭(30rem)을 쓰고 있어 제목이
  잘리고 후보가 5줄만 보였다 — 폭 56rem, 목록 높이 `min(70vh, 600px)`
  로 확대. (BUG-160)
- **검색 팔레트 스크롤바를 overlay 로 통일** — 결과 목록/미리보기 본문이
  native 스크롤바를 써서 컨텐츠 폭을 차지하고 있었다(주석엔 overlay 를
  쓴다고 적혀 있었지만 실제 연결이 빠져 있었음). 콤보박스 등 다른 스크롤
  영역과 같은 규칙으로. (BUG-157)
- **최근 본 문서 버튼이 타이틀바 검색 pill 을 밀어내던 문제** — 버튼 유무와
  무관하게 pill 이 항상 화면 중앙에 고정되도록 배치 변경. (BUG-158)
- **최근 본 문서 목록의 제목이 잘못 표시되던 문제** — 화면 h1 을 긁던 방식
  이라 도서관은 모든 문서가 "도서관", 규칙은 라벨과 중복 표시됐다.
  cross-link 인덱스에서 조회하도록 바꿔 정확해지고, 문서 이름을 바꾸면
  목록도 따라간다. (BUG-159)
- **띄어쓰기가 있는 규칙 이름을 cross-link 로 쓸 수 없던 문제** —
  `[[코딩 규칙]]` 처럼 공백이 들어가면 링크로 인식되지 않고 자동완성도
  공백에서 끊겼다. 규칙 slug 는 파일명이라 공백을 포함할 수 있는데
  패턴이 공백을 통째로 배제하고 있었음. 렌더러·본문 편집기·댓글
  자동완성 3곳 모두 수정(줄바꿈만 배제, 64자 상한). (BUG-156)
- **도서관 아이콘 보기에서 긴 문서 제목이 옆 타일 영역을 침범하던 문제**
  — 제목을 타일 폭 안에서 줄바꿈하고 3줄 초과는 말줄임. (BUG-153)
- **타이틀바 ☰ 메뉴가 타이틀바(빈 영역) 클릭으로 닫히지 않던 문제** —
  Tauri drag-region 스크립트가 mousedown 을 가로채 window click 리스너에
  이벤트가 도달하지 않았음. 검색 팔레트와 동일하게 capture-phase
  mousedown 으로 감지 전환. (BUG-155)
- **에이전트 스킬 트리거/지침 보강** — `.guild` 가 없는 디렉토리(새 길드
  init 시나리오)에서도 스킬이 트리거되게 description 수정(BUG-152),
  도서관/댓글 등 "조회"도 CLI 명령을 쓰도록 규칙 추가(BUG-154).
- **모바일(좁은 화면) 웹 레이아웃 1차 정리** — 375px 급 뷰포트에서 홈
  작업기록 카드와 작업기록 페이지 컨트롤이 가로 스크롤을 유발하던 문제를
  줄바꿈으로 수정, 검색 팔레트가 과도하게 좁아지던 것(62vw)을 모바일에선
  거의 전폭으로. 나머지 페이지는 전수 스캔에서 정상 확인 — 터치 인터랙션
  전환/Tauri 모바일 빌드는 후속. (DEV-257 1차)
- **크로스링크가 인덱스 적재 전에 렌더되면 '존재하지 않음'(빨강)으로
  영구 고착되던 레이스** — 인덱스가 늦게 적재돼도 이미 만들어진 링크를
  다시 resolve 해 정상 표시로 갱신(reindex 후 존재→미존재 전환도 동일
  경로로 반영). (DEV-256 작업 중 발견)
- **웹(브라우저) 배포에서 검색 팔레트/캠페인·작업기록·태그 페이지에
  접근할 방법이 아예 없던 문제** — 이 기능들이 커스텀 타이틀바(Tauri
  데스크탑 전용, `usesCustomTitlebar()`)에만 있었는데 웹은 그 타이틀바
  자체가 없었다. Nav 바에 웹 전용 fallback(검색 버튼 + 캠페인/작업기록/
  태그 링크)을 추가. (DEV-271)

### Changed
- **컨텐츠 폭 상한 1800px → 3200px, 최대값은 "전체"(폭 제한 해제)** —
  울트라와이드/4K 에서 양옆 여백이 과하게 남던 문제. 슬라이더를 끝까지
  밀면 모니터 크기와 무관하게 창 전체를 쓴다. (DEV-275)
- **`openguild docs <name>` 기본 출력이 전체 본문 → 목차로 변경** —
  늘 전체를 뱉어 사람은 스크롤, 에이전트는 토큰을 낭비했다. 이제 기본은
  번호가 붙은 목차(제목만), 특정 절은 `--section <번호|제목>`(하위 절
  포함, 제목은 대소문자·공백 무시 부분일치), 전체는 `--full`.
  목차/섹션은 `--json` 으로 구조화 출력도 지원. **주의: 전체 본문을
  기대하던 기존 호출은 `--full` 을 붙여야 한다.** (DEV-274)
- **`openguild comments` / `quest comment list` / `quest comment show` 옵션셋
  통일** — `comments` 에 `--top-only`/`--reply-to`/`--reverse`/`--tree` 추가,
  `quest comment list` 에 `--until`/`--discussion`/`--unresolved`/`--summary`
  추가. `comments` 기본 정렬을 최신순 → 오래된순(대화 흐름)으로 변경해
  `quest comment list` 와 통일(`--reverse` 로 최신순 전환은 그대로 가능).
  `quest comment show`/`campaign comment show` 는 `--id` 없이 실행 시
  기본 최근 20개만 출력(전체는 `--all`)하도록 변경 — 기존엔 무제한 전체
  출력. (DEV-262)

### Fixed
- **GUI/CLI/서버 시동 sync 가 무관한 quest `updated_at` 을 일괄 변조하던
  문제(BUG-103 재발)** — 콘텐츠 동일성 비교에 `updated_at` 자기 자신이
  껴 있어, DB 캐시가 한 번이라도 어긋나면 브랜치 전환/실행할 때마다
  영원히 재발하는 자기강화 루프가 있었다. 비교 대상에서 제외해 근본
  수정. (BUG-145)
- **`--json` 출력이 `--compact` 여부와 무관하게 늘 한 줄로 나오던 문제
  (rule/tag/worklog/quest/첨부/댓글/메모/도서관 등 다수 명령)** —
  `serde_json::json!(...)` 값을 `println!` 로 바로 Display 하는 경로가
  68곳 있었는데, 이 경로는 pretty/compact 를 결정하는 내부 헬퍼(json_str)
  를 안 거쳐서 항상 compact 로 나왔다. 전부 json_str 경유로 통일하고,
  같은 실수가 재발하지 않도록 소스를 스캔하는 회귀 테스트 추가. (DEV-261)

## 0.4.0-beta — 2026-07-19

### Added
- **에이전트용 정식 스킬 패키지** — `docs agents`(AGENTS_OPENGUILD_USAGE.md)
  문서를 대체해 Claude Code plugin marketplace 구조(`skills/`)로 제공.
  Windows installer 가 설치 시점에 바로 `~/.openguild/skill-marketplace/`
  로 복사해둬 앱을 한 번도 안 띄워도 `/plugin marketplace add` 로 바로
  등록 가능(그 외 플랫폼 + 소스 빌드는 앱 최초 실행 시 동기화). 기존
  `openguild docs agents` 는 제거됨(용도가 스킬로 이전). 스킬 자체에도
  작업 전 규칙/도서관 확인 체크리스트, 무엇을 quest/댓글/규칙/도서관으로
  남길지에 대한 판단 기준, 첨부파일 사용법을 추가로 다듬음.
  (DEV-264, DEV-268/269/270)
- **도서관(Library)** — 프로젝트 참고문서/노트 저장소 신설. 파일 진리원 +
  index.db 캐시(core), CLI `library` 명령군 + 서버 `/api/library` 라우트,
  GUI `/library` 페이지까지 전 구간 지원. 폴더 계층, 전문 검색(현재
  폴더+하위 범위, 폴더명 검색), 정렬 옵션(번호/이름/수정순), cross-link
  (`[[BOOK-NNN]]`) 통합 + 자동완성, 태그, 임의 파일 첨부(이미지/동영상
  외 포함, `library attach`), 백업/복원 대상 포함.
  (DEV-215/216/217/218/219/237/238/239/240/243/251)
- **Linux 배포 패키지(deb/rpm/AppImage)** 추가 — 기존 Windows NSIS installer
  외에 리눅스에서도 정식 패키지로 배포. (BUG-142)
- **rule list / tag list `--table` 출력** 추가 — 정렬된 표 형식(사람용).
  (DEV-252)
- **server 설정 파일(`openguild-server.toml`)** — frontend 정적 자산 위치를
  `--frontend-dist` / env 외에 설정 파일로도 지정 가능. API-only 모드로
  떨어졌을 때 원인과 지정 방법을 시동 로그에 안내. (DEV-229)
- **`library attach` 명령** — 도서관 문서에 큰/바이너리 첨부파일(기획 문서
  원본, PDF, zip 등)을 붙일 수 있게 CLI에 추가. core/server/GUI 는 이미
  지원하고 있었는데 CLI만 빠져 있던 걸 `quest attach`/`campaign attach` 와
  동일한 형태(list/add/remove)로 보완. (BUG-150)
- **`openguild docs <name>`** — usage/readme/changelog 번들 문서를 CLI에서
  바로 embed 출력. (DEV-248)
- **길드 전체 tag 관리 top-level 명령** 신설(`tag list/add/update/delete`).
  (DEV-228)

### Changed
- **CLI/서버 메시지 다국어화 확장** — quest/campaign/rule/library/worklog/
  tag/template 등 모든 서브커맨드의 `--help` 가 `locale` 설정(en/ko)을
  그대로 따라가도록 전환. (DEV-254)
- **커스텀 타이틀바 도입(VSCode 식)** — 플랫폼별 네이티브 타이틀바 테마
  어긋남을 원천 해소. 리눅스에서 커스텀 대신 네이티브로 표시되던 문제도
  함께 수정. (DEV-253, BUG-140)
- **창 컨트롤 버튼 — 플랫폼별 실제 네이티브화** — Windows 는 OS 가 실제로
  쓰는 아이콘 폰트(Segoe Fluent Icons/Segoe MDL2 Assets)로 교체하고
  `WM_NCHITTEST` 훅으로 최대화 버튼 호버 시 진짜 OS Snap Layout 이 뜨도록
  복원, macOS 는 `titleBarStyle: overlay` 로 네이티브 traffic light 를
  그대로 유지, Linux 는 실행 중인 GTK 아이콘 테마·`gsettings` 버튼 순서를
  실제로 조회해 렌더링(하드코딩 근사 폐기). (DEV-265)
- **댓글 시스템 대폭 개선** — 길드 전체 검색(`comments`), 기본 출력을
  요약 대신 본문 전체로(`--summary` 로 축약 선택), 핀(고정)/토론 필터,
  접기 상태 localStorage 영속, 답글 depth·부모 포함 출력 옵션(`--tree`,
  `--depth all`, `--with-parents`), 토론 해결/재개 전환의 이력 기록.
  (DEV-213/214/221/230/234/235/236/241/250)
- **CLI 명령 체계 재정리** — type/status/rule top-level 명령을 단수형으로
  통일(복수형은 alias 유지), `rule` 복수형 alias(`rules`) 및 `create`
  alias 완전 제거, quest 본문 `--description-file` 입력 지원.
  (DEV-222/227/231/232)
- **캠페인** — 퀘스트 진행바를 상태별 색 누적 바로 교체, 상태 변경도
  `quest_history` 패턴으로 이력 기록. (DEV-226/233)
- **커스텀 테마 프리셋을 `~/.openguild/themes.json` 파일로 이전**
  (기존 `localStorage`). (DEV-249)
- **사용자 데이터 위치를 `~/.openguild/` 로 일원화** — 설치 시 기존
  `docs`/recents 데이터 마이그레이션. (DEV-247)
- 퀘스트 상세의 '브랜치 이름' 섹션 제거, 검색 팔레트 결과 열기 방식
  선택 옵션(미리보기/자식윈도우/페이지이동) 추가. (DEV-255/258)
- **알림 시스템 통합** — 앱 곳곳에 흩어져 있던 개별 toast/`alert()` 구현과
  레이아웃을 밀어내던 in-flow 배너(SchemaAheadBanner)를 `ToastHost` 하나로
  정리하고, 업데이트 확인 알림과 동일한 우하단 카드 스타일로 통일.
  (DEV-259, BUG-139)
- **커스텀 테마 에디터 다국어 지원.** (DEV-205)
- 좁은 창 폭에서 어드민 페이지 등 UI 가 깨지는 문제(줄바꿈/버튼 구분선
  어긋남) 수정. (BUG-143)
- **메뉴바 상단 로고/길드명 블록 제거** — 타이틀바 pill 과 중복이라 정리
  (모든 플랫폼·웹 공통). (BUG-146)
- **`openguild docs readme` 에서 이 repo 고유 이슈 번호 인용 제거** —
  "복구" 절 제목의 `(BUG-041)` 같은 인용은 다른 길드 사용자가 자기 길드
  얘기로 오인할 수 있어 삭제. (DEV-263)
- **cross-link 는 명시 `[[..]]` 만 인식** — bare `DEV-033`(대괄호 없음) 자동
  링크와 일반 타이핑 중 자동완성 팝업을 제거. 자동완성은 `[[` 를 연
  상태에서만, 빈 `[[` 트리거와 `]]` 중복 방지도 함께 정리. 기존 본문의
  bare 참조는 더 이상 링크되지 않음(의도된 변경). (DEV-220, DEV-223)

### Fixed
- **리눅스 전반 성능 저하** — WebKitGTK 의 DMABUF 렌더러가 일부 GPU 드라이버
  조합에서 소프트웨어 합성으로 떨어져 보드/스크롤이 심하게 느려지는 문제
  완화(`WEBKIT_DISABLE_DMABUF_RENDERER=1`). (BUG-144)
- **도서관 관련 다수 수정** — 도서관/규칙/작업기록 페이지에서 뒤로가기가
  페이지를 그냥 나가버리던 문제, 트리뷰 폴더 접기 누락, 폴더 안 검색 시
  자기 자신이 결과에 뜨던 문제, 검색 범위가 전체 길드로 새던 문제,
  드래그앤드롭 폴더 이동, 뷰모드 토글 중복, 본문 저장/reindex 후 첨부파일
  목록이 사라지거나 재조회 안 되던 문제, JS 색 계산이 OS 테마 전환에
  반응 안 하던 문제.
  (BUG-119/120/121/122/123/124/126/127/128/129/130/133/134)
- **댓글 UI 다수 수정** — cross-link 추천 목록이 스크롤 안 되거나 조상
  overflow 에 잘려 표시되던 문제, 이모지 반응 팝업의 방향/화면밖
  클리핑/스크롤 추종/화면밖 클릭 시 안 닫히던 문제, 답글의 답글 입력창이
  스레드 맨 아래로 튀던 문제, 전역 댓글 검색에서 답글의 부착 위치가 안
  보이던 문제. (BUG-110/114/115/116/125/131/132)
- 보드(Board) grid snap 관련 노드 위치 오류 2건, 다른 길드로 전환해도
  보드/리스트 필터가 남아있던 문제. (BUG-109/112/113)
- 규칙 페이지에서 다른 규칙 `[[링크]]` 클릭 시 이동 안 되던 문제,
  `rule new` 실행이 멈추던 문제(create/new canonical 뒤바뀜),
  debug 빌드가 모든 명령에서 stack overflow 나던 문제(`run()` 단일
  프레임 1MB 초과), 신규 클론에서 dev-frontend API 가 404 나던 문제
  (`VITE_API_URL` 미설정), 웹 dev(SSR) 모드가 SPA 인데 `ssr=false` 를
  안 줘서 크래시 나던 문제, 오버레이 스크롤바가 타이틀바/메뉴바 위에
  그려지던 문제, justfile 빌드 레시피의 의존성 순서 오류, 서버 설정
  테스트가 `~/.openguild` 를 격리하지 않아 실기서 실패하던 문제, snapshot
  디렉토리명이 초 단위로 충돌해 과거 snapshot 을 오염시키던 문제,
  `change_status` 이력 append 타입 오류로 workspace 빌드가 안 되던 문제,
  Welcome 상태에서 설정 진입 시 길드 이름이 placeholder 로 뜨던 문제.
  (BUG-103/104/105/106/107/108/111/117/118/135/136/137/138/147/148)

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
