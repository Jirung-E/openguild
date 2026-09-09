# 예제 플러그인

복사해서 바로 쓰라고 두는 것들이다. 셋 다 다른 축을 보여준다.

```bash
cp -R examples/plugins/telegram-quest-status /내-길드/.guild/plugins/
openguild plugin allow telegram-quest-status        # 내용을 먼저 보여준다
openguild plugin allow telegram-quest-status --yes  # 그러고 나서 허용
```

| | 동작 | 스크립트 | 비밀값 | 시점 |
|---|---|---|---|---|
| `telegram-quest-status` | `post` | 있음 (태그로 거름) | url + `body_env` | post |
| `discussion-to-ai` | `post` | 있음 (조건으로 거름) | 헤더 | post |
| `desktop-notify` | `run` | 없음 (이벤트 JSON 그대로) | — | post |
| `deleted-audit` | `run` | 있음 (모양만 다듬음) | — | **pre** |

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
  본문에 넣을 값은 정의가 `"body_env": { "chat_id": "TELEGRAM_CHAT_ID" }` 로
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
`payload` 가 없으면 이벤트 JSON 이 그대로 stdin 으로 들어오고, 거르는 일은
셸에서 해도 된다. rhai 가 유일한 방법은 아니다.

`scope` 가 `["gui"]` 라 CLI 에서는 안 돈다. 데스크톱에서 일할 때만 뜨는 게
맞기 때문이다 — CLI 로 스무 개를 일괄 처리하는데 알림이 스무 번 뜨면 곤란하다.

`notify.sh` 도 **동의 지문에 들어간다.** 고치면 다시 물어본다 — 정의가 지목해
실행하는 코드이기 때문이다. 그래서 인자가
`${OPENGUILD_PLUGIN_DIR}/notify.sh` 다: 훅의 작업 디렉터리는 이 폴더가
아니라 **데이터 폴더**라, 코드 폴더는 그 환경변수로 가리킨다(아래 참고).

## deleted-audit

**퀘스트가 지워지기 전에 무엇이 지워질지 남긴다.** 관찰 `pre` 의 예다.

```
{"guild":"myguild","id":"DEV-002","status":"open","title":"지워질 퀘스트",
 "ts":"2026-09-08T12:46:40+09:00"}
```

pre 는 **거부하지 못한다.** 훅이 죽으면 길드가 멈추기 때문에 관찰만 하기로 했다.
그래서 이건 막는 장치가 아니라 찾는 단서다 — 복구는 `openguild quest restore`
로 하고, 이 로그는 무엇을 복구할지 찾는 데 쓴다.

pre 를 내는 이벤트는 지금 `quest.deleted` 와 `comment.added` 둘뿐이다. 없는
이벤트에 `pre:` 를 걸면 적재 때 거부된다 — 조용히 안 도는 것보다 낫다.

---

## 직접 만들 때

`openguild plugin events` 로 구독할 수 있는 이름을 본다. 이벤트가 어떻게 생겼는지
보려면 아무거나 하나 걸어 두고 파일로 받아 보는 게 제일 빠르다:

```json
{ "name": "peek", "on": ["*"], "scope": ["cli"],
  "action": { "run": { "command": "sh", "args": ["-c", "cat >> peek.log; echo >> peek.log"] } } }
```

자세한 규칙은 `openguild docs show USAGE` 의 플러그인 절을 본다.

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
