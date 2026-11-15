#!/usr/bin/env bash
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
output="$(mktemp -d)"
trap 'rm -rf "$output"' EXIT
python3 "$root/scripts/base-latin/build.py" --output "$output" --web-output "$output/web"
for face in Regular Bold Italic BoldItalic; do
  cmp "$root/crates/base-latin/fonts/NimbleOfficeBaseLatin-$face.ttf" "$output/NimbleOfficeBaseLatin-$face.ttf"
  cmp "$root/packages/fonts/assets/NimbleOfficeBaseLatin-$face.ttf.zst" "$output/web/NimbleOfficeBaseLatin-$face.ttf.zst"
done
cmp "$root/packages/fonts/assets/base-latin.json" "$output/web/base-latin.json"
