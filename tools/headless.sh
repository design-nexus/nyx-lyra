#!/usr/bin/env bash
# Restart a test copy on this session's headless output, muted, with the scratch XDG folders
# in DIR (default /tmp/lyra-probe). DIR/output.env holds the output (out=, ws=) made at the
# start of the session; DIR/cmds and DIR/events.log are its pipes.
# Usage: tools/headless.sh [URL]          the probe (LyraProbeTest)
#        LYRA=app tools/headless.sh       Lyra itself with its dev channel (LyraTest)
#
# Lyra starts with its window built but not shown; only once the page is ready are the
# output's position and the window rule set and checked, and then the window is shown, so a
# config reload (which clears runtime rules and moves the output) has a tenth of a second to
# land in, not the whole start-up. The window is watched every 20 ms and killed at once if
# it appears anywhere but the headless output.
set -euo pipefail
dir=${LYRA_PROBE_DIR:-/tmp/lyra-probe}
root=$(cd "$(dirname "$0")/.." && pwd)
if [[ ${LYRA:-probe} == app ]]; then
  bin=$root/target/release/nyx-lyra class=LyraTest
  envs=(NYX_LYRA_APP_ID=io.github.design_nexus.nyx.LyraTest NYX_LYRA_DEV=1 NYX_LYRA_NO_WINDOW=1 NYX_LYRA_MUTE=1 NYX_LYRA_DUMP=$dir ${LYRA_ENV:-})
else
  bin=$root/target/release/nyx-lyra-probe class=LyraProbeTest
  envs=(LYRA_PROBE_APP_ID=io.github.design_nexus.nyx.LyraProbeTest LYRA_PROBE_MUTE=1 LYRA_PROBE_DUMP=$dir ${LYRA_ENV:-})
fi
source "$dir/output.env"
cd "$dir"
for b in "$root/target/release/nyx-lyra" "$root/target/release/nyx-lyra-probe"; do
  if pgrep -f "^$b( |$)" >/dev/null; then
    echo '{"cmd":"quit"}' >>cmds
    sleep 1.5
    pkill -9 -f "^$b( |$)" || true
  fi
done
pkill -9 -f "tail -n \+1 -f $dir/cmds" || true
[[ -s events.log ]] && mv events.log "events-$(date +%H%M%S).log"
: >cmds

rules() {
  hyprctl eval "hl.monitor({ output = \"$out\", mode = \"1280x800\", position = \"-24000x-20000\", scale = 1 })" >/dev/null
  hyprctl eval "hl.window_rule({ name = \"nyx-test-$class\", match = { class = \"^io\\\\.github\\\\.design_nexus\\\\.nyx\\\\.$class\$\" }, workspace = \"$ws silent\", no_initial_focus = true, suppress_event = \"activate activatefocus\" })" >/dev/null
  local x
  x=$(hyprctl monitors -j | python3 -c "import json,sys; print([m['x'] for m in json.load(sys.stdin) if m['name']=='$out'][0])")
  [[ $x == -24000 ]] || { echo "output $out isn't at -24000 ($x)"; exit 1; }
}
mon=$(hyprctl monitors -j | python3 -c "import json,sys; print([m['id'] for m in json.load(sys.stdin) if m['name']=='$out'][0])")
watch_window() {
  for _ in $(seq 1 250); do
    r=$(hyprctl clients -j | python3 -c "
import json,sys
for c in json.load(sys.stdin):
    if c['class'].endswith('.$class'): print(c['pid'], c['monitor'])")
    if [[ -n $r ]]; then
      read -r pid m <<<"$r"
      if [[ $m != "$mon" ]]; then kill -9 "$pid"; echo "window opened on monitor $m: killed"; exit 1; fi
      echo "$class $pid on $out"
      return 0
    fi
    sleep 0.02
  done
  echo "no window"; exit 1
}

[[ $class == LyraProbeTest ]] && rules
# The real data folder stays on the search path so the icon theme (Umbra) is found.
env XDG_CONFIG_HOME="$dir/xdg/config" XDG_DATA_HOME="$dir/xdg/data" XDG_CACHE_HOME="$dir/xdg/cache" \
  XDG_DATA_DIRS="$HOME/.local/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}" "${envs[@]}" \
  setsid bash -c "tail -n +1 -f $dir/cmds | exec $bin ${1:-} >$dir/events.log 2>$dir/stderr.log" </dev/null >/dev/null 2>&1 &
if [[ $class == LyraTest ]]; then
  for _ in $(seq 1 150); do grep -q '"connection":"ready"' events.log 2>/dev/null && break; sleep 0.1; done
  rules
  echo '{"cmd":"show"}' >>cmds
fi
watch_window
