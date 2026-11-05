+++
created_at = "2026-06-23T01:30:44+09:00"
updated_at = "2026-11-05T22:52:15+09:00"
version = 5
+++
# 릴리즈 패키지 절차

매 릴리즈마다 반복되는 종합 절차. DEV-036 의 본문을 본 길드 규칙으로 정착.

## 사전 점검 — 네 관문을 **전부** 지난 뒤에만 "준비 끝" 이라고 한다

"준비 끝" · "태그해도 될까요" 는 아래 네 관문을 다 지나고 나서만 말한다. 하나라도 비어 있으면 무엇이
비었는지부터 말한다.

왜 이렇게 엄격한가 — 0.6.0 준비에서 테스트 · CI · 버전만 보고 "준비 끝" 이라고 보고했다. Testing 퀘스트는
제목만, 문서 · 스킬은 바뀐 줄 수만 봤다. 사용자가 묻고서야 다시 보니 BOOK-004 가 없어진 문법(`e.items`)을
설명하고 있었고, DEV-458 은 구조를 바꾼 뒤 앱에서 본 적이 없었다. 테스트가 통과해도 이 둘은 안 걸린다.

**네 관문 공통**

- **기준은 HEAD 다.** 점검 뒤 코드 커밋이 들어가면 그 영역은 다시 본다.
- **그 커밋으로 새로 빌드한 것으로 본다.** `just test` 는 실행 파일을 다 새로 만들어 주지 않는다 — 0.6.0 점검 때
  `openguild-server` 가 16시간 전 빌드였다. `cargo build -p openguild-cli -p openguild-server`,
  `cd gui/frontend && npm run build` 를 먼저 돌린다.
- **격리 길드에서 돌린다.** `OPENGUILD_HOME` 도 따로. 실제 길드에는 실험하지 않는다.
- **못 본 것은 못 봤다고 적는다.** "아마 될 것" 은 확인이 아니다.

### 관문 1 — 자동 검사

- [ ] `just test` 통과(EXIT=0, FAILED 0). 실패 수와 시험 수를 적는다.
- [ ] HEAD 의 `check` 워크플로 통과 — 윈도우 포함.
- [ ] HEAD 의 `release` 워크플로 수동 실행 통과 — 세 플랫폼 빌드와 `assemble & publish release`(태그 없이는 게시 안 함).

### 관문 2 — Testing 퀘스트를 **하나씩**

모든 Testing 퀘스트에 아래를 한다. 태그를 앞두고 한 번에 Done 으로 옮기지 않는다.

- [ ] **본문과 댓글을 전부 읽는다.** 제목 · CHANGELOG 언급 · "테스트 통과" 는 확인이 아니다.
- [ ] **HEAD 빌드로 다시 돌린다.** 버그는 본문의 재현 순서를 그대로 돌려 안 나는지, 기능은 주요 동작과 오류 경우를 본다.
- [ ] **옛 확인이 아직 유효한지 본다.** 확인 댓글보다 뒤에 같은 영역 커밋이 있으면 그 확인은 무효다
      (`git log --format='%h %ad %s' --date=iso --grep='\[퀘스트\]'` 와 댓글 시각을 맞춰 본다).
- [ ] **경로를 나눠 적는다** — CLI · 서버 · 웹 화면 · 데스크톱 앱 · 윈도우. 바뀐 경로에서 봐야 한다. 한 경로에서 본 것으로
      다른 경로를 확인했다고 하지 않고, 단위 시험은 실제 동작 확인을 대신하지 않는다.
- [ ] **결과를 퀘스트 댓글로 남긴다** — 확인한 커밋 번호, 돌린 경로, 못 본 경로.
- [ ] **에이전트가 못 보는 것은 사용자에게 넘긴다.** 따라 할 수 있는 순서와 기대 결과를 함께 준다. 사용자 확인 전에는 그 퀘스트를
      Done 으로 옮기지 않는다.

### 관문 3 — 문서와 스킬

**대상:** `README.md` · `AGENTS.md` · `docs/USAGE.md` · `CHANGELOG.md` · `skills/` 전체 · `.agents/skills/` ·
`examples/plugins/`(README · 스키마 · 예제 주석) · `.guild/rules/` · `.guild/library/` 의 설계 문서 · CLI `--help` ·
앱 안 도움말(`gui/frontend/src/lib/stores/locale.ts`). 바뀐 줄 수나 마지막 수정일은 확인이 아니다.

- [ ] **기계 대조** — 대상 문서의 `openguild` 명령 · 옵션이 CLI 에 있는지, `-q` · `query()` · `?q=` 쿼리가 실제로 돌아가는지
      실행해 본다. 걸린 것은 하나씩 오류인지 가려서 적는다.
- [ ] **없어진 것 찾기** — `git log $PREV..HEAD` 와 퀘스트 댓글에서 **없애거나 이름을 바꾼** 칸 · 옵션 · 문법 · 함수를 목록으로
      적고, 옛 이름을 대상 전체에서 찾는다. 시험은 이것을 못 잡는다(BOOK-004 의 `e.items`).
- [ ] **읽고 대조** — 이번 사이클에 바뀐 기능마다 그것을 설명하는 곳을 **읽고** 실제 동작과 맞춰 본다. 기계 대조는 명령이 있는지만
      알려 주고, 설명이 맞는지는 모른다.
- [ ] **스킬은 따라 해 본다** — 스킬은 에이전트가 그대로 믿고 쓴다. 스킬의 예시와 절차를 적힌 대로 격리 길드에서 돌린다.
      예제 플러그인은 `plugin check` · `plugin test`, 시험이 없는 예제는 없다고 적는다.
- [ ] **언어 짝** — 앱 도움말 · CLI 도움말의 한국어 · 영어가 같은 말을 하는지 본다.
- [ ] **안 읽은 파일은 보고에 적는다.**

### 관문 4 — 나머지

- [ ] 버전 파일이 모두 맞다(아래 "버전 동기화").
- [ ] CHANGELOG — 아래 "검증" 대로 태그와 대조한다. 노트에 없는 퀘스트도 하나씩 넣을지 뺄지 판단한다. 날짜는 태그 날.
- [ ] 알려진 BUG 는 고쳤거나 Known issues 에 적었다. 반려된 퀘스트가 없다.
- [ ] 이번 릴리스 캠페인을 닫을 준비 — 연결된 퀘스트 상태 정리 후 사용자 확인(마감은 태그 **전**에, 아래 "캠페인 마감" 절).
- [ ] `openguild check drift` · `check counters` 를 돌려 경고마다 이유를 적는다.
- [ ] BUG-041 후속: `core/migrations/` 의 새 mig 가 있다면 `_sqlx_migrations` 에 영향 — README 의 "복구" 절차 인지.

### 보고 형식

태그를 묻기 전에 아래를 보여 준다. 칸을 못 채운 줄이 있으면 태그를 묻지 않는다.

- 관문 1 — 확인한 커밋과 결과.
- 관문 2 — 퀘스트마다 **한 일 · 이번에 돌린 경로 · 못 본 경로 · 사용자 몫**.
- 관문 3 — 고친 것, 걸렸지만 오류가 아닌 것과 그 이유, **안 읽은 파일**.
- 관문 4 — 항목별 결과.

## 버전 동기화

다음 파일들의 version 값을 `X.Y.Z` 로 **일관 갱신**:

- `core/Cargo.toml`
- `cli/Cargo.toml`
- `server/Cargo.toml`
- `gui/Cargo.toml`
- `gui/tauri.conf.json` (`version` 필드)
- `gui/frontend/package.json`
- `skills/openguild-plugin/.claude-plugin/plugin.json` — **`-beta` 를 뗀 값**
  (앱이 `0.5.2-beta` 면 여기는 `0.5.2`).

**tauri-cli 는 `release.yml` 에 버전을 고정한다** — `Cargo.lock` 의 tauri 크레이트와 같은 2.x 로. 함께 올리고,
`release` 워크플로 수동 실행으로 세 플랫폼을 확인한다. 윈도우 설치본은 자체 `installer.nsi` 템플릿을 쓰므로 CLI 를
올리면 템플릿도 맞춰야 한다(2.12 는 템플릿에 없는 `RestartManager_StartSession` 매크로를 찾아 빌드가 깨진다).
2.12 는 RPM 버전의 `-` 도 거절해 `0.6.0-beta` 로는 RPM 을 못 만든다 — 2.12 이상으로 올릴 때 버전 표기도 함께 정한다.

**plugin.json 을 빼먹으면 사용자 쪽 스킬이 영영 갱신되지 않는다.** 설치된
스킬은 파일 내용이 아니라 이 version 으로만 갱신 여부를 판단한다. BUG-261:
이 목록에 plugin.json 이 없어서 범프가 습관으로만 이뤄졌고, 0.5.1 / 0.5.2 에서
끊긴 채 5주간 스킬 9건이 사용자에게 안 나갔다. 그동안 에이전트가 이미 고쳐진
제약을 현재 사양처럼 답했다. `cargo test -p openguild-cli` 의
`plugin_json_version_tracks_crate_version` 이 이제 이걸 막는다.

후속 quest 후보: 한 곳 (`[workspace.package]`) 에서 관리하도록 통합.

### 릴리스 후 확인 — 스킬이 실제로 갱신되는가

설치된 스킬은 앱이 번들한 `skills/` 에서 `~/.openguild/skill-marketplace/`
로 동기화된다. **새 버전 앱을 설치한 뒤** 한 줄로 확인한다:

```bash
grep version ~/.openguild/skill-marketplace/openguild-plugin/.claude-plugin/plugin.json
```

방금 릴리스한 버전이 나와야 한다. 안 나오면 사용자 쪽 에이전트가 옛 스킬을
계속 쓰게 되고, **아무 오류도 나지 않는다**(BUG-261 / BUG-267 이 그렇게
5주를 갔다). 두 버그가 각각 "버전을 안 올림" 과 "mtime 으로 판단함" 을
고쳤지만, 경로 전체가 도는지는 실제 설치본으로만 확인된다.

## CHANGELOG

### 먼저: 무엇을 적는가 — **이번 사이클에 한 일이 아니다**

노트에 적는 것은 **지난 릴리스 대비 사용자에게 달라진 것**이다. "이번에
작업한 퀘스트 목록" 과는 다른 집합이고, 그냥 두면 반드시 어긋난다. 0.5.3 에서
실제로 두 번 어긋났다:

- **사이클 안에서 넣었다 뺀 것은 순 변화가 0 이다.** [[BUG-257]] 이 문서
  스크롤을 잠갔고 [[BUG-264]] / [[BUG-265]] 가 그 부작용을 고쳤는데, 셋 다
  같은 미출시 구간이었다. 사용자는 아무 일도 겪지 않았는데 노트는 "모바일에서
  쓸 수 있게 만든 릴리스" 라고 요약하고 있었다 — **없던 문제를 고쳤다고
  광고한 것**이다.
- **여러 릴리스에 걸친 퀘스트는 이번 몫만 센다.** [[BUG-199]] / [[BUG-236]] /
  [[BUG-252]] / [[BUG-254]] 는 앞부분이 이미 0.5.2 로 나갔는데, 노트가 그
  부분까지 이번에 고친 것처럼 적고 있었다.

### 검증 — 항목마다 태그 대비로 확인한다

작성한 뒤가 아니라 **작성하면서** 확인한다. 초안을 퀘스트 목록에서 만들었다면
반드시 한 번 걸러야 한다.

```bash
PREV=v0.5.3-beta          # 직전 릴리스 태그

# 1) 이 퀘스트가 이번에 실제로 들어갔나 (0 이면 노트에서 뺀다)
git log $PREV..HEAD --oneline --grep='\[BUG-263\]'

# 2) 앞부분이 이미 나갔나 (>0 이면 **이번 몫만** 서술한다)
git log $PREV --oneline --grep='\[BUG-263\]'
```

둘 다 >0 이면 그 항목의 문장은 **이번 커밋들이 한 일로만** 좁힌다.

### 요약 문단은 업그레이드 경로에서 쓴다

"내가 이번에 무슨 일을 했나" 가 아니라 **"0.5.2 를 쓰던 사람이 0.5.3 을 받으면
무엇이 달라지나"** 로 쓴다. 작업량이 큰 쪽이 아니라 사용자에게 큰 쪽이
헤드라인이다 — 이 둘은 자주 다르다.

### 형식

`CHANGELOG.md` 신설 / 갱신 — Keep-a-Changelog 형식:

```
## X.Y.Z — YYYY-MM-DD

(요약 문단 — 아래 "릴리스 노트에는 요약을 반드시 쓴다" 절 참조)

**주요 변경점**

- **굵은 한 줄 제목** — 무엇이 달라졌는지 1~2문장 (퀘스트 ID)

---

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Known issues
- ...
```

## tag 작업 절차

```bash
git checkout develop
# 모든 testing 정리, 버전 업데이트 commit
git commit -m "[chore] X.Y.Z 버전"

git checkout master
git merge develop                # FF 권장 (사용자 정책)

# 태그 push 전에 캠페인을 닫는다 (아래 "캠페인 마감" 절 — 사용자 확인 필수)
openguild campaign end C-0NN
git commit -m "[C-0NN][guild] 캠페인 완료"

git tag vX.Y.Z
git push origin master --tags
```

## 캠페인 마감 — **태그 전에, 사용자 확인 후**

- **시점은 태그 push 전.** 태그를 밀면 끝난 기분이 들어 매번 잊는다. 그러면
  캠페인이 active 로 남고, 다음 릴리스 때 두 개가 열려 있어 무엇이 이번
  범위인지 흐려진다. 릴리스 커밋에 캠페인 상태까지 담겨야 태그가 가리키는
  트리가 "이 릴리스는 끝났다"를 그대로 담는다.
- **에이전트가 임의로 닫지 않는다.** 캠페인 마감은 "이번 범위를 여기서
  끊는다"는 판단이라 사용자 몫이다. 에이전트는 대상 캠페인과 연결된 퀘스트
  상태를 정리해 보여주고 **확인을 받은 뒤에** `campaign end` 를 실행한다.
  확인 없이 닫았다면 되돌린다(`campaign start`).

## GitHub Release artifact

- DEV-034 의 workflow 가 tag push 트리거로 installer 자동 첨부.
- description: `CHANGELOG.md` 의 해당 버전 절 그대로 — **자동화됨**(BUG-171).
  `release-notes` 잡이 `scripts/extract-release-notes.ps1` 로 태그(`vX.Y.Z`)에 맞는
  `## X.Y.Z` 절과 compare 링크를 한 번만 생성한다. 따라서:
  - 태그를 밀기 전에 CHANGELOG 절이 **반드시** 있어야 한다 — 없거나 헤딩이
    태그와 안 맞으면 릴리스 잡이 그 자리에서 실패한다(의도된 가드).
  - `generate_release_notes` 는 쓰지 않는다. 자동 생성을 플랫폼별로 실행하면
    본문이 잡 수만큼 중복된다(0.4.1 실사고).
  - 수동 실행의 미리보기 artifact도 OS별 복사본이 아니라
    `release-notes-preview` 하나만 만든다.
- Windows/macOS/Linux 잡은 패키지만 artifact로 올린다. 마지막
  `publish-release` 잡만 실제 GitHub Release를 한 번 생성·갱신한다.
- **`latest.json` 도 attach** — Tauri updater 가 `endpoints` 로 가리키는 파일.
  형식:
  ```json
  {
    "version": "X.Y.Z",
    "notes": "release notes",
    "pub_date": "YYYY-MM-DDTHH:MM:SSZ",
    "platforms": {
      "windows-x86_64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild_X.Y.Z_x64-setup.exe"
      },
      "darwin-aarch64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild.app.tar.gz"
      },
      "linux-x86_64": {
        "signature": "<minisign>",
        "url": "https://github.com/.../openguild.AppImage"
      }
    }
  }
  ```
- **`latest.json` 은 빌드 잡이 만들지 않는다** (DEV-314). updater 엔드포인트는
  파일 하나(`releases/latest/download/latest.json`)뿐이라, 플랫폼별 잡이 각자
  같은 이름으로 올리면 **마지막 잡이 이겨서 나머지 플랫폼이 통째로 사라진다.**
  빌드 잡은 패키지와 `.sig`를 플랫폼 artifact로만 올린다. 마지막
  `publish-release` 잡이 세 artifact를 내려받아 `*.sig`를 하나의
  `latest.json`으로 합치고, 그 입력 패키지와 manifest를 같은 Release에 함께
  게시한다. manifest가 가리키는 파일과 실제 배포 파일이 동일한 입력에서 나온다.
  - 새 플랫폼 추가 시 해당 빌드 artifact의 `.sig` 업로드, `publish-release`의
    artifact 다운로드, `platform_of()` 매핑과 expected set을 함께 갱신한다.
  - 서명 시크릿이 없으면 `.sig` 자체가 없으므로 `latest.json` 없이 릴리스가
    끝난다 — 설치 파일은 정상, 자동 업데이트만 비활성.
- BUG-045 (예정): `latest.json` 가 없으면 사용자 GUI 의 "업데이트 확인" 이 그냥
  실패. 첫 release 에서 반드시 포함.

## 사후 점검

- [ ] release page 에서 installer 다운로드 / 설치 / 실행 확인.
- [ ] `latest.json` 의 `platforms` 에 **그 릴리스가 지원하는 플랫폼이 전부**
      들어 있는지 (DEV-314 이전엔 한 플랫폼만 남는 사고가 가능한 구조였다).
      현재 대상: `windows-x86_64`, `darwin-aarch64`, `linux-x86_64`.
- [ ] macOS: dmg 열기 → Applications 로 드래그 → 첫 실행이 Gatekeeper 안내대로
      우클릭>열기로 통과되는지 (미서명 배포라 정상 동작이다).
- [ ] `openguild-gui --version` 출력이 새 version 맞는지.
- [ ] BUG-041 의 SchemaAheadBanner 가 구버전 길드 열 때 정상 표시되는지.
- [ ] Tauri updater (구 release 에서) 새 release 감지 동작 확인.

## 후속

- 사용자 피드백 채널 (GitHub Issues / discussions).
- 메이저 cleanup quest (멀티유저 / JWT / Campaign / 등).
- auto-updater (Tauri updater plugin) 의 첫 release 검증.

## 릴리즈 브랜치를 따로 쓸 땐 **develop 에서 분기** (0.4.1 실사고)

기본은 별도 브랜치 없이 develop → master FF + 태그다(위 절차). 부분 릴리스
등으로 release 브랜치가 필요하면 **반드시 develop 에서 분기**한다.
master 에서 따서 필요한 커밋만 체리픽해 채우면 아래가 전부 터진다 —
0.4.1(예외적으로 master 에서 분기)에서 실제로 다 겪었다:

- **중복 커밋**: 릴리스 후 develop 로 되가져올 때 체리픽 사본이 그대로
  유입된다(0.4.1: 머지된 41 커밋 중 **21 이 중복**).
- **`.guild` 갈라짐**: 캠페인이 링크한 퀘스트 파일이 그 브랜치에 없어
  dangling 이 되고(GUI 칸반이 빈 것처럼 보임), 퀘스트 상태가 브랜치마다
  어긋난다. 퀘스트 `.md` + history 사이드카를 수동 동반해야 했다.
- **카운터 오염**: index.db 는 브랜치를 안 따라간다([[BOOK-001]]). 브랜치
  전환 후 SQL 카운터가 파일보다 뒤처지면(0.4.1: SQL DEV 270 ↔ 파일 300)
  ID 발급이 **기존 파일과 충돌**할 수 있다(발급은 DB MAX 기준 self-heal).
- **스키마 경고**: 두 브랜치의 마이그레이션 개수가 다르면, 새 쪽 바이너리가
  만든 index.db 를 옛 쪽 앱이 열 때 "DB 가 더 새롭다" 경고가 뜬다.

### 부득이 master 에서 분기했다면 (릴리스 후 정리)

1. `git merge master` 로 develop 에 되가져온다 — 조상 관계가 생겨 다음
   릴리스 싱크가 자동화된다(중복 로그는 `git log --first-parent` 로 회피).
2. 아래 4항목은 머지가 자동으로 못 맞추므로 **직접 확인**한다(0.4.1 에선
   전부 갭이었다):
   - 버전 파일 6개 + `skills/openguild-plugin/.claude-plugin/plugin.json`
   - CHANGELOG — 확정된 릴리스 절 편입 + `Unreleased` 에서 **이미 출시된
     항목 제거**(안 하면 다음 릴리스 노트에 중복)
   - 스킬(`.agents/` · `skills/`) — 릴리스 브랜치에서만 고친 게 있는지
   - `.guild` 퀘스트/캠페인 상태(done) — 캠페인은 별도 확인
3. 머지 충돌 해결 원칙: **소스는 develop**(기능이 앞섬, 단 릴리스 수정
   마커를 grep 으로 전수 검증) · **퀘스트/캠페인 상태는 릴리스 브랜치** ·
   **history 사이드카는 합집합**(append-only 감사 로그 — 어느 쪽도
   상위집합이 아닐 수 있다) · **카운터는 큰 쪽**(단조 증가).
4. 브랜치 전환/머지 후 `rm .guild/index.db && openguild reindex` →
   `openguild check drift` + `openguild check counters` 로 확인.

## 릴리스 파이프라인을 "시험용 태그"로 테스트하지 말 것 (BUG-171)

updater 엔드포인트가 `releases/latest/download/latest.json` 이고 워크플로가
`prerelease: false` 로 하드코딩돼 있다. 즉 **아무 태그나 밀면 그게 곧
"latest"** 가 되어 모든 사용자 앱이 그 버전을 업데이트로 제안한다. 파이프라인
검증용 태그(`v0.0.0-test` 등)를 밀면 사용자에게 시험 빌드가 배포된다.

대신 릴리스 없이 검증하는 경로를 쓴다:

- `check` 워크플로의 **release notes extraction** 잡 — 매 push 마다 CHANGELOG
  최신 출시 버전으로 추출을 돌리고 compare 링크·인코딩 손상까지 검사한다.
  결과 본문은 `release-notes-preview` artifact 로 확인.
- `release` 워크플로를 **workflow_dispatch** 로 수동 실행 — 릴리스는 만들지
  않는다(태그 gate). `release-notes-preview` 하나로 본문을 확인하고, 마지막
  `assemble & publish release` 잡이 세 플랫폼 artifact를 합쳐
  `updater-manifest-preview`를 만든다.

정말 태그 기반 검증이 필요하면 그때는 `prerelease: true` 를 먼저 넣고
(latest 에서 제외됨) 검증 후 태그·릴리스를 삭제한다.

## 릴리스 노트에는 **요약**을 반드시 쓴다 (사람이 쓰는 부분)

CHANGELOG 의 각 버전 절은 `### Added` 같은 카테고리 목록 **앞에** 요약이
와야 한다. 릴리스 본문은 이 절을 그대로 싣기 때문에, 요약이 없으면 릴리스
페이지가 20~30개 항목의 평평한 목록이 되어 "이번 버전이 뭘 바꿨는지"가
읽히지 않는다. 이 부분은 자동 생성할 수 없다 — 항목을 기계적으로 요약해도
중요도 판단이 빠지기 때문이다. `extract-release-notes.ps1` 이 요약 없는 절을
**에러로 막는다**(절이 `### ` 로 시작하면 실패).

형식:

1. **2~4줄 문단** — 이 릴리스의 성격을 사용자 관점으로. "무엇을 할 수 있게
   됐는지 / 무엇이 덜 아프게 됐는지". 예: *"문서를 오가는 비용을 줄이는 데
   초점을 둔 릴리스. …"*
2. **`**주요 변경점**` + 3~5개 불릿** — 각 불릿은
   `**굵은 한 줄 제목** — 무엇이 달라졌는지 1~2문장 (퀘스트 ID)`.
3. **`---` 구분선** — 요약과 카테고리 목록(`### Added` …) 사이에 넣는다.
   릴리스 페이지에서 "사람이 쓴 요약" 과 "기계적인 전체 목록" 의 경계가
   시각적으로 드러나야 스크롤하는 사람이 어디까지가 요약인지 안다.

   **구분선 앞에는 반드시 빈 줄을 둔다.** 바로 위가 텍스트 줄이면 마크다운이
   `---` 를 setext 제목(h2)으로 해석해 구분선이 아니라 거대한 제목이 된다.

   `extract-release-notes.ps1` 은 절의 **시작**이 `### ` 인지만 검사하므로
   (`$body -match '^###\s'`) 구분선을 넣어도 추출은 영향받지 않는다.

쓸 때 판단 기준:

- **묶어서 말한다.** 한 주제를 여러 퀘스트가 나눠 구현한 경우 하나의 불릿으로
  합친다(예: DEV-276 + DEV-294 = "최근 본 문서"). 퀘스트 단위로 나열하면
  요약이 아니라 목록의 반복이다.
- **영향이 큰 것부터.** 사용자가 실제로 부딪히던 문제(실패·느림·불가)를 위로.
  내부 리팩터링·CI·문서 작업은 요약에서 빼고 카테고리 목록에만 남긴다.
- **숫자를 쓴다.** "빨라졌다"가 아니라 "0.5초 → 15밀리초", "1.5 MB → 64 MB".
- **퀘스트 ID 는 괄호로만.** 제목에 넣지 않는다 — 읽는 사람은 ID 를 모른다.
- **3~5개를 넘기지 않는다.** 전부 중요하다고 쓰면 아무것도 강조되지 않는다.
  넘칠 것 같으면 주제를 더 크게 묶는다.
