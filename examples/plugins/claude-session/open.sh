#!/bin/sh
# 퀘스트 번호 이름의 claude 세션을 터미널에서 연다(맥 · 리눅스). 들어오는 것: {"name":"DEV-001"}
#
# - 그 이름의 세션이 하나면 그 세션을 이어서 연다(`claude -r <세션 id>`).
# - 여럿이면 `claude -r 이름` — claude 가 그 이름으로 거른 고르기 목록을 띄운다.
# - 없으면 그 이름으로 새로 만든다(`claude -n 이름`).
#
# 세션은 claude 를 띄운 **폴더마다** 따로 모인다(~/.claude/projects/{폴더 경로의 글자 · 숫자 말고는 - 로}).
# CLAUDE_SESSION_DRY_RUN=1 이면 터미널을 열지 않고 띄울 명령만 적는다(시험용).
set -eu

name=$(sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"\([A-Za-z0-9_-]*\)".*/\1/p' | head -n 1)
if [ -z "$name" ]; then
	echo "claude-session: 세션 이름이 없습니다" >&2
	exit 1
fi

dir=${PROJECT_DIR:-${OPENGUILD_GUILD_DIR:-.}}
dir=$(cd "$dir" && pwd -P)
store="${CLAUDE_CONFIG_DIR:-$HOME/.claude}/projects/$(printf '%s' "$dir" | sed 's/[^A-Za-z0-9]/-/g')"

# 이 이름의 세션 — 세션 파일마다 **마지막으로 붙은 이름**이 이것인 것(이름은 나중에 바꿀 수 있다).
count=0
id=
for f in "$store"/*.jsonl; do
	[ -f "$f" ] || continue
	last=$(grep '"type":"custom-title"' "$f" | tail -n 1 || true)
	case $last in
	*"\"customTitle\":\"$name\""*)
		count=$((count + 1))
		id=$(basename "$f" .jsonl)
		;;
	esac
done

case $count in
0) run="claude -n $name" ;;
1) run="claude -r $id" ;;
*) run="claude -r $name" ;;
esac

# 폴더 이름의 작은따옴표는 셸에 맞게 감싼다.
qdir=$(printf '%s' "$dir" | sed "s/'/'\\\\''/g")
line="cd '$qdir' && $run"

if [ "${CLAUDE_SESSION_DRY_RUN:-}" = 1 ]; then
	echo "$line"
	exit 0
fi

case $(uname -s) in
Darwin)
	# 처음 한 번은 macOS 가 'openguild 가 터미널을 제어하려 합니다' 를 묻는다 — 허용해야 열린다.
	osascript - "$line" <<'OSA'
on run argv
	tell application "Terminal"
		do script (item 1 of argv)
		activate
	end tell
end run
OSA
	;;
*)
	if command -v x-terminal-emulator >/dev/null 2>&1; then
		x-terminal-emulator -e sh -c "$line; exec \$SHELL" >/dev/null 2>&1 &
	elif command -v gnome-terminal >/dev/null 2>&1; then
		gnome-terminal -- sh -c "$line; exec \$SHELL" >/dev/null 2>&1 &
	else
		echo "claude-session: 터미널을 찾지 못했습니다 — 직접 여세요: $line" >&2
		exit 1
	fi
	;;
esac
