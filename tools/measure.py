#!/usr/bin/env python3
"""Measure a process and every process under it: count, PSS and RSS from smaps_rollup, and
CPU over an interval. PSS shares each page among the processes that map it, so the total
doesn't count shared libraries twice. Usage: measure.py PID [SECONDS] [--json]"""
import json
import os
import sys
import time

TICK = os.sysconf("SC_CLK_TCK")


def tree(root):
    kids = {}
    for p in os.listdir("/proc"):
        if not p.isdigit():
            continue
        try:
            with open(f"/proc/{p}/stat") as f:
                ppid = int(f.read().rsplit(")", 1)[1].split()[1])
        except OSError:
            continue
        kids.setdefault(ppid, []).append(int(p))
    out, todo = [], [root]
    while todo:
        p = todo.pop()
        out.append(p)
        todo += kids.get(p, [])
    return out


def info(pid):
    try:
        with open(f"/proc/{pid}/comm") as f:
            name = f.read().strip()
        with open(f"/proc/{pid}/stat") as f:
            fields = f.read().rsplit(")", 1)[1].split()
        cpu = int(fields[11]) + int(fields[12])
        mem = {}
        with open(f"/proc/{pid}/smaps_rollup") as f:
            for line in f:
                k, _, v = line.partition(":")
                if k in ("Rss", "Pss"):
                    mem[k] = int(v.split()[0])
        return {"pid": pid, "name": name, "cpu_ticks": cpu, "pss_kb": mem.get("Pss", 0), "rss_kb": mem.get("Rss", 0)}
    except OSError:
        return None


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    root, secs = int(args[0]), float(args[1]) if len(args) > 1 else 5.0
    first = {i["pid"]: i for i in filter(None, map(info, tree(root)))}
    t0 = time.monotonic()
    time.sleep(secs)
    wall = time.monotonic() - t0
    procs = []
    for i in filter(None, map(info, tree(root))):
        before = first.get(i["pid"], {"cpu_ticks": i["cpu_ticks"]})["cpu_ticks"]
        i["cpu_pct"] = round(100 * (i["cpu_ticks"] - before) / TICK / wall, 1)
        procs.append(i)
    total = {
        "processes": len(procs),
        "pss_mb": round(sum(p["pss_kb"] for p in procs) / 1024, 1),
        "rss_mb": round(sum(p["rss_kb"] for p in procs) / 1024, 1),
        "cpu_pct": round(sum(p["cpu_pct"] for p in procs), 1),
    }
    if "--json" in sys.argv:
        print(json.dumps({"total": total, "processes": procs}))
        return
    for p in sorted(procs, key=lambda p: -p["pss_kb"]):
        print(f'{p["pid"]:>8} {p["name"]:<22} PSS {p["pss_kb"] / 1024:7.1f} MB  RSS {p["rss_kb"] / 1024:7.1f} MB  CPU {p["cpu_pct"]:5.1f}%')
    print(f'total: {total["processes"]} processes, PSS {total["pss_mb"]} MB (RSS sum {total["rss_mb"]} MB), CPU {total["cpu_pct"]}% over {secs:g} s')


if __name__ == "__main__":
    main()
