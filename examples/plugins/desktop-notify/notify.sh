#!/bin/sh
# 이벤트 JSON 이 stdin 으로 들어온다. 작업 디렉터리는 이 플러그인 폴더다.
#
# 스크립트가 없는 예제 — `payload` 를 안 쓰면 이벤트 JSON 이 그대로 온다.
# 걸러내기는 여기서 해도 된다(rhai 가 유일한 방법은 아니다).
#
# 주의: 이 파일도 동의 지문에 들어간다. 고치면 다시 물어본다.
set -eu
json=$(cat)

# jq 가 있으면 쓰고, 없으면 첫 200자만 보여준다 — 예제가 의존성 때문에
# 안 도는 것보다 낫다.
if command -v jq >/dev/null 2>&1; then
  title=$(printf '%s' "$json" | jq -r '.event')
  body=$(printf '%s' "$json" | jq -r '(.quest.id // "?") + " " + (.quest.title // .comment.body // "")')
else
  title="openguild"
  body=$(printf '%s' "$json" | cut -c1-200)
fi

case "$(uname -s)" in
  Darwin)
    osascript -e "display notification \"$body\" with title \"$title\"" ;;
  Linux)
    command -v notify-send >/dev/null 2>&1 && notify-send "$title" "$body" ;;
  *)
    printf '%s: %s\n' "$title" "$body" >> notify.log ;;
esac
