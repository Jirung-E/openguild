#!/bin/sh
# 토론 댓글을 **이 컴퓨터**의 무언가에 건네준다 (BUG-311).
#
# 본문은 stdin 으로 들어온다 — main.rhai 의 send("ai", ...) 가 만든 JSON 이다.
# 설정값(HOW / INBOX_PATH / AI_COMMAND / TMUX_TARGET)은 환경변수로 온다.
#
# 작업 디렉터리는 이 파일이 있는 폴더가 아니라 플러그인의 데이터 폴더다(BUG-279).
# 코드 폴더가 필요하면 $OPENGUILD_PLUGIN_DIR 를 쓴다.
#
# 주의: 이 파일도 동의 지문에 들어간다. 고치면 다시 물어본다.
set -eu
json=$(cat)

# jq 가 있으면 보기 좋게 풀고, 없으면 받은 그대로 넘긴다 — 예제가 의존성 때문에
# 안 도는 것보다 낫다.
if command -v jq >/dev/null 2>&1; then
  target=$(printf '%s' "$json" | jq -r '.target // "?"')
  author=$(printf '%s' "$json" | jq -r '.author // "?"')
  question=$(printf '%s' "$json" | jq -r '.question // ""')
  msg="[openguild] $target · $author
$question"
else
  msg="[openguild] $json"
fi

# BUG-329: 어디로 보냈는지 한 줄 남긴다 — 명령이나 tmux 로 보내면 결과가 눈에 안 보인다.
# 작업 폴더에 쌓이므로 허용 화면의 '작업 폴더' 에서 찾을 수 있다.
receipt() {
  printf '%s  %s  %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" "$2" >> delivered.log
}

case "${HOW:-inbox}" in
  command)
    # 설정에 적은 명령을 띄우고 본문을 그 입에 넣어 준다.
    if [ -z "${AI_COMMAND:-}" ]; then
      echo "discussion-to-ai: '넘길 프로그램' 이 비어 있습니다." >&2
      exit 1
    fi
    if printf '%s\n' "$msg" | sh -c "$AI_COMMAND"; then
      receipt "명령" "$AI_COMMAND"
    else
      receipt "명령 실패" "$AI_COMMAND"
      exit 1
    fi
    ;;

  tmux)
    # 돌고 있는 세션 창에 그대로 넣어 준다. send-keys 로 여러 줄을 보내면 줄마다
    # 엔터가 눌린 것처럼 되므로, 버퍼에 담아 한 번에 붙이고 엔터를 따로 친다.
    if [ -z "${TMUX_TARGET:-}" ]; then
      echo "discussion-to-ai: 'tmux 창' 이 비어 있습니다." >&2
      exit 1
    fi
    printf '%s' "$msg" | tmux load-buffer -
    tmux paste-buffer -d -t "$TMUX_TARGET"
    tmux send-keys -t "$TMUX_TARGET" Enter
    receipt "tmux" "$TMUX_TARGET"
    ;;

  *)
    # 받은 편지함 — 정해 둔 파일 뒤에 한 덩이씩 붙인다. 세션이 이 파일을 지켜보면 된다.
    out=${INBOX_PATH:-}
    [ -n "$out" ] || out="${OPENGUILD_PLUGIN_DATA_DIR:-.}/inbox.md"
    {
      printf '## %s\n\n' "$(date '+%Y-%m-%d %H:%M:%S')"
      printf '%s\n\n' "$msg"
    } >> "$out"
    receipt "파일" "$out"
    ;;
esac
