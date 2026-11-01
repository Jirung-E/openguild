#!/bin/sh
# 이벤트 JSON 이 stdin 으로 들어온다.
#
# BUG-279: 작업 디렉터리는 **이 파일이 있는 폴더가 아니다** — 플러그인의
# 데이터 폴더(~/.openguild/plugin-data/…)다. 훅이 출력을 코드 폴더에 쓰면
# 동의 지문이 바뀌어 스스로 꺼지기 때문이다. 코드 폴더가 필요하면
# $OPENGUILD_PLUGIN_DIR 를 쓴다.
#
# 스크립트가 없는 예제 — `payload` 를 안 쓰면 이벤트 JSON 이 그대로 온다.
# 걸러내기는 여기서 해도 된다(rhai 가 유일한 방법은 아니다).
#
# 주의: 이 파일도 동의 지문에 들어간다. 고치면 다시 물어본다.
set -eu
json=$(cat)

# jq 가 있으면 쓰고, 없으면 첫 200자만 보여준다 — 예제가 의존성 때문에
# 안 도는 것보다 낫다.
#
# BUG-309: 제목은 늘 "openguild" 로 시작한다. 맥·리눅스는 보낸 이를 바꿀 수 없다 —
# 알림은 서명된 앱 묶음에 묶여 있어서, 여기서 띄우면 스크립트 실행기가 보낸 것으로
# 잡힌다. 적어도 글에서는 어디서 온 알림인지 바로 보이게 한다. (윈도우는 앱
# 이름표를 쓸 수 있어 notify.ps1 이 진짜 openguild 이름으로 띄운다.)
if command -v jq >/dev/null 2>&1; then
  # 댓글 이벤트에는 quest 가 없다 — 달린 곳은 target 이다.
  title="openguild - $(printf '%s' "$json" | jq -r '.quest.id // .target.id // .event')"
  body=$(printf '%s' "$json" | jq -r 'if .comment then ((.comment.author // "") | if . == "" then "" else . + ": " end) + .comment.body else .quest.title // "" end')
else
  title="openguild"
  body=$(printf '%s' "$json" | cut -c1-200)
fi

case "$(uname -s)" in
  Darwin)
    # BUG-387: 글은 **인자로** 넘긴다 — AppleScript 글자 안에 끼워 넣으면 제목에 `" & (do shell script "…") & "` 를
    # 쓴 사람이 이 기계에서 아무 명령이나 돌릴 수 있었다(길드 내용은 남이 쓴다).
    osascript -e 'on run argv' -e 'display notification (item 1 of argv) with title (item 2 of argv)' -e 'end run' \
      "$body" "$title" ;;
  Linux)
    command -v notify-send >/dev/null 2>&1 && notify-send "$title" "$body" ;;
  *)
    # DEV-425: 훅은 길드 폴더에서 돈다 — 데이터 폴더에 절대경로로.
    printf '%s: %s\n' "$title" "$body" >> "$OPENGUILD_PLUGIN_DATA_DIR/notify.log" ;;
esac
