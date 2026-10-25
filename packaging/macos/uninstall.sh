#!/usr/bin/env bash
# DEV-327: openguild 지우기 (macOS). **길드(.guild 가 든 폴더)는 사용자 문서다 — 절대 건드리지 않는다.**
#
#   ./uninstall.sh              CLI · 서버(/usr/local/bin 의 openguild, openguild-server)만
#   ./uninstall.sh --app        + 데스크톱 앱(/Applications/openguild.app)과 앱이 복사해 둔 문서 · 스킬
#   ./uninstall.sh --settings   + 이 기계의 설정(최근 길드 · 언어 · 테마 · 플러그인 허용 · 플러그인 설정값 ·
#                                 서버 설정)과 앱 데이터(~/Library/… io.openguild.desktop)
#   --yes                       묻지 않고 지운다
#
# 남기는 것: ~/.openguild/plugin-data — 플러그인이 남긴 기록(삭제 감사 로그 · 받은 편지함 등)은 사용자가 만든
# 자료다. 필요 없으면 직접 지운다.
#
# 시험용: OPENGUILD_UNINSTALL_BIN · OPENGUILD_UNINSTALL_APPS 로 /usr/local/bin · /Applications 를 바꾼다.
# ~/.openguild 는 OPENGUILD_HOME, ~/Library 는 HOME 을 따른다.

set -euo pipefail

bin_dir="${OPENGUILD_UNINSTALL_BIN:-/usr/local/bin}"
apps_dir="${OPENGUILD_UNINSTALL_APPS:-/Applications}"
og_home="${OPENGUILD_HOME:-$HOME/.openguild}"
bundle_id="io.openguild.desktop"

want_app=0
want_settings=0
assume_yes=0
for arg in "$@"; do
  case "$arg" in
    --app) want_app=1 ;;
    --settings) want_settings=1 ;;
    --yes | -y) assume_yes=1 ;;
    -h | --help)
      sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "모르는 옵션: $arg (--app, --settings, --yes)" >&2
      exit 2
      ;;
  esac
done

targets=()

# CLI · 서버 — 이름만 같은 다른 프로그램을 지우지 않게, 버전 줄이 openguild 인지 본다.
for name in openguild openguild-server; do
  path="$bin_dir/$name"
  [[ -e "$path" ]] || continue
  if "$path" --version 2>/dev/null | grep -q "^$name "; then
    targets+=("$path")
  else
    echo "건너뜀: $path — openguild 의 실행 파일이 아닌 것 같습니다" >&2
  fi
done

if [[ $want_app == 1 ]]; then
  [[ -e "$apps_dir/openguild.app" ]] && targets+=("$apps_dir/openguild.app")
  for p in "$og_home/skill-marketplace" "$og_home/docs"; do
    [[ -e "$p" ]] && targets+=("$p")
  done
fi

if [[ $want_settings == 1 ]]; then
  for f in recents.json locale.json themes.json plugin-consent.json plugin-values.json \
    plugin-sources.json openguild-server.toml panic.log clipboard; do
    [[ -e "$og_home/$f" ]] && targets+=("$og_home/$f")
  done
  for p in "$HOME/Library/Application Support/$bundle_id" "$HOME/Library/Caches/$bundle_id" \
    "$HOME/Library/WebKit/$bundle_id" "$HOME/Library/Saved Application State/$bundle_id.savedState"; do
    [[ -e "$p" ]] && targets+=("$p")
  done
fi

if [[ ${#targets[@]} == 0 ]]; then
  echo "지울 것이 없습니다."
  exit 0
fi

echo "다음을 지웁니다:"
printf '  %s\n' "${targets[@]}"
if [[ -d "$og_home/plugin-data" ]]; then
  echo "남깁니다: $og_home/plugin-data (플러그인이 남긴 기록)"
fi
echo "길드 폴더는 건드리지 않습니다."

if [[ $assume_yes != 1 ]]; then
  read -r -p "지울까요? [y/N] " answer
  case "$answer" in
    y | Y | yes | YES) ;;
    *)
      echo "취소했습니다."
      exit 1
      ;;
  esac
fi

for t in "${targets[@]}"; do
  if [[ -w "$(dirname "$t")" ]]; then
    rm -rf -- "$t"
  else
    sudo rm -rf -- "$t"
  fi
done

# ~/.openguild 가 비었으면 그것도 — 남은 게 있으면(plugin-data 등) 그대로.
rmdir "$og_home" 2>/dev/null || true
echo "지웠습니다."
