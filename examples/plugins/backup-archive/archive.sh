#!/bin/sh
# 백업 파일 한 개를 `ARCHIVE_DIR` 로 복사한다.
#
# stdin 으로는 `main.rhai` 가 `run("archive", …)` 에 넘긴 값이 온다 — 스냅샷 **경로**
# 한 줄(JSON 문자열이라 따옴표가 붙는다). 셸에서 JSON 을 파싱하지 않으려고
# 스크립트 쪽에서 모양을 줄여 놓았다.
#
# BUG-279: 작업 디렉터리는 이 파일이 있는 폴더가 아니라 플러그인 데이터 폴더다.
# 설정값(ARCHIVE_DIR 등)은 환경변수로 들어온다.
set -eu

src=$(cat | tr -d '"')
[ -n "$src" ] || exit 0
[ -n "${ARCHIVE_DIR:-}" ] || { echo "ARCHIVE_DIR 가 비어 있습니다" >&2; exit 1; }
[ -f "$src" ] || { echo "백업 파일이 없습니다: $src" >&2; exit 1; }

mkdir -p "$ARCHIVE_DIR"
# 같은 이름이 이미 있으면 그대로 둔다 — 이미 쌓아 둔 사본을 덮지 않는다.
#
# `[ -e "$dest" ] && exit 0` 로 쓰면 안 된다: `set -e` 에서 그 목록은 조건이
# 아니라 마지막 명령이라, 파일이 **없을 때** 스크립트가 1 로 죽는다(실제로 밟았다).
dest="$ARCHIVE_DIR/$(basename "$src")"
if [ -e "$dest" ]; then
  exit 0
fi

# 먼저 임시 이름으로 복사한 뒤 옮긴다 — 복사 도중에 죽어도 반쪽짜리 파일이
# 쌓이지 않는다(같은 폴더 안의 rename 은 원자적이다).
tmp="$dest.part"
cp "$src" "$tmp"
mv "$tmp" "$dest"
