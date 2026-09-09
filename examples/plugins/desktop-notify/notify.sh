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
