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
| `discussion-to-ai` | 하나 | `run` | 있음 (조건으로 거름) | — (밖으로 안 나감) | post |
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

**미해결 토론 댓글만** 이 컴퓨터에서 돌고 있는 것에 건네준다. 옆 터미널의 AI 세션에게
물어보게 할 때.

밖으로 나가는 것이 없다(BUG-311). 예전에는 웹훅으로 보냈는데, 뜻이 그게 아니었다 —
지금은 `run` 뿐이라 토큰도 필요 없고 권한 화면에 '바깥 통신' 이 안 뜬다.

건네주는 길은 설정에서 고른다. 셋 다 이 컴퓨터 안이다.

| 고르면 | 하는 일 | 같이 적는 것 |
|---|---|---|
| 받은 편지함 (기본) | 파일 뒤에 한 덩이씩 붙인다 — 세션이 그 파일을 지켜보면 된다 | 파일 경로 (비우면 데이터 폴더의 `inbox.md`) |
| 프로그램에 넘기기 | 적어 둔 명령을 띄우고 본문을 그 입에 넣는다 | 예: `claude -p` |
| tmux 창에 넣기 | 돌고 있는 세션 창에 그대로 붙이고 엔터를 친다 | 예: `ai:0.0` |

받은 편지함에 쌓이는 모양:

```
## 2026-09-21 00:37:38

[openguild] quest:BUG-311 · admin
이 방식 맞나요?
```

거르는 것이 이 스크립트의 존재 이유다 — 평범한 댓글까지 보내면 소음이 되고,
AI 를 부르는 훅이면 돈까지 든다. 평범한 댓글에는 **0건**이 나간다. 필터를 만들었으면
"안 보내는 경우" 도 반드시 확인할 것 — 한 번도 거르지 않는 필터는 필터가 아니다.

`comment.unresolved` 도 구독한다. 한 번 해결한 토론이 다시 열리면 그것도 답이
필요한 상태다.

비밀값이 헤더로 가는 예를 찾는다면 텔레그램 예제를 본다 — url 과 `body_env` 를 쓴다.

### 처음부터 끝까지 한 번 (BUG-329)

무엇을 고를 수 있는지만 적어 두면 **어디서 확인해야 하는지**를 모른다. 실제로 해 본다.

```bash
openguild plugin add <저장소>/examples/plugins/discussion-to-ai
openguild plugin allow discussion-to-ai          # '작업 폴더' 경로를 적어 둔다
openguild plugin allow discussion-to-ai --yes

# 토론 댓글을 하나 단다
openguild quest comment add DEV-001 --discussion --file <어떤 파일>
```

작업 폴더에 둘이 생긴다.

```
inbox.md        쌓인 댓글
delivered.log   언제 무엇이 어디로 갔는지 한 줄씩
```

`delivered.log` 가 **갔는지 확인하는 자리**다. '명령 실행' 이나 'tmux 창에 넣기' 로 바꾸면
그쪽은 결과가 눈에 안 보이므로, 이 파일만 본다.

```
2026-09-22 01:54:33  파일  /…/discussion-to-ai/inbox.md
2026-09-22 01:55:02  명령  claude -p
2026-09-22 01:55:40  명령 실패  claude -p
```

명령으로 바꿔 시험할 때는 `cat >> got.txt` 처럼 결과가 남는 것을 먼저 써 본다 — 진짜
프로그램을 걸기 전에 **본문이 제대로 들어가는지**부터 본다.

### AI 가 실제로 답글을 달게 (BUG-331)

이름이 `discussion-to-ai` 지만 기본으로 하는 일은 **건네주기까지**다. 답을 길드에 적는
것은 받는 쪽 몫이다.

> **`claude -p` 만 적으면 아무 일도 안 일어난다**(BUG-335). AI 는 답을 화면으로 말하고
> 끝나는데 그 화면이 없다 — 답이 그냥 사라진다. 게다가 명령은 **성공**했으므로 오류도 안
> 나고, 영수증에도 실패가 아니라 이렇게 찍힌다.
>
> ```
> 2026-09-23 02:30:13  명령  claude -p
> ```
>
> 실제로 이걸로 한참 헤맸다(admin). 답을 **다시 `openguild` 로 넘기는 것까지** 한 줄이
> 해야 한다. 아래가 그 한 줄이다.

'명령 실행' 에 아래 한 줄을 넣으면 한 바퀴가 돈다.

```bash
claude -p "$(cat) — 한국어로 두 문장 이내로 답하라." |
  openguild --guild "$OPENGUILD_GUILD_DIR" quest comment add "$OG_TARGET_ID"     --author ai --parent-id "$OG_COMMENT_ID"
```

세 가지가 이걸 가능하게 한다.

| | |
|---|---|
| `$OPENGUILD_GUILD_DIR` | 훅의 작업 폴더는 **길드 밖**이라([[BUG-279]]) 이게 없으면 `openguild` 가 길드를 못 찾는다 |
| `$OG_TARGET_ID` · `$OG_COMMENT_ID` | 어디에, 무엇의 답글로 달지 |
| 되돌이 막기 | AI 가 단 댓글은 같은 플러그인에 **다시 안 온다**([[DEV-401]]) — 안 그러면 무한이다 |

**되돌이 막기가 어떻게 되는지** 알아 둘 값어치가 있다. '모든 댓글' 로 놓으면 AI 의 답글도
`comment.added` 를 내므로, 막는 것이 없으면 정말로 끝없이 돈다. 막는 것은 환경변수 하나다.

훅의 자식은 `OPENGUILD_PLUGIN_CHAIN` 을 받는다 — 거쳐 온 플러그인 이름이 담긴 JSON 배열이다.

```
["discussion-to-ai"]
```

환경변수라 파이프 건너 손자(`claude -p | openguild …` 의 `openguild`)까지 따라간다. 그
`openguild` 는 시작할 때 이 값을 읽어 두고, 자기가 만드는 이벤트에 실어 보낸다. 받는 쪽은
목록에 자기 이름이 있으면 건너뛴다. 다른 플러그인은 정상으로 받는다.

여기서 헷갈리기 쉬운 것 하나 — **이벤트는 쓰기를 한 그 프로세스가 만든다.** 답글을 쓴 것은
훅의 자식인 CLI 이므로 `comment.added` 도 그 CLI 안에서 만들어지고 거기 꽂힌 플러그인이
받는다. 앱은 남의 프로세스가 쓴 댓글에 대해 이벤트를 내지 않는다.

그래서 **체인이 안 따라가면 막을 것도 없다.** 이런 것은 피한다.

- `env -i` · `env -u OPENGUILD_PLUGIN_CHAIN` 으로 환경을 비우고 부르기
- `nohup … &` 로 떼어 던져 놓고 한참 뒤 다른 경로로 댓글 달기
- 답글을 다른 기계에서 달기 — 그때는 HTTP 헤더 `X-OpenGuild-Plugin-Chain` 이 같은 일을 하므로
  중계가 있으면 그 헤더를 그대로 넘겨야 한다

실제로 돌려 본 결과다.

```
#1  admin ●미해결   파일을 진리원으로 두고 index.db 를 캐시로 쓰는 구조, 이대로 가도 될까요?
#2  ai ↩ #1         네, 로컬 CLI/Git 친화 도구에는 타당한 구조입니다 — …
#3  admin ●미해결   캐시가 깨졌을 때 자동으로 다시 만들려면 무엇을 봐야 하나요?
#4  ai ↩ #3         …
```

**시한을 보라.** `claude -p` 는 짧은 물음에도 5초쯤, 실제 질문이면 15초쯤 걸린다. 기본
10초로는 잘린다 — 이 예제는 동작과 줄 **양쪽에** 60초를 준다(코어가 받아 주는 최대다).
잘리면 `delivered.log` 에도 안 남으므로 조용히 사라진 것처럼 보인다.

**CLI 로 시험한다면 하나 더 있다**(BUG-334). CLI 는 명령이 끝나면 전달을 **2초만** 기다리고
나간다. AI 는 그보다 오래 걸리므로 반드시 이 경고가 뜬다.

```
⚠ 플러그인 전달을 2000ms 까지만 기다렸습니다 — 훅은 계속 돌고 있을 수 있습니다
```

**훅이 죽은 것이 아니다.** 기다리기를 그만둔 것뿐이고 답글은 잠시 뒤에 달린다. 끝까지
보고 싶으면 올린다.

```bash
OPENGUILD_PLUGIN_DRAIN_MS=60000 openguild quest comment add DEV-001 --author admin --file q.md
```

앱과 서버는 계속 살아 있으므로 이 문제가 없다 — **CLI 로 시험할 때만** 걸린다.

#### 밖으로 안 나가게 — ollama (DEV-423)

이 플러그인의 취지가 "밖으로 안 나간다" 이므로, 로컬 모델이 더 어울린다. 공짜이고 빠르다
— 재 보니 `claude -p` 가 15초쯤인 물음에 **1초**였다.

```sh
#!/bin/sh
# ol-reply.sh — 플러그인 폴더 **밖**에 둔다(안에 두면 동의 지문이 바뀐다).
q=$(cat)
printf '%s\n\n한국어로 두 문장 이내로만 답하라.' "$q" \
  | ollama run gemma4:e4b --think=false --nowordwrap 2>/dev/null \
  | openguild --guild "$OPENGUILD_GUILD_DIR" quest comment add "$OG_TARGET_ID" \
      --author gemma --parent-id "$OG_COMMENT_ID"
```

'실행할 명령' 에는 `sh /어디/ol-reply.sh` 만 적는다.

**깃발 셋이 다 필요하다.** 하나라도 빼면 첫 시도가 반드시 깨진다.

| | 왜 |
|---|---|
| `--think=false` | 사고 모델은 답 앞에 추론을 통째로 붙인다. 그게 댓글로 달린다 |
| `--nowordwrap` | 줄바꿈을 다시 그리느라 `ESC[K`(줄 지우기)가 **본문에 섞인다** |
| `2>/dev/null` | 스피너가 stderr 로 나온다 |

모델 이름만 바꾸면 다른 것도 같다.

#### 윈도우에서는 (BUG-332 · BUG-335)

'명령 실행' 은 **파워셸**로 돈다(`deliver.ps1` 이 그렇게 띄운다). 한때 `cmd.exe /c` 였는데
그럴 이유가 없었다 — BUG-332 를 고친 것은 stderr·인코딩·빠진 `OG_*` 변수였지 셸이 아니었고,
파워셸 스크립트 한복판에서 사용자에게만 `%VAR%` 를 쓰게 하는 꼴이었다.

그러니 위 한 줄에서 **`"$VAR"` 를 `$env:VAR` 로만 바꾸면** 된다.

```powershell
claude -p | openguild --guild $env:OPENGUILD_GUILD_DIR quest comment add `
  $env:OG_TARGET_ID --author ai --parent-id $env:OG_COMMENT_ID
```

배치 파일로 뺄 필요도, `%TEMP%` 에 본문을 받아 둘 필요도 없다.

**아직 윈도우에서 안 돌려 봤다.** 이 기계에 파워셸이 없어 코드로만 맞춰 뒀다 — 점검표의
`discussion-to-ai` 항목에서 처음 실제로 돈다.
