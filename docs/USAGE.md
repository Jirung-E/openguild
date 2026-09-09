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
```

전체 명령은 `openguild --help` / `openguild <명령> --help` 참조.

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
.guild/plugins/{이름}/plugin.json    ← 설정 (git 공유)
.guild/plugins/{이름}/*.rhai         ← 보낼지 / 어떤 모양으로 보낼지 (선택)
~/.openguild/plugin-consent.json     ← 허용 여부 (이 기계에만, git 아님)
```

```json
{
  "name": "ai-notify",
  "description": "새 퀘스트와 댓글을 내 서비스로 보냅니다.",
  "on": ["quest.created", "comment.added"],
  "scope": ["cli", "gui"],
  "action": {
    "post": {
      "url": "https://내-서비스.example/hook",
      "headers": { "Authorization": "Bearer ${MY_API_KEY}" }
    }
  }
}
```

- `description` 은 선택입니다. 적어 두면 데스크톱 설정 → 플러그인 과
  `openguild plugin list` 에 그대로 보입니다 — 나머지 필드는 전부 기계가 읽는
  값이라, 없으면 남이 이 플러그인을 허용할지 정할 때 "무슨 일을 하는가" 를
  알려주는 것이 하나도 없습니다. 500자까지.
- `scope` 는 **필수**입니다. `cli` / `gui` / `server` 중에서 고릅니다.
  `server` 를 넣으면 그 서버를 쓰는 **모두**에게 적용됩니다.
- 동작은 `post`(HTTP 로 보내기)와 `run`(프로그램 실행, 이벤트는 stdin) 둘뿐입니다.
- `run` 이 **파일을 쓰는 자리**는 `~/.openguild/plugin-data/{길드}/{플러그인}/`
  입니다. 플러그인 폴더가 아닙니다 — 그 폴더는 동의 지문의 대상이라, 훅이
  거기 무언가를 쓰면 동의가 풀려 스스로 꺼집니다. 플러그인과 함께 배포한
  파일을 부르려면 `${OPENGUILD_PLUGIN_DIR}` 를 씁니다.
  슬랙·디스코드 연동 같은 건 그 둘 중 하나로 여러분의 프로그램이 합니다.
- **API 키를 직접 적으면 거부됩니다.** `plugin.json` 은 git 에 올라가니까요.
  `${MY_API_KEY}` 처럼 환경변수 참조만 쓸 수 있고, 값은 보낼 때 이 기계의
  환경에서 읽습니다.

허용은 이렇게 합니다:

```bash
openguild plugin list                    # 무엇이 돌고 무엇이 대기 중인지
openguild plugin allow ai-notify         # 무엇에 동의하는지 내용만 보여줌
openguild plugin allow ai-notify --yes   # 실제로 허용
openguild plugin trust --yes             # 혼자 쓰는 길드면 통째로 허용
openguild plugin untrust                 # 되돌리기 (개별 동의는 남음)
```

데스크톱 앱에서는 **설정 → 플러그인** 에서 같은 일을 합니다.

**목록은 웹에서도 보입니다** — 브라우저로 서버에 접속해 설정 → 플러그인 을 열면
어떤 훅이 걸려 있고 무엇이 돌고 있는지 볼 수 있습니다. 다만 **허용·철회는 안
됩니다.** 동의는 기계마다 따로 남는 것이고, 팀이 함께 쓰는 주소로 동의를 받으면
"누구의 동의인가" 가 흐려지기 때문입니다. 서버 쪽 허용은 서버가 있는 기계에서
위 CLI 로 하세요.

훅이 나가는 동안 명령이 멈추지는 않습니다. 느린 훅 하나가 `openguild comment
add` 를 붙잡지 않도록 따로 보내고, 명령이 끝나기 직전에 잠깐만 기다립니다.
플러그인 하나가 실패해도 길드 동작과 다른 플러그인은 멀쩡합니다.

**바로 쓸 수 있는 예제**가 저장소의 `examples/plugins/` 에 있습니다:

- `telegram-quest-status` — 퀘스트에 `notify` 태그를 붙이면 그 퀘스트의 상태가
  바뀔 때 텔레그램 메시지가 옵니다.
- `desktop-notify` — 퀘스트·댓글이 생기면 데스크톱 알림 (스크립트 없이 `run` 만).
- `deleted-audit` — 퀘스트가 지워지기 **전에** 무엇이 지워질지 기록.

```bash
cp -R examples/plugins/telegram-quest-status /내-길드/.guild/plugins/
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
| **Campaigns** | 캠페인 목록 + 체크리스트 + 링크된 quest. |
| **Rules** | 길드 규칙 다중 파일 편집. |
| **Settings** | 정보 / 업데이트 확인 / 백업 / drift / reindex / 타입·상태 관리. |

---

## 6. 백업 / 복구

- **자동 snapshot**: 변경 50 회 또는 24 시간 마다 자동 (`.guild/backups/snapshots/`).
- **수동 snapshot**: Settings → Admin → "즉시 백업".
- **복구**: Settings → Admin → snapshot 선택 → 복원. 기존 `index.db` 는
  `.pre-restore.db` 로 안전 보관.

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
