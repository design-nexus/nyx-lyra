#!/usr/bin/env python3
"""Drive a running probe through its command file and check what the bridge reports.
The probe reads commands from DIR/cmds and writes events to DIR/events.log.
Usage: drive.py DIR [test ...]   tests: commands, background, crash, signed-in"""
import json
import sys
import time

DIR = sys.argv[1]
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


def state(since=0):
    s = [e["message"] for e in events(since) if e.get("event") == "bridge" and e["message"].get("kind") == "state"]
    return s[-1] if s else None


def ask():
    """The bridge's state right now, through the state command."""
    m = mark()
    send({"cmd": "state"})
    return wait(lambda: next((e["result"] for e in events(m) if e.get("event") == "ack" and e.get("cmd") == "state"), None), 5)


def wait(pred, timeout):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v = pred()
        if v:
            return v
        time.sleep(0.2)
    return None


def check(name, ok, detail=""):
    results.append((name, bool(ok), detail))
    print(f"{'PASS' if ok else 'FAIL'}  {name}  {detail}")


def command(c, pred, timeout=8):
    """Send a command; pass when the bridge's reported state satisfies pred (the confirmation)."""
    m = mark()
    t0 = time.monotonic()
    send(c)
    ack = wait(lambda: next((e for e in events(m) if e.get("event") == "ack"), None), 5)
    got = wait(lambda: (lambda s: s if s and pred(s) else None)(ask()), timeout)
    ms = round((time.monotonic() - t0) * 1000)
    return (ack or {}).get("result"), got, ms


def test_commands():
    r, s, ms = command({"cmd": "pause"}, lambda s: s["paused"])
    check("pause", s, f"via {r}, confirmed in {ms} ms")
    r, s, ms = command({"cmd": "play"}, lambda s: s["paused"] is False)
    check("play", s, f"via {r}, confirmed in {ms} ms")
    r, s, ms = command({"cmd": "seek", "position": 120}, lambda s: s["position"] and 119 <= s["position"] <= 126)
    check("seek to 2:00", s, f"via {r}, at {s and s['position']} in {ms} ms")
    r, s, ms = command({"cmd": "volume", "level": 40}, lambda s: s["volume"] == 0.4)
    check("volume 40", s, f"via {r}, in {ms} ms")
    send({"cmd": "volume", "level": 100})
    before = (ask() or {}).get("title")
    r, s, ms = command({"cmd": "next"}, lambda s: s["title"] and s["title"] != before and s["paused"] is False, 15)
    check("next", s, f"via {r}: {before!r} -> {s and s['title']!r} in {ms} ms")
    after = s and s["title"]
    r, s, ms = command({"cmd": "previous"}, lambda s: s["title"] and s["title"] != after, 15)
    if not s:
        # YouTube Music's previous restarts the song first when it's past a few seconds.
        r, s, ms = command({"cmd": "previous"}, lambda s: s["title"] and s["title"] != after, 15)
    check("previous", s, f"via {r}: back to {s and s['title']!r} in {ms} ms")
    r, _, _ = command({"cmd": "volume", "level": 200}, lambda s: True, 2)
    check("bad volume refused", r == "bad-argument", f"result {r!r}")
    s = ask() or {}
    check("metadata", s.get("title") and s.get("artist") and s.get("duration") and s.get("artwork"),
          f"{s.get('title')!r} / {s.get('artist')!r} / album {s.get('album')!r} / {s.get('duration')} s / art {'yes' if s.get('artwork') else 'no'}")


def test_background():
    send({"cmd": "hide"})
    a = ask()
    time.sleep(20)
    b = ask()
    moved = a and b and b["position"] and a["position"] is not None and b["position"] - a["position"] >= 15
    check("plays with the window hidden", moved and not b["paused"], f"{a and a['position']} -> {b and b['position']} s over 20 s")


def test_crash():
    m = mark()
    send({"cmd": "crash"})
    t = wait(lambda: next((e for e in events(m) if e.get("event") == "web-process-terminated"), None), 10)
    check("web process crash reported", t, t and f"reloading {t['reloading']}")
    back = wait(lambda: next((e for e in events(m) if e.get("event") == "load" and e.get("stage") == "Finished"), None), 20)
    check("page reloads after a crash", back, back and f"at t={back['t']}")
    s = wait(lambda: (lambda s: s if s and s.get("title") else None)(ask()), 20)
    check("bridge back after reload", s, s and f"{s['title']!r}, paused {s['paused']}")


def test_signed_in():
    m = mark()
    send({"cmd": "load", "uri": "https://music.youtube.com/watch?v=dX3k_QDnzHE"})
    s = wait(lambda: (lambda s: s if s and s.get("title") and s.get("duration") else None)(state(m)), 25)
    check("session survives a restart (song loads)", s, s and f"{s['title']!r}")
    r, s, ms = command({"cmd": "play"}, lambda s: s["paused"] is False and (s["position"] or 0) > 1, 15)
    check("plays after a restart", s, f"via {r}, in {ms} ms")


for t in sys.argv[2:]:
    globals()[f"test_{t.replace('-', '_')}"]()
print(f"{sum(ok for _, ok, _ in results)}/{len(results)} passed")
