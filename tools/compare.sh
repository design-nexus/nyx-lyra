#!/usr/bin/env bash
# Put the running test copy (tools/headless.sh, LYRA=app) beside a mockup shot.
# Usage: tools/compare.sh SCREEN [DEV-COMMAND-JSON…]
#   SCREEN is a shot in nyx-design/mockups/lyra/shots without its theme (directions-a,
#   screens-library, players-expanded-a, …); the commands drive the copy there first.
# Writes DIR/compare-SCREEN-TIME.png (the mockup on the left, Lyra on the right, both
# 1280×800) and DIR/app-SCREEN-TIME.png, and prints their names.
# Needs ImageMagick (magick) and grim. DIR is LYRA_PROBE_DIR (default /tmp/lyra-probe).
set -euo pipefail
screen=$1
shift
dir=${LYRA_PROBE_DIR:-/tmp/lyra-probe}
theme=${LYRA_THEME:-dark}
root=$(cd "$(dirname "$0")/.." && pwd)
mock=$root/../nyx-design/mockups/lyra/shots/$screen-$theme.png
[[ -f $mock ]] || { echo "no mockup shot $mock"; exit 1; }
source "$dir/output.env"
for c in "$@"; do
  echo "$c" >>"$dir/cmds"
  sleep "${LYRA_STEP:-2}"
done
sleep 1
# The headless output only draws when captured: the first capture wakes it.
stamp=$(date +%H%M%S)
app=$dir/app-$screen-$stamp.png
grim -o "$out" "$app"
sleep 0.5
grim -o "$out" "$app"
magick "$mock" -resize 1280x800\! \( "$app" -resize 1280x800\! \) +append \
  -gravity north -background '#000' -splice 0x28 -fill white -pointsize 18 \
  -annotate +0+4 "mockup: $screen ($theme)                                                                 Lyra" \
  "$dir/compare-$screen-$stamp.png"
echo "$app $dir/compare-$screen-$stamp.png"
