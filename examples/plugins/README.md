# 예제 플러그인

복사해서 바로 쓰라고 두는 것들이다. 각각 다른 축을 보여준다.

```bash
cp -R examples/plugins/telegram-quest-status /내-길드/.guild/plugins/
openguild plugin allow telegram-quest-status        # 내용을 먼저 보여준다
openguild plugin allow telegram-quest-status --yes  # 그러고 나서 허용
```

| | 줄 | 동작 | 스크립트 | 비밀값 | 시점 |
|---|---|---|---|---|---|
| `telegram-quest-status` | 셋 (`with` 로 문서를 읽음) | `post` | 있음 (태그로 거름) | url + `body_env` | post |
| `discussion-to-ai` | 하나 | `post` | 있음 (조건으로 거름) | 헤더 | post |
| `desktop-notify` | 하나 (스크립트 없이) | `run` | 없음 (이벤트 JSON 그대로) | — | post |
| `deleted-audit` | 둘 (하나는 **막는다**) | `run` | 있음 | — | **pre** |
| `backup-archive` | 하나 | `run` | 있음 (경로만 넘김) | — | post |

`telegram-quest-status` 와 `deleted-audit` 에는 **시험 파일**(`main.test.rhai`)이 함께 있다.
`openguild plugin test examples/plugins/<이름>` 으로 돌려 본다 — 아무것도 밖으로 안 나간다.

---

## telegram-quest-status

**퀘스트에 `notify` 태그를 붙이면, 그 퀘스트의 상태가 바뀔 때 텔레그램이 온다.**

```bash
export TELEGRAM_BOT_TOKEN=123456:AA...   # @BotFather 에서 받는다
export TELEGRAM_CHAT_ID=987654321        # @userinfobot 에게 물어보면 알려준다
openguild quest tag add DEV-001 notify
```

태그를 떼면 알림도 멈춘다. "알림 설정" 이라는 새 개념을 만드는 대신 이미 있는
태그를 썼다.

실제로 나가는 요청(대역 서버로 확인함):

```
POST /bot123456:AA.../sendMessage
{"chat_id":"987654321","disable_notification":false,
 "text":"[DEV-001] 배포 스크립트 정리\nin_progress → testing"}
```

두 가지가 눈여겨볼 만하다:

- **토큰은 URL 에, chat_id 는 `body_env` 에.** 스크립트에는 I/O 가 없어서
  `payload()` 가 환경변수를 못 읽는다 — 그게 rhai 샌드박스의 요점이다. 그래서
  본문에 넣을 값은 정의가 `body_env = { chat_id = "TELEGRAM_CHAT_ID" }` 로
  지목하고 코어가 채운다.
- **리터럴로 적으면 적재가 거부된다.** `.guild/plugins/` 는 git 에 커밋되므로
  토큰을 그대로 적으면 이력에 남는다.

## discussion-to-ai

**미해결 토론 댓글만** 밖으로 보낸다. AI 에게 물어보게 하거나 담당자에게 알릴 때.

```bash
export DISCUSSION_WEBHOOK_TOKEN=whk_...
```

거르는 것이 이 스크립트의 존재 이유다 — 평범한 댓글까지 보내면 소음이 되고,
AI 를 부르는 훅이면 돈까지 든다. 실제로 나가는 것(대역 서버로 확인함):

```
POST /hook   Authorization: Bearer whk_...
{"author":"kim","quest":"DEV-001","question":"이 방식 맞나요?"}
```

평범한 댓글에는 **0건**이 나간다. 필터를 만들었으면 "안 보내는 경우" 도 반드시
확인할 것 — 한 번도 거르지 않는 필터는 필터가 아니다.

`comment.unresolved` 도 구독한다. 한 번 해결한 토론이 다시 열리면 그것도 답이
필요한 상태다.

비밀값이 **헤더**로 가는 예이기도 하다. 텔레그램 예제는 url 과 `body_env` 를
쓴다 — 셋 다 되고, 서비스가 요구하는 자리에 맞추면 된다.

## desktop-notify

**퀘스트나 댓글이 생기면 데스크톱 알림.** 스크립트 없이 `run` 만 쓰는 예다 —
줄에 `action` 을 적으면 이벤트 JSON 이
그대로 stdin 으로 들어오고, 거르는 일은 셸에서 해도 된다. rhai 가 유일한 방법은 아니다
(조건만 걸 거라면 줄의 `when` 으로도 된다).

`scope` 가 `["gui"]` 라 CLI 에서는 안 돈다. 데스크톱에서 일할 때만 뜨는 게
맞기 때문이다 — CLI 로 스무 개를 일괄 처리하는데 알림이 스무 번 뜨면 곤란하다.

`notify.sh` 도 **동의 지문에 들어간다.** 고치면 다시 물어본다 — 정의가 지목해
실행하는 코드이기 때문이다. 그래서 인자가
`${OPENGUILD_PLUGIN_DIR}/notify.sh` 다: 훅의 작업 디렉터리는 이 폴더가
아니라 **데이터 폴더**라, 코드 폴더는 그 환경변수로 가리킨다(아래 참고).

Windows 에서는 `sh` 대신 `notify.ps1` 을 PowerShell 로 띄운다(정의의 `windows` — 아래
"운영체제마다 다른 명령"). 알림은 트레이 풍선으로 띄우고, Windows 10/11 에서는 토스트로 보인다.

## deleted-audit

**퀘스트가 지워지기 전에 무엇이 지워질지 남기고, `keep` 태그가 붙은 것은 막는다.**
바뀌기 전(`pre`) 단계와 **줄 둘**의 예다.

```
{"guild":"myguild","id":"DEV-002","status":"open","title":"지워질 퀘스트",
 "ts":"2026-09-08T12:46:40+09:00"}
```

pre 줄은 **막을 수 있다.** 함수가 글자를 돌려주면 그것이 막는 이유가 되고(그대로 사용자에게
보인다), 표를 돌려주면 그 칸이 바뀐 채로 진행한다. 아무것도 안 돌려주면 그냥 지나간다.

```bash
openguild quest tag add DEV-002 keep
openguild quest delete DEV-002 --yes
# error: DEV-002 에 keep 태그가 있습니다 — 태그를 떼고 다시 지우세요
```

줄이 둘인 것이 요점이다. **먼저 남기고, 그다음에 막을지 본다** — 막힌 뒤의 줄은 안 돌기
때문에 순서가 반대면 막힌 시도는 기록에 안 남는다. `keep` 태그를 보는 것은 함수가 아니라
줄의 조건(`when`)이라, 함수는 "막는다" 하나만 한다.

지워진 것을 되살리는 것은 여전히 `openguild quest restore` 다(soft delete). 이 로그는
무엇을 복구할지 찾는 데 쓴다.

pre 를 내는 이벤트는 몇 개뿐이다 — `openguild plugin events` 로 확인한다. 없는 이벤트에
`pre` 를 걸면 적재 때 거부된다 — 조용히 안 도는 것보다 낫다.

---

## backup-archive

**백업이 만들어지면 정해 둔 폴더로 한 벌 복사해 둔다.**

길드 안의 백업은 **7개만 남는다** — 새 백업이 생기면 가장 오래된 것이 지워진다
(`.guild/backups/snapshots/`). 자동 백업은 ops 50개 또는 24시간마다 도니, 바쁜 날에는
며칠 전 상태가 이미 없다. 이 훅은 새 백업이 생길 때마다 밖으로 한 벌 복사해 둔다 —
그 폴더에는 개수 제한이 없다.

```bash
cp -R examples/plugins/backup-archive /내-길드/.guild/plugins/
openguild plugin allow backup-archive --yes
printf '%s' /Volumes/backup/openguild | openguild plugin set backup-archive ARCHIVE_DIR
openguild backup new          # 그 폴더에 사본이 생긴다
```

- **쌓아 둘 폴더**(`ARCHIVE_DIR`)를 안 정하면 아무것도 안 한다. 길드 밖의 경로를 권한다 —
  길드 폴더를 통째로 잃어도 사본이 남는다.
- **직접 만든 백업만**(`ONLY_MANUAL`)을 켜면 자동 백업은 건너뛴다.
- 같은 이름이 이미 있으면 덮지 않는다. 복사는 `.part` 로 받아 두었다가 옮기므로, 도중에
  죽어도 반쪽짜리 파일이 남지 않는다.

설정값은 **자식 프로세스의 환경변수로** 온다(`$ARCHIVE_DIR`). `post` 의 url·헤더처럼
`${...}` 로 쓸 수도 있지만, 토큰 같은 값을 `args` 에 넣으면 `ps` 에 보인다 — `run` 훅은
환경변수로 읽는 편이 낫다.

스크립트는 **복사할지 정하고, 경로 한 줄만 내보낸다.** 셸에서 JSON 을
파싱하면 `jq` 가 있느냐에 따라 도는 예제가 되기 때문이다.

Windows 에서는 같은 일을 `archive.ps1` 이 한다 — `ARCHIVE_DIR` 는 `D:\backup\openguild`
처럼 적는다.

---

## 훅이 길드를 바꿔도 자기 자신은 다시 안 불린다 (DEV-401)

`run` 훅이 `openguild quest tag add …` 처럼 길드를 바꾸면 그 변경도 이벤트가 된다. 그 이벤트는
**그 변경을 일으킨 플러그인에게는 다시 가지 않는다** — "태그가 바뀌면 태그를 단다" 가 끝없이 돌지
않는다. 다른 플러그인은 받는다.

- 이벤트의 `origin` 에 누가 일으켰는지가 있다: `{"by": "user", "chain": []}` 또는
  `{"by": "plugin", "chain": ["tagger"]}`. 스크립트에서 `e.origin.by == "user"` 로 사람이 한 것만
  고를 수도 있다.
- 훅 자식은 `OPENGUILD_PLUGIN_CHAIN` 을 받고, 그 안에서 부른 `openguild` 가 이어 받는다. **환경을
  비우고(`env -i`) 부르면 이 보호가 사라진다.**
- `--remote` 로 서버를 바꾸면 `X-OpenGuild-Plugin-Chain` 헤더로 넘어간다. `post` 훅도 이 헤더를
  붙여 보내니, 받은 쪽이 openguild 서버를 다시 부르는 중계라면 헤더를 그대로 전달한다.

## 운영체제마다 다른 명령 (BUG-294)

`run` 의 `command`/`args` 는 모든 OS 의 기본이다. 셸 스크립트는 Windows 에 `sh` 가 없어
못 돈다 — 그 OS 에서 띄울 것을 따로 적는다. 적힌 OS 에서는 기본 대신 그것을 띄운다.

```toml
[actions.archive.run]
    command = "sh"
    args    = ["${OPENGUILD_PLUGIN_DIR}/archive.sh"]

    [actions.archive.run.windows]
        command = "powershell"
        args    = ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass",
                   "-File", "${OPENGUILD_PLUGIN_DIR}/archive.ps1"]
```

- 쓸 수 있는 키는 `windows` / `macos` / `linux`. 시한(`timeout_ms`)은 함께 쓴다.
- 동의 화면에는 **이 기계에서 실제로 띄울 명령**이 보인다. 키 리터럴 검사와 입력란은 다른
  OS 것까지 본다 — git 에 올라가는 것은 정의 전체다.
- `.ps1` 은 **ASCII 로** 쓴다. Windows PowerShell 5.1 은 BOM 없는 스크립트를 시스템 코드
  페이지로 읽어, 한글이 섞이면 파싱이 깨질 수 있다. stdin 은 UTF-8 이므로 스트림을 UTF-8 로
  직접 읽는다(`archive.ps1` 참고) — 그냥 읽으면 한글 경로가 깨진다.
- Windows 에서 띄운 훅에는 콘솔 창이 뜨지 않는다.

## 직접 만들 때

`openguild plugin events` 로 구독할 수 있는 이름을 본다. 이벤트가 어떻게 생겼는지
보려면 아무거나 하나 걸어 두고 파일로 받아 보는 게 제일 빠르다:

```toml
name   = "peek"
scope  = ["cli"]

[actions.peek.run]
    command = "sh"
    args    = ["-c", "cat >> peek.log; echo >> peek.log"]

[[handlers]]
    post   = ["*"]
    action = "peek"
```

만들면서 쓰는 것:

```bash
openguild plugin events                  # 이벤트 이름 · 대상 종류 · 쓸 수 있는 `with`
openguild plugin check <폴더>             # 적재가 하는 검사 + 흔한 실수 (오류면 종료 코드 1)
openguild plugin test <폴더>              # *.test.rhai 의 test_ 함수들
openguild plugin schema --out plugin.schema.json   # 편집기 자동 완성
```

예제 첫 줄의 `#:schema ../plugin.schema.json` 이 그 스키마를 가리킨다 — VS Code 의
Even Better TOML 같은 확장이 집어 들어 칸 이름과 이벤트 이름을 채워 준다.

> 다른 건 다 TOML 인데 이것만 왜 JSON 인가 — **편집기가 그 형식만 읽기 때문**이다.
> 스키마를 적는 표준은 JSON Schema 하나뿐이고, TOML 로 된 표준은 없다. `#:schema` 를
> 알아듣는 Taplo(Even Better TOML 의 속)도 JSON Schema 만 받는다. 이 파일은 사람이
> 손으로 쓰는 설정이 아니라 `openguild plugin schema` 가 찍어 주는 기계용 파일이라,
> 사람이 읽고 쓰는 곳은 그대로 TOML 이다.

자세한 규칙은 `openguild docs show USAGE` 의 플러그인 절을 본다.

## `notify` 태그는 어떻게 되는 건가

`telegram-quest-status` 가 "이 퀘스트만 알림" 을 **평범한 퀘스트 태그**로 고른다.
새 개념도, 새 CLI 옵션도 아니다 — 이미 있는 태그 기능을 그대로 쓴다.

| | |
|---|---|
| GUI | 퀘스트 상세 화면의 태그 줄에서 `+` → `notify` 입력 |
| CLI | `openguild quest tag add DEV-001 notify` |

스크립트는 이벤트에 실려 오는 `e.quest.tags` 를 볼 뿐이다. 태그를 떼면 알림도
멈춘다. 설정에서 "notify 태그가 붙은 퀘스트만" 을 끄면 전부 알린다.

`quest.tags` 가 실제로 채워지게 된 것이 [[DEV-381]] 이다 — 그전에는 항상 빈
배열이라 이 예제가 아예 안 됐다.

**댓글 알림에도 걸린다 — 줄이 문서를 읽기 때문이다.** `comment.added` 이벤트 자체는 어느
문서에 달렸는지(`subject: {kind, id}`)만 알리고 태그는 안 싣는다([[DEV-391]]). 그래서 그 줄에
`with = ["subject"]` 를 적어 두면 코어가 그 문서를 읽어 함수의 둘째 인자로 넘긴다 —
`fn on_comment(e, doc)` 안에서 `doc.tags` 를 본다([[DEV-405]]).

## 사용자에게 받는 값 (REQ-021)

`telegram-quest-status` 가 그 예다. 봇 토큰·채팅 ID 는 사람이 넣어야 하고,
"무엇을 알릴지" 는 켜고 끌 수 있어야 한다.

```toml
[[inputs]]
    key    = "TELEGRAM_BOT_TOKEN"
    label  = "봇 토큰"
    secret = true

[[inputs]]
    key     = "ON_COMMENT"
    label   = "댓글 알림"
    type    = "checkbox"
    default = false
```

관리 → 플러그인 에서 채우고, 스크립트는 `config("ON_COMMENT")` 로 읽는다 —
체크박스는 **bool 로** 온다. 정의 안에서는 `${TELEGRAM_BOT_TOKEN}` 으로 쓴다.

터미널에서는:

```bash
echo -n <값> | openguild plugin set telegram-quest-status TELEGRAM_BOT_TOKEN
openguild plugin config telegram-quest-status
```

## 훅이 파일을 쓰는 자리 (BUG-279)

`run` 훅의 작업 디렉터리는 **플러그인 폴더가 아니다.**

```
~/.openguild/plugin-data/{길드}/{플러그인}/     ← 여기서 돈다. 쓰는 곳.
.guild/plugins/{플러그인}/                      ← 코드. 지문의 대상. 쓰면 안 된다.
```

전에는 플러그인 폴더에서 돌았는데, 동의 지문이 **그 폴더의 모든 파일**을
보기 때문에 훅이 로그 하나만 남겨도 지문이 바뀌어 **스스로 동의를 깼다.**
한 번 돌고 그 뒤로 조용히 안 도는, 알아채기 어려운 종류였다.

두 경로는 환경변수로 온다:

| | |
|---|---|
| `OPENGUILD_PLUGIN_DIR` | 코드 폴더 — 옆 파일을 부를 때 |
| `OPENGUILD_PLUGIN_DATA_DIR` | 데이터 폴더 — 작업 디렉터리와 같다 |

정의의 `command` · `args` 안에서도 `${OPENGUILD_PLUGIN_DIR}` 로 쓸 수 있다.
`sh -c` 를 안 거치는 명령(`python ${OPENGUILD_PLUGIN_DIR}/hook.py`)도 되라고
그렇게 했다.

`post` 는 작업 디렉터리가 없으므로 해당 없다.
