# openguild 사용 가이드

> 처음 설치한 사용자를 위한 빠른 시작 + 핵심 기능 안내.

---

## 1. 첫 실행

설치 후 시작 메뉴 또는 바탕화면에서 **openguild** 를 실행하면 Welcome 화면이
열립니다. 길드를 새로 만들거나, 기존 길드 폴더를 열 수 있습니다.

### 플랫폼별 설치본

| 플랫폼 | 첨부 파일 | 비고 |
|--------|----------|------|
| Windows x64 | `openguild_{version}_x64-setup.exe` | GUI / CLI / 서버 선택 설치 + PATH 등록 옵션 |
| Linux x64 | `.deb` / `.rpm` / `.AppImage` | GUI 만 포함 |
| macOS (Apple Silicon) | `openguild_{version}_aarch64.dmg` | GUI 만 포함. Intel 맥은 대상 외 |
| macOS CLI / Server | `openguild_{version}_macos_arm64_cli-server.tar.gz` | 사전 빌드된 실행 파일. Rust/Cargo 불필요 |

**macOS 첫 실행 — Gatekeeper.** 코드 서명을 하지 않은 빌드라 그냥 더블클릭하면
"개발자를 확인할 수 없기 때문에 열 수 없습니다" 로 막힙니다. 둘 중 하나로 한 번만
통과시키면 그 뒤로는 평범하게 열립니다.

1. Applications 에서 openguild 를 **우클릭(또는 Control-클릭) → 열기 → 열기**
2. 또는 터미널에서 격리 속성 제거:

```bash
xattr -dr com.apple.quarantine /Applications/openguild.app
```

macOS 에서 CLI(`openguild`)·서버(`openguild-server`)는 dmg 에 들어 있지 않습니다.
같은 Release의 tar.gz를 내려받아 설치합니다.

```bash
tar -xzf openguild_*_macos_arm64_cli-server.tar.gz
cd openguild_*_macos_arm64_cli-server
sudo install -d /usr/local/bin
sudo install -m 755 openguild openguild-server /usr/local/bin/
openguild --version
openguild-server --version
```

### 길드 만들기

길드 = 한 프로젝트 단위. (예: "내 개인 프로젝트", "팀 X 의 사이드 프로젝트")

1. Welcome 화면에서 **"새 길드 만들기"** 클릭.
2. 빈 폴더 선택 (예: `C:\Users\me\Projects\my-app`).
3. 길드 이름 입력 (예: `my-app`).
4. 자동으로 `my-app.guild` 마커 파일 + `.guild/` 디렉토리 (캐시 / 백업) 생성.

### 기존 길드 열기

- Welcome 의 **"폴더 열기"** → 길드 폴더 선택.
- 또는 탐색기에서 `*.guild` 파일을 더블클릭 (파일 연결 등록됨).
- 최근에 열었던 길드는 자동 목록에 표시.

---

## 2. 핵심 개념

| 용어 | 의미 |
|------|------|
| **Guild** | 한 프로젝트 단위. 폴더에 `{name}.guild` 마커 파일로 표시. |
| **Quest** | 개별 이슈 (작업 / 버그 / 요청). 타입 + 번호로 식별 (`DEV-001`, `BUG-045` 등). |
| **Sub-quest** | quest 의 하위 작업. parent quest 가 자동으로 sub 목록 갖춤. |
| **Prerequisite** | 선행 quest. A 의 prereq B → A 시작 전 B 가 필요. |
| **Campaign** | 마일스톤 / 다음 업데이트 묶음. quest 들을 선택적으로 링크. |
| **Status** | quest 의 진행 상태 (open / in_progress / testing / done / on_hold / cancelled / returned). 사용자 정의 가능. |
| **Urgency** | 1=Critical / 2=High / 3=Medium / 4=Low. |

---

## 3. 데이터는 어디에?

`.guild/` 폴더 안:

```
my-app/
├── my-app.guild              ← 길드 마커 (TOML)
└── .guild/
    ├── quests/
    │   ├── DEV-001.md        ← quest 본문 (frontmatter + markdown)
    │   ├── DEV-001.comments.md  ← 공개 댓글 (git 공유)
    │   └── DEV-001.memo.md      ← 비공개 메모 (gitignored)
    ├── campaigns/            ← 캠페인
    ├── types/                ← quest 타입 정의 (DEV / BUG / REQ ...)
    ├── statuses/             ← 상태 정의
    ├── rules/                ← 길드 규칙 문서
    ├── index.db              ← 쿼리 캐시 (자동 재구축 가능)
    └── backups/              ← 자동 백업 (journal + snapshot)
        ├── journal.db
        └── snapshots/
```

### 텍스트 에디터로 직접 열어보세요

각 quest 는 평범한 Markdown 파일. VS Code, Obsidian, GitHub web 등 어디서든
편집·검색 가능합니다. 깊이 들어간 도구 없이도 데이터를 직접 확인하세요.

### git 친화

`.guild/quests/*.md`, `.guild/campaigns/*.md`, `.guild/rules/*.md`, `.guild/types/`,
`.guild/statuses/` 모두 git tracked 권장. diff / blame / branch / PR 자연스럽게
활용 가능.

`.guild/index.db` / `.guild/backups/` / `.guild/positions.json` 은 자동으로
`.gitignore` 처리. 손실되어도 파일에서 재구축 가능.

### git 안 써도 안전

`backups/journal.db` (모든 변경 의도 기록) + `backups/snapshots/*.db` (시점별
사본) 으로 시점 복원 가능. 설정 화면의 "백업 / 복구" 메뉴 사용.

---

## 4. CLI 도구

Windows installer에서는 GUI와 함께 `openguild`(CLI)와
`openguild-server`(HTTP 서버)를 선택 설치할 수 있습니다. "Add to PATH"를
체크하면 명령 프롬프트에서 바로 실행됩니다. macOS는 위 tar.gz를 별도로
설치하며, Linux GUI 패키지에는 CLI와 Server가 포함되지 않습니다.

### 자주 쓰는 명령

```bash
# 현재 폴더의 길드를 자동 탐색해서 실행. 없으면 init 안내.
openguild quest list

# 새 quest
openguild quest new --type DEV --title "API 추가"

# 상태 변경
openguild quest move DEV-001 in_progress

# 댓글 추가 (stdin 으로 본문)
echo "디자인 확정함." | openguild quest comment add DEV-001 --author alice

# 메모 (비공개)
echo "TODO: foo 확인" | openguild quest memo set DEV-001

# 길드 규칙 (top-level 은 `rule` 단수형만, 복수형 alias 없음)
openguild rule list
echo "내용" | openguild rule new branch-policy

# 캠페인
openguild campaign new --title "베타 1.0"
openguild campaign link C-001 DEV-001

# 도서관 폴더 — 옮기면 하위 폴더·문서 경로가 함께 바뀜 (옮겨 갈 부모 폴더는 미리 있어야 함)
openguild library folder new 아키텍처/결정
openguild library folder new 보관
openguild library folder move 아키텍처/결정 보관/결정

# 도서관을 파일 시스템으로 — 폴더 구조 그대로 펼칩니다(제목이 파일 이름).
# 도서관의 폴더는 진짜 디렉터리가 아니라 문서가 적어 둔 값이라, 밖으로 가져갈 때 만들어집니다.
openguild library export ~/내보낸도서관
openguild library export ~/내보낸도서관 --folder 아키텍처
openguild library export ~/내보낸도서관 --id <문서-번호>

# 태그 색·설명 (한글 이름 가능)
openguild tag add 리팩터링 --color "#e94f4f" --description "동작은 그대로"

# 태그로 거르기 — 여러 개면 모두 가진 것만(화면의 태그 줄과 같은 규칙)
openguild quest list --tag backend
openguild quest list --tag backend,api
openguild library list --tag 설계
```

전체 명령은 `openguild --help` / `openguild <명령> --help` 참조.

> **제목은 `--title`, 이름은 위치 인자** — 만들 때 번호가 자동으로 붙는 것
> (`quest new`, `campaign new`, `library new`)은 번호가 식별자이고 제목은 필드
> 하나일 뿐이라 `--title "..."` 로 받습니다. 입력한 이름이 곧 식별자인 것
> (`rule new <slug>`, `template new <이름>`, `library folder new <경로>`,
> `type add <PREFIX>`, `status add <name_en>`, `tag add <slug>`)만 위치 인자입니다.

> **Windows PowerShell 주의**: 위처럼 `echo "한글" | openguild ...` 로
> 파이프하면 콘솔 인코딩 설정에 따라 한글이 깨질 수 있습니다. 깨지면
> UTF-8 파일에 내용을 적어두고 `--file <PATH>` 로 넘기세요 (댓글/메모/
> 규칙/quest 본문 명령 전부 지원).

### 플러그인 (0.6.0)

퀘스트가 생기거나 댓글이 달렸을 때 **내가 만든 프로그램**을 부를 수 있습니다.
설정은 길드 안에 두고 git 으로 공유되지만, **돌릴지 말지는 각자의 기계에서**
정합니다 — 동료가 만든 설정이 `git pull` 만으로 내 컴퓨터에서 실행되면 안
되니까요.

```
.guild/plugins/{이름}/plugin.toml    ← 정의 (git 공유)
.guild/plugins/{이름}/*.rhai         ← 판단·가공 스크립트 (선택)
.guild/plugins/{이름}/*.test.rhai    ← 그 스크립트의 시험 (선택)
~/.openguild/plugin-consent.json     ← 허용 여부 (이 기계에만, git 아님)
```

정의는 **언제 무엇을 할지**를 줄로 적습니다. 한 플러그인이 여러 줄을 가질 수 있습니다.

```toml
#:schema ../plugin.schema.json

name        = "ai-notify"
description = "새 퀘스트와 토론 댓글을 내 서비스로 보냅니다."
scope       = ["cli", "gui"]
scripts     = ["main.rhai"]

# 어디로 나가는지는 **여기에만** 있습니다. 스크립트는 이름만 압니다.
[actions.out.post]
    url     = "https://내-서비스.example/hook"
    headers = { Authorization = "Bearer ${MY_API_KEY}" }

# 줄 1 — 조건만으로 바로 보냅니다(스크립트 없이).
[[handlers]]
    post   = ["quest.created"]
    action = "out"
    [handlers.when]
        "quest.urgency" = [1, 2]

# 줄 2 — 스크립트 함수가 판단하고, 필요하면 `send("out", 본문)` 을 적습니다.
[[handlers]]
    post = ["comment.added"]
    with = ["subject"]          # fn on_comment(e, subject)
    call = "on_comment"
```

줄 하나에 적는 것:

| 칸 | 뜻 |
| --- | --- |
| `post` / `pre` | 언제 볼지. `pre` 는 **바뀌기 전** — 막거나 값을 바꿀 수 있고, 언제나 기다립니다. |
| `call` / `action` | 무엇을 할지. 스크립트 함수를 부르거나, 동작을 바로 실행합니다. |
| `when` | 이 줄이 불릴 조건(전부 만족). `"change.to" = ["done", "closed"]` |
| `with` | 함수가 이벤트 뒤에 받을 연결 데이터 — `subject`, `parent`, `children`, `prereqs`, `campaigns`, `quests`. |
| `wait` | 이 줄이 끝날 때까지 기다립니다(기본은 안 기다림). |
| `id` | 줄 이름 — 화면과 오류 메시지가 이 줄을 가리킬 때 씁니다. |
| `timeout_ms`, `on_timeout`, `on_error` | 시한과, 넘거나 던졌을 때 할 일(`continue` / `block`). |

스크립트는 **밖으로 못 나갑니다.** 파일도 네트워크도 없고, 할 일을 *적기만* 합니다 — 적힌
것은 함수가 끝난 뒤 코어가 실행합니다.

```rhai
fn on_comment(e, subject) {
    if !e.comment.discussion { return; }
    send("out", #{ text: `[${subject.id}] ${e.comment.body}` });   // 동작 이름만 압니다
    notify(`${e.subject.id} 에 토론 댓글`);                         // 권한이 있어야 합니다
}
```

스크립트가 쓸 수 있는 것:

- `send(이름, 본문)` / `run(이름, 입력)` — `[actions]` 의 동작을 부릅니다.
- `notify(글)` / `backup()` — 길드에 시키는 일. **`permissions = ["notify", "backup"]`** 으로
  밝혀야 합니다(허용 화면에 그대로 보입니다).
- `config("키")` — 사용자가 설정 화면에 넣은 값.
- `guild_name()`, `status_name("in_progress")`, `type_name("DEV")`, `link("quest", "DEV-001")`,
  `now()`, `ago(시각)`, `truncate(글, 수)`, `plain_text(마크다운)`.
  - 언어는 골라 쓸 수 있습니다 — `status_name("done", "en")`. 안 주면 이 기계의 언어 설정을
    따릅니다. 두 언어를 다 받으려면 `status_info("done")` → `#{ slug, ko, en, color, done,
    order }`. `done` 은 "완료로 세어지는 상태인가" 라, 슬러그를 외워 박지 않아도 됩니다.
    타입은 파일에 설명 한 줄뿐이라 언어 선택이 없습니다(`type_info("DEV")` 는 색까지 줍니다).
- `import "경로" as 이름` — 여러 플러그인이 공통 함수를 나눠 씁니다(폴더 밖도 됩니다).

`pre` 줄의 함수는 **돌려주는 값**으로 말합니다 — 글자면 막을 이유, 표면 바꿀 칸, `false` 면 그냥
막습니다.

- `description` 은 선택입니다. 적어 두면 데스크톱 관리 → 플러그인 과
  `openguild plugin list` 에 그대로 보입니다 — 나머지 필드는 전부 기계가 읽는
  값이라, 없으면 남이 이 플러그인을 허용할지 정할 때 "무슨 일을 하는가" 를
  알려주는 것이 하나도 없습니다. 500자까지.
- `inputs` 로 **사용자에게 받을 값**을 선언하면 데스크톱 관리 → 플러그인 에
  입력란(텍스트·체크박스·선택상자·숫자)이 뜹니다. 넣은 값은 이 컴퓨터의
  `~/.openguild/plugin-values.json` 에 길드별로 저장되고 git 에는 안 올라갑니다.
  정의에서는 `${KEY}`, 스크립트에서는 `config("KEY")` 로 씁니다.
- `scope` 는 **필수**입니다. `cli` / `gui` / `server` 중에서 고릅니다.
  `server` 를 넣으면 그 서버를 쓰는 **모두**에게 적용됩니다.
- 동작은 `post`(HTTP 로 보내기)와 `run`(프로그램 실행, 이벤트는 stdin) 둘뿐입니다.
- `run` 은 운영체제마다 다른 명령을 적을 수 있습니다 — `windows = { command =
  "powershell", args = [...] }` (`macos`, `linux` 도). 셸 스크립트는 Windows 에서 못 돌므로
  예제 `backup-archive`·`desktop-notify` 는 Windows 용 PowerShell 스크립트를 함께 둡니다.
- `run` 이 **파일을 쓰는 자리**는 `~/.openguild/plugin-data/{길드}/{플러그인}/`
  입니다. 플러그인 폴더가 아닙니다 — 그 폴더는 동의 지문의 대상이라, 훅이
  거기 무언가를 쓰면 동의가 풀려 스스로 꺼집니다. 플러그인과 함께 배포한
  파일을 부르려면 `${OPENGUILD_PLUGIN_DIR}` 를 씁니다.
  슬랙·디스코드 연동 같은 건 그 둘 중 하나로 여러분의 프로그램이 합니다.
- **API 키를 직접 적으면 거부됩니다.** `plugin.toml` 은 git 에 올라가니까요.
  `${MY_API_KEY}` 처럼 환경변수 참조만 쓸 수 있고, 값은 보낼 때 이 기계의
  환경에서 읽습니다.

만들면서 확인하는 것:

```bash
openguild plugin events                        # 볼 수 있는 이벤트와 `with` 목록
openguild plugin check .guild/plugins/내것      # 돌리기 전 검사 (오류가 있으면 종료 코드 1)
openguild plugin test .guild/plugins/내것       # *.test.rhai 의 test_ 함수들
openguild plugin schema --out plugin.schema.json  # 편집기 자동 완성용
```

시험은 나가는 것 없이, 실제와 같은 실행 규칙으로 돕니다.

```rhai
// main.test.rhai
fn test_done_goes_out() {
    set_config("ON_STATUS", true);                       // 이 시험에만 먹습니다
    let r = fire("quest.status_changed",
                 #{ ok: true, quest: #{ id: "DEV-001" }, change: #{ to: "done" } });
    assert_eq(r.commands.len(), 1);
    assert_eq(r.commands[0].action, "out");
}
```

허용은 이렇게 합니다:

```bash
openguild plugin list                    # 무엇이 돌고 무엇이 대기 중인지
openguild plugin allow ai-notify         # 무엇에 동의하는지 내용만 보여줌
openguild plugin allow ai-notify --yes   # 실제로 허용
openguild plugin revoke ai-notify        # 철회 — 다시 허용할 때까지 안 돎
openguild plugin allow --all --yes       # 지금 있는 것 전부 허용
openguild plugin revoke --all            # 지금 있는 것 전부 철회
openguild plugin trust --yes             # 자동 허용 켜기 — 새로 오거나 바뀐 것도 묻지 않음
openguild plugin untrust                 # 자동 허용 끄기 — 돌던 것은 그대로
```

`allow <이름>` 은 그 플러그인이 **무엇을 하는지**를 줄마다 풀어 보여 줍니다 — 언제 무엇을
하는지, 막을 수 있는지, 길드에 무엇을 시키는지, 어디로 나가는지, 어떤 환경변수를 쓰는지
(값은 안 보여 줍니다), 그리고 정의 원문과 스크립트 전체.

전체 허용·전체 해제는 **지금 있는 것들의 상태를 한 번에 바꿀 뿐**이라, 그 뒤에도
하나씩 허용·철회할 수 있습니다. 앞으로 추가되거나 git 으로 바뀌어 오는 플러그인까지
묻지 않으려면 **자동 허용**을 켭니다 — 켜 둔 동안에도 직접 철회한 것은 안 돕니다.

예제는 `examples/plugins/` 에 있습니다 — 텔레그램 알림, 토론 댓글 전달, 데스크톱 알림,
삭제 감사, 그리고 **백업 쌓아 두기**(`backup-archive` — 길드 안의 백업은 7개만 남지만 정해 둔
폴더에는 제한 없이 모입니다).

**길드 밖 폴더의 플러그인도 쓸 수 있습니다.** 플러그인 여럿을 담은 폴더를 **소스**로 등록하고,
그중 이 길드에서 쓸 것을 고릅니다. 복사하지 않고 그 자리에서 읽으므로 원본을 고치면 그대로
반영되고(동의는 다시 묻습니다 — 도는 앱·서버에는 아래 **다시 읽기** 뒤에), 여러 길드가 같은
플러그인을 나눠 씁니다.

```bash
openguild plugin source add ~/dev/my-plugins   # 소스 등록 (이 기계)
openguild plugin available                     # 소스들이 내놓는 것
openguild plugin add backup-archive            # 이 길드에서 쓴다 (돌리려면 allow 가 따로 필요)
openguild plugin add ~/dev/one-plugin          # 폴더 하나면 등록 + 사용까지 한 번에
openguild plugin remove backup-archive         # 안 쓴다 — 파일은 안 지움
openguild plugin source list / remove <이름>
```

`.guild/plugins/` 는 그대로 늘 읽힙니다 — 길드에 딸린 것이고 git 으로 공유됩니다. 소스와
"이 길드에서 쓴다"는 기록은 이 기계(`~/.openguild/`)에만 남습니다. 길드 것과 소스 것의 이름이
겹치면 적재에서 거부하고, 소스 폴더가 사라지면 이유와 함께 목록에 남습니다.

데스크톱 앱에서는 **관리 → 플러그인** 에서 같은 일을 합니다.

- **+ 플러그인 추가** — 폴더를 고릅니다. 플러그인이 하나면 바로 이 길드에서 쓰고, 그 항목으로
  데려가 스크립트를 펼쳐 둡니다 — 무엇에 동의하는지 읽고 그 자리에서 **허용**합니다. 여럿 든
  폴더는 소스로만 등록되고, 아래 **소스 (이 기계)** 목록에서 쓸 것을 고릅니다.
- 목록의 각 플러그인에 **출처**(이 길드 / 소스 이름과 경로)가 보이고, 소스에서 온 것에는
  **이 길드에서 빼기** 가 있습니다(파일은 그대로).
- **소스 해제** 는 그 소스에서 쓰던 것을 이 기계의 모든 길드에서 뺍니다. 폴더는 지우지 않습니다.

**다시 읽기.** 앱과 서버는 길드를 열 때 플러그인을 한 번 읽고 들고 있습니다. `git pull` 이나
직접 편집으로 정의가 바뀌어도 **다시 읽기 전까지는 옛 정의가 돕니다** — 손보는 중인 정의가
저장되는 순간 돌기 시작하지 않도록 일부러 자동으로 읽지 않습니다. 바뀐 정의는 다시 읽을 때
동의가 풀려 멈추고, 다시 허용해야 돕니다.

- 데스크톱: 관리 → 플러그인 → **다시 읽기**. (허용·철회·추가·빼기는 누르는 즉시 다시 읽습니다.)
- 서버: 서버가 있는 기계에서 `openguild plugin reload --remote http://<서버 주소>`.
  **같은 기계에서 온 요청만** 받습니다(루프백, 또는 서버가 묶인 주소 그대로). 서버를 역방향
  프록시 뒤에 같은 기계로 두면 모든 요청이 같은 기계로 보이니, 프록시에서
  `/api/plugins/reload` 를 막으세요.
- CLI: 실행할 때마다 새로 읽으므로 따로 할 일이 없습니다.

**목록은 웹에서도 보입니다** — 브라우저로 서버에 접속해 관리 → 플러그인 을 열면
어떤 훅이 걸려 있고 무엇이 돌고 있는지 볼 수 있습니다. 다만 **허용·철회는 안
됩니다.** 동의는 기계마다 따로 남는 것이고, 팀이 함께 쓰는 주소로 동의를 받으면
"누구의 동의인가" 가 흐려지기 때문입니다. 서버 쪽 허용은 서버가 있는 기계에서
위 CLI 로 하세요.

**플러그인이 자기 자신을 다시 부르지 않습니다.** 훅이 `openguild` 명령으로 길드를 바꾸면 그 변경도
이벤트가 되는데, 그 이벤트는 **그 변경을 일으킨 플러그인에게는 다시 가지 않습니다** — 끝없이 도는
일이 없습니다. 다른 플러그인은 받습니다(예: 훅이 만든 백업을 `backup-archive` 가 복사). 이벤트의
`origin` 에 누가 일으켰는지가 실립니다 — 사람이면 `{"by": "user", "chain": []}`, 플러그인이면
`{"by": "plugin", "chain": ["거쳐 온 플러그인", …]}`. `run` 훅이 부른 `openguild` 는 환경변수
`OPENGUILD_PLUGIN_CHAIN` 으로 이 목록을 이어 받고, `--remote` 로 서버를 부르면 요청 헤더
`X-OpenGuild-Plugin-Chain` 으로 넘깁니다. `post` 훅도 이 헤더를 붙여 보내므로, 받는 쪽이 다시
openguild 서버를 부르는 중계라면 그 헤더를 그대로 전달하세요.

훅이 나가는 동안 명령이 멈추지는 않습니다. 느린 훅 하나가 `openguild comment
add` 를 붙잡지 않도록 따로 보내고, 명령이 끝나기 직전에 잠깐만 기다립니다.
플러그인 하나가 실패해도 길드 동작과 다른 플러그인은 멀쩡합니다.

**바로 쓸 수 있는 예제**가 저장소의 `examples/plugins/` 에 있습니다:

- `telegram-quest-status` — 퀘스트에 `notify` 태그를 붙이면 그 퀘스트의 상태가
  바뀔 때 텔레그램 메시지가 옵니다.
- `desktop-notify` — 퀘스트·댓글이 생기면 데스크톱 알림 (스크립트 없이 `run` 만).
- `deleted-audit` — 퀘스트가 지워지기 **전에** 무엇이 지워질지 기록.

```bash
cp -R examples/plugins/telegram-quest-status /내-길드/.guild/plugins/   # 길드에 넣어 공유하거나
openguild plugin add examples/plugins/telegram-quest-status          # 이 기계에서만 제자리로 쓰거나
openguild plugin allow telegram-quest-status --yes
```

자세한 내용(이벤트 이름 목록, rhai 스크립트, 시한 설정)은
`openguild plugin events` 와 `openguild plugin --help` 를 보세요.

### 원격 모드

서버 모드로 띄운 다른 곳의 길드를 조작하려면:

```bash
openguild --remote https://my-team.example/api quest list
# 또는 환경변수
$env:OPENGUILD_REMOTE = "https://my-team.example/api"
openguild quest list
```

---

## 5. 데스크탑 앱 주요 화면

| 화면 | 용도 |
|------|------|
| **Home** | 진행 중 캠페인 carousel / 마감 임박 / 최근 quest. |
| **Quest List** | 트리 / 평면 quest 목록 + 필터. |
| **Quest Board** | Cytoscape 노드 그래프 (선행 / 서브 관계 시각화). |
| **Quest Detail** | 본문 편집 / 댓글 / 메모 / 상태 변경 / 권장 브랜치명 / 캠페인 링크. |
| **Campaigns** | 캠페인 목록 + 체크리스트 + 링크된 quest. 새 캠페인은 모달, 퀘스트 연결에서 바로 새 퀘스트를 만들어 붙일 수 있습니다. |
| **Rules** | 길드 규칙 다중 파일 편집. |
| **Library** | 문서 + 폴더 트리. 폴더를 끌어 다른 폴더로 옮기거나 이름을 바꾸면 하위 문서 경로가 함께 바뀝니다. |
| **Tags** | 쓰이는 태그 목록. 여기서 태그마다 색·설명을 정합니다(한글 이름도 됨). |
| **관리 (Admin)** | 길드 관리 — 탭 넷: 퀘스트 구성(타입·상태) / 플러그인 / 백업 / 진단(drift·reindex). |
| **Settings** | 이 기계의 개인 설정만 — 정보·업데이트 확인 / 표시 / 편집기. |

---

## 6. 백업 / 복구

- **자동 snapshot**: 변경 50 회 또는 24 시간 마다 자동 (`.guild/backups/snapshots/`).
- **수동 snapshot**: 관리 → 백업 → "+ 새 백업", 또는 `openguild backup new`.
- **복구**: 관리 → 백업 → snapshot 선택 → 복원 (`openguild restore`). 기존
  `index.db` 는 `.pre-restore.db` 로 안전 보관.
- **보관 개수**: 길드 안에는 최근 **7개**만 남고 오래된 것은 지워집니다. 더
  오래 두려면 예제 플러그인 `backup-archive` 를 켜서 길드 밖 폴더에 쌓으세요
  (백업이 생길 때마다 `backup.created` 이벤트로 복사 — 지우지 않습니다).

설치된 사본 위치 기본값:
`C:\Program Files\openguild\` (Windows) — 단 사용자 설정으로 변경 가능.
macOS 는 `/Applications/openguild.app`.

---

## 7. 자동 업데이트

설치된 binary 는 시작 시 GitHub release 의 `latest.json` 을 조회해서 새 버전
감지. Settings → "업데이트 확인" 으로 수동 확인도 가능. 다운로드 → 서명 검증
→ 재시작 흐름은 자동.

---

## 8. 도움말 / 문제 해결

| 상황 | 해결 |
|------|------|
| 파일을 직접 편집했는데 GUI 가 옛 상태 | 시동 시 자동 reindex 합니다. 또는 상단 reindex 버튼. |
| 다른 PC 에서 git pull 후 quest 안 보임 | 위와 동일. 자동 / 수동 reindex. |
| 캐시 손상 의심 | Settings → "Drift 검사" → 필요 시 Reindex. |
| 데이터 손실 의심 | Settings → Admin → 최근 snapshot 복원. |
| CLI 가 인식 안 됨 | 설치 시 "Add to PATH" 체크 안 했을 가능성. 재설치 또는 수동 PATH 등록. |

문의 / 버그 보고: https://github.com/Jirung-E/openguild
