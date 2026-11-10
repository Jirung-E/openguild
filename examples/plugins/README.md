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
| `desktop-notify` | 하나 (스크립트 없이) | `run` | 없음 (이벤트 JSON 그대로) | — | post |
| `deleted-audit` | 둘 (하나는 **막는다**) | `run` | 있음 | — | **pre** |
| `backup-archive` | 하나 | `run` | 있음 (경로만 넘김) | — | post |
| `claude-session` | 하나 (**댓글 명령**) | `run` | 있음 | — | **pre** |
| `overdue-digest` | 하나 (**시간마다** — `every` + 함수 안의 `query()`) | `notify`(길드에 시킴), `query`(길드를 읽음) | 있음 | — | **every** |

모든 예제에 **시험 파일**(`main.test.rhai`)이 함께 있다.
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
  스크립트가 환경변수를 못 읽는다 — 그게 rhai 샌드박스의 요점이다. 그래서
  본문에 넣을 값은 정의가 `body_env = { chat_id = "TELEGRAM_CHAT_ID" }` 로
  지목하고 코어가 채운다.
- **리터럴로 적으면 적재가 거부된다.** `.guild/plugins/` 는 git 에 커밋되므로
  토큰을 그대로 적으면 이력에 남는다.

## claude-session

**퀘스트에 `/session` 댓글을 달면 이 컴퓨터의 터미널에서 그 퀘스트의 claude 세션을 연다**(DEV-464).
퀘스트마다 세션을 하나씩 두고 일할 때, 퀘스트 화면에서 바로 그 세션으로 간다.

| 그 이름(퀘스트 번호)의 세션이 | 띄우는 명령 |
|---|---|
| 없으면 | `claude -n DEV-001 "<첫 메시지>"` — 그 이름으로 새로 만들고, 어느 문서의 세션인지 첫 메시지로 알린다 |
| 하나면 | `claude -r <세션 id>` — 그 세션을 잇는다 |
| 여럿이면 | `claude -r DEV-001` — claude 가 그 이름으로 거른 고르기 목록을 띄운다 |

세션은 claude 를 띄운 **폴더마다** 따로 모인다(`~/.claude/projects/…`). 설정의 **세션 폴더**를 비우면
길드 폴더(`.guild` 파일이 있는 곳)다. 이름은 세션 파일마다 **마지막으로 붙은 이름**을 본다 — `/rename` 으로
바꾼 세션은 새 이름으로 찾는다.

새로 만드는 세션에는 설정의 **새 세션 첫 메시지**를 보낸다(`{ID}` 는 문서 번호로). 기본은 "이 세션은 DEV-001 를 위한
세션입니다. 먼저 openguild 로 DEV-001 의 본문과 댓글을 읽고 …" — 그래서 "본문 확인해 봐" 라고만 해도 어느 문서인지 안다.
비우면 보내지 않는다. 이어서 여는 세션에는 보내지 않는다(이미 안다). 이 글은 명령 글자에 끼워 넣지 않고 임시 파일로
건넨다 — 기본값은 커밋되는 `.guild/plugins/` 의 정의에 있어 남이 바꿀 수 있다(BUG-387).

**댓글 명령**의 예다. 바뀌기 전(pre) 줄이 댓글이 저장되기 전에 본문을 보고, 명령이면 `run` 을 맡긴 뒤

- **대신 처리한다**(기본, DEV-465): `#{instead: ...}` 를 돌려준다. 댓글은 남지 않고, 앱에는 오류가 아니라
  “플러그인이 대신 처리했습니다 — 터미널에서 DEV-001 세션을 엽니다” 안내가 뜨고 입력칸이 비워진다. CLI 는 그
  문장을 찍고 정상 종료한다.
- **기록 한 줄로 바꾼다**(설정 `명령 댓글은` → 기록): “터미널에서 `DEV-001` 세션을 열었습니다” 가 댓글로 남는다.

댓글이 남지 않아도 함수가 맡긴 `run` 은 돈다 — 그래서 댓글 대신 터미널을 열 수 있다.

줄에 `command = "${COMMAND}"` 를 적어 두어서(DEV-469) 본문이 그 명령일 때만 돌고, 같은 명령을 잡는 플러그인이
또 있으면 관리 → 플러그인과 `openguild plugin check` 가 경고한다. 둘 다 바뀌기 전 줄이면 **먼저 도는 쪽만**
처리한다(대신 처리하면 거기서 끝난다). 먼저 도는 쪽은 읽은 순서(길드 폴더 이름순, 그 뒤 소스)이고, 정의 맨 위의
`run_after = ["claude-session"]` · `run_before = [...]` 로 바꾼다.

터미널: 맥은 '터미널' 앱(처음 한 번 macOS 가 '제어 허용' 을 묻는다), 리눅스는 `x-terminal-emulator` 또는
`gnome-terminal`, 윈도우는 Windows Terminal(`wt.exe`)이 있으면 그것(안에서 파워셸로 claude 를 띄운다), 없으면 파워셸 창.
서버(`server`)에서는 안 돌린다 — 창이 서버 기계에 뜬다.

`CLAUDE_SESSION_DRY_RUN=1` 이면 창을 열지 않고 띄울 명령만 적는다.

---

## overdue-digest

**하루에 한 번, 기한이 지난 열린 퀘스트를 모아 알림 하나로** 알려 준다 — 시간마다 도는 줄(`every`)의 예(DEV-458).

```toml
[[handlers]]
    id    = "기한 지남"
    every = "1d"
    call  = "digest"
```

- 함수는 받는 것 없이 불린다. `query("quest", "status:any(open, in_progress) due:overdue")` 로 목록을 고르고(`permissions` 에 `query`), 비어 있으면 그냥 끝낸다.
  쿼리는 목록 화면 · `quest list -q` 와 같다.
- 앱에서만 돈다. 알림은 앱 화면의 알림이다 — 서버에서 돌면 서버 로그로만 가서 뺐다.
- `1d` 는 **날짜**로 센다 — 그날 처음 앱을 켜면 약 5초 뒤 돌고, 오늘 안에 다시 켜면 안 돈다. 켜 둔 채면 자정에 돈다(30초마다 확인).
  24시간마다로 하려면 `every = "24h"`. 처음 허용하면 곧 한 번 돌고, 꺼져 있던 동안 놓친 것은 켤 때 한 번만 돈다.
- 설정 `알림에 적을 개수`(기본 5)를 넘으면 "외 N건" 으로 줄인다.
- 시험은 `set_query("quest", "쿼리", [목록])` 로 결과를 정하고 `tick("기한 지남")` 으로 — 길드 없이 그 줄을 돌린다.
