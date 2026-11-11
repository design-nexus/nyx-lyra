#!/usr/bin/env bash
# Restart Lyra for the playback and data checks with no window at all (NYX_LYRA_NO_WINDOW),
# muted, with its dev channel and the scratch XDG folders in DIR (default /tmp/lyra-probe).
# Nothing appears on any screen, so no headless output or window rule is involved.
# Usage: tools/run-windowless.sh
set -euo pipefail
dir=${LYRA_PROBE_DIR:-/tmp/lyra-probe}
bin=$(cd "$(dirname "$0")/.." && pwd)/target/release/nyx-lyra
mkdir -p "$dir/xdg/config" "$dir/xdg/data" "$dir/xdg/cache"
cd "$dir"
if pgrep -f "^$bin( |$)" >/dev/null; then
  echo '{"cmd":"quit"}' >>cmds
  sleep 1.5
  pkill -9 -f "^$bin( |$)" || true
fi
pkill -9 -f "tail -n \+1 -f $dir/cmds" || true
[[ -s events.log ]] && mv events.log "events-$(date +%H%M%S).log"
: >cmds
env XDG_CONFIG_HOME="$dir/xdg/config" XDG_DATA_HOME="$dir/xdg/data" XDG_CACHE_HOME="$dir/xdg/cache" \
  NYX_LYRA_APP_ID=io.github.design_nexus.nyx.LyraTest NYX_LYRA_DEV=1 NYX_LYRA_NO_WINDOW=1 NYX_LYRA_MUTE=1 NYX_LYRA_DUMP="$dir" ${LYRA_ENV:-} \
  setsid bash -c "tail -n +1 -f $dir/cmds | exec $bin >$dir/events.log 2>$dir/stderr.log" </dev/null >/dev/null 2>&1 &
for _ in $(seq 1 150); do
  grep -q '"connection":"ready"' events.log 2>/dev/null && { echo "Lyra ready (no window)"; exit 0; }
  sleep 0.1
done
echo "not ready after 15 s"; exit 1
