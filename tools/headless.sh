#!/usr/bin/env bash
# Restart the probe on this session's headless output, muted, with the scratch XDG folders
# in DIR (default /tmp/lyra-probe), and wait for its window. DIR/output.env holds the output
# (out=, ws=) made at the start of the session; DIR/cmds and DIR/events.log are its pipes.
# Usage: tools/headless.sh [URL]
set -euo pipefail
dir=${LYRA_PROBE_DIR:-/tmp/lyra-probe}
bin=$(cd "$(dirname "$0")/.." && pwd)/target/release/nyx-lyra-probe
source "$dir/output.env"
cd "$dir"
if pgrep -f "^$bin" >/dev/null; then
  echo '{"cmd":"quit"}' >>cmds
  sleep 1.5
  pkill -9 -f "^$bin" || true
fi
pkill -9 -f "tail -n \+1 -f $dir/cmds" || true
[[ -s events.log ]] && mv events.log "events-$(date +%H%M%S).log"
: >cmds
# A config reload clears runtime rules and moves the output, so set both again.
hyprctl eval "hl.monitor({ output = \"$out\", mode = \"1280x800\", position = \"-20000x-20000\", scale = 1 })" >/dev/null
hyprctl eval "hl.window_rule({ name = \"nyx-test-lyra\", match = { class = \"^io\\\\.github\\\\.design_nexus\\\\.nyx\\\\.LyraProbeTest\$\" }, workspace = \"$ws silent\", no_initial_focus = true, suppress_event = \"activate activatefocus\" })" >/dev/null
x=$(hyprctl monitors -j | python3 -c "import json,sys; print([m['x'] for m in json.load(sys.stdin) if m['name']=='$out'][0])")
[[ $x == -20000 ]] || { echo "output $out isn't at -20000 ($x)"; exit 1; }
XDG_CONFIG_HOME=$dir/xdg/config XDG_DATA_HOME=$dir/xdg/data XDG_CACHE_HOME=$dir/xdg/cache \
  LYRA_PROBE_APP_ID=io.github.design_nexus.nyx.LyraProbeTest LYRA_PROBE_MUTE=1 \
  setsid bash -c "tail -n +1 -f $dir/cmds | exec $bin ${1:-} >$dir/events.log 2>$dir/stderr.log" </dev/null >/dev/null 2>&1 &
mon=$(hyprctl monitors -j | python3 -c "import json,sys; print([m['id'] for m in json.load(sys.stdin) if m['name']=='$out'][0])")
for _ in $(seq 1 100); do
  r=$(hyprctl clients -j | python3 -c "
import json,sys
for c in json.load(sys.stdin):
    if c['class'].endswith('LyraProbeTest'): print(c['pid'], c['monitor'])")
  if [[ -n $r ]]; then
    read -r pid m <<<"$r"
    if [[ $m != "$mon" ]]; then kill -9 "$pid"; echo "window opened on monitor $m: killed"; exit 1; fi
    echo "probe $pid on $out"
    exit 0
  fi
  sleep 0.1
done
echo "no window"; exit 1
