#!/usr/bin/env python3
"""Sample a running probe once a minute for MINUTES: memory, CPU and the song playing.
Usage: longrun.py DIR PID MINUTES   (writes DIR/longrun.jsonl, one line per sample)"""
import json
import subprocess
import sys
import time

DIR, PID, MINUTES = sys.argv[1], sys.argv[2], int(sys.argv[3])
here = __file__.rsplit("/", 1)[0]
start = time.monotonic()
with open(f"{DIR}/longrun.jsonl", "w") as out:
    for i in range(MINUTES + 1):
        while time.monotonic() - start < i * 60:
            time.sleep(1)
        m = json.loads(subprocess.run([sys.executable, f"{here}/measure.py", PID, "5", "--json"], capture_output=True, text=True).stdout)
        with open(f"{DIR}/events.log") as f:
            events = [json.loads(l) for l in f if '"kind":"state"' in l]
        states = [e["message"] for e in events if e.get("event") == "bridge"]
        s = states[-1] if states else {}
        web = sorted((p["pss_kb"] // 1024 for p in m["processes"] if p["name"].startswith("WebKitWeb")), reverse=True)
        out.write(json.dumps({"minute": i, **m["total"], "web_processes_mb": web, "title": s.get("title"), "paused": s.get("paused")}) + "\n")
        out.flush()
