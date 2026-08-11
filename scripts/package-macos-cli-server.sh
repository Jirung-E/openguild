#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
release_dir="${1:-$repo_root/target/release}"

read_package_version() {
  awk -F'"' '/^version = "/ { print $2; exit }' "$1"
}

cli_version="$(read_package_version "$repo_root/cli/Cargo.toml")"
server_version="$(read_package_version "$repo_root/server/Cargo.toml")"

if [[ -z "$cli_version" || -z "$server_version" ]]; then
  echo "CLI/Server version을 Cargo.toml에서 읽지 못했습니다." >&2
  exit 1
fi

if [[ "$cli_version" != "$server_version" ]]; then
  echo "CLI($cli_version)와 Server($server_version) 버전이 다릅니다." >&2
  exit 1
fi

for binary in openguild openguild-server; do
  if [[ ! -x "$release_dir/$binary" ]]; then
    echo "실행 가능한 $release_dir/$binary 파일이 없습니다." >&2
    exit 1
  fi
done

package_name="openguild_${cli_version}_macos_arm64_cli-server"
stage_root="$(mktemp -d "${TMPDIR:-/tmp}/openguild-macos-package.XXXXXX")"
trap 'rm -rf -- "$stage_root"' EXIT

package_dir="$stage_root/$package_name"
mkdir -p "$package_dir"
install -m 755 "$release_dir/openguild" "$package_dir/openguild"
install -m 755 "$release_dir/openguild-server" "$package_dir/openguild-server"
install -m 644 \
  "$repo_root/packaging/macos/cli-server-README.md" \
  "$package_dir/README.md"

archive="$release_dir/$package_name.tar.gz"
checksum="$archive.sha256"
tar -C "$stage_root" -czf "$archive" "$package_name"

archive_name="$(basename "$archive")"
checksum_name="$(basename "$checksum")"
(
  cd "$release_dir"
  shasum -a 256 "$archive_name" > "$checksum_name"
)

echo "created: $archive"
echo "created: $checksum"
