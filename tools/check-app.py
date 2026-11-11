#!/usr/bin/env python3
"""Check Lyra through its dev channel (LYRA=app tools/headless.sh first): every command is
confirmed by the page, the data requests parse, and a crash comes back to the same song.
Usage: check-app.py [DIR]"""
import json
import sys
import time

DIR = sys.argv[1] if len(sys.argv) > 1 else "/tmp/lyra-probe"
results = []


def events(since=0):
    with open(f"{DIR}/events.log") as f:
        lines = f.readlines()[since:]
    out = []
    for line in lines:
        try:
            out.append(json.loads(line))
        except ValueError:
            pass
    return out


def mark():
    with open(f"{DIR}/events.log") as f:
        return sum(1 for _ in f)


def send(c):
    with open(f"{DIR}/cmds", "a") as f:
        f.write(json.dumps(c) + "\n")


def wait(pred, timeout):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.2)
    return None


def state():
    s = [e["state"] for e in events() if e.get("event") == "state"]
    return s[-1] if s else {}


def check(name, ok, detail=""):
    results.append(bool(ok))
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


def command(c, timeout=15):
    """Send a command and wait for Lyra's own verdict: confirmed or failed."""
    m = mark()
    t0 = time.monotonic()
    send(c)
    sent = wait(lambda: next((e for e in events(m) if e.get("event") == "sent"), None), 5)
    if not sent:
        return None, 0
    v = wait(lambda: next((e for e in events(m) if e.get("event") in ("confirmed", "failed") and e.get("id") == sent["id"]), None), timeout)
    return v, round((time.monotonic() - t0) * 1000)


def data(c, timeout=15):
    m = mark()
    send(c)
    return wait(lambda: next((e for e in events(m) if e.get("event") == "data"), None), timeout)


# A song, then every command.
send({"cmd": "watch", "video": "dX3k_QDnzHE"})
s = wait(lambda: (lambda s: s if (s.get("track") or {}).get("video_id") == "dX3k_QDnzHE" and s.get("duration") and not s.get("error") else None)(state()), 25)
check("loads a song", s, s and f"{s['track']['title']!r} by {s['track']['artist']!r}, {s['status']}")
t0 = time.monotonic()
send({"cmd": "play"})
b = wait(lambda: (lambda s: s if s.get("status") == "playing" and (s.get("position") or 0) >= 1 and not s.get("error") else None)(state()), 20)
check("actually plays (position moves, no error)", b, f"audible after {time.monotonic() - t0:.1f} s" if b else "not within 20 s")
for c, name in [({"cmd": "play"}, "play"), ({"cmd": "pause"}, "pause"), ({"cmd": "toggle"}, "toggle (resumes)"),
                ({"cmd": "seek", "position": 120}, "seek to 2:00"), ({"cmd": "volume", "level": 40}, "volume 40"),
                ({"cmd": "volume", "level": 100}, "volume 100")]:
    v, ms = command(c)
    check(name, v and v["event"] == "confirmed", f"{v and v['event']} in {ms} ms" + (f" ({v.get('failure')})" if v and v.get("failure") else ""))
before = state()["track"]["title"]
v, ms = command({"cmd": "next"})
check("next", v and v["event"] == "confirmed", f"{before!r} -> {state()['track']['title']!r} in {ms} ms")
v, ms = command({"cmd": "previous"})
if v and v["event"] == "confirmed" and state()["track"]["title"] != before:
    v, ms = command({"cmd": "previous"})
check("previous", v and v["event"] == "confirmed", f"now {state()['track']['title']!r}, {ms} ms")
v, _ = command({"cmd": "volume", "level": 300})
check("bad volume fails cleanly", v and v["event"] == "failed", v and v.get("failure"))

# Data from the playing page.
for c, name, need in [({"cmd": "search", "query": "daft punk"}, "search", "shelves"), ({"cmd": "browse", "id": "FEmusic_home"}, "home", "shelves"),
                      ({"cmd": "browse", "id": "FEmusic_liked_playlists"}, "library playlists", "shelves"),
                      ({"cmd": "browse", "id": "VLLM"}, "liked music", "shelves"), ({"cmd": "up-next", "video": "dX3k_QDnzHE"}, "up next", "queue")]:
    d = data(c)
    ok = d and not d.get("error") and d.get(need)
    detail = d and (d.get("error") or f"{d.get('ms')} ms, " + (", ".join(f"{s['title'] or '-'}: {s['items']}" for s in d["shelves"]) if need == "shelves" else f"{d['queue']} queued"))
    check(name, ok, detail)
a = state().get("position") or 0
time.sleep(5)
s = state()
check("still playing through the requests", s.get("status") == "playing" and (s.get("position") or 0) > a + 3, f"{s.get('status')}, {a} -> {s.get('position')} s, error {s.get('error')}")

# Crash: back to the same song near the same place, playing.
was = state()
m = mark()
send({"cmd": "crash"})
lost = wait(lambda: next((e for e in events(m) if e.get("event") == "state" and e["state"]["connection"] == "reconnecting"), None), 10)
check("crash reported as reconnecting", lost)
t0 = time.monotonic()
back = wait(lambda: (lambda s: s if s.get("connection") == "ready" and s.get("status") == "playing" and (s.get("position") or 0) > 0 and (s.get("track") or {}).get("video_id") == was["track"]["video_id"] else None)(state()), 40)
check("recovers the same song and plays", back, back and f"{back['track']['title']!r} at {back['position']} s (was {was['position']} s), {time.monotonic() - t0:.1f} s after the crash")

print(f"{sum(results)}/{len(results)} passed")
