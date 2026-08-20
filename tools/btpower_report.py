#!/usr/bin/env python3
"""Turn two raw counter samples from btpower_sampler.sh into the numbers worth arguing about.

Cumulative counters only, so every figure below is a delta over the window — nothing here can be
skewed by the sampling, which is the whole reason the device writes raw values and this does the
arithmetic. Jiffies are 10 ms on this kernel (USER_HZ=100).
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

HZ = 100.0


def blocks(text: str) -> dict[str, list[str]]:
    out: dict[str, list[str]] = {}
    current = None
    for line in text.splitlines():
        if line.startswith("## "):
            current = line[3:].strip()
            out[current] = []
        elif current:
            out[current].append(line)
    return out


def scalar(lines: list[str], prefix: str) -> str:
    for line in lines:
        if line.startswith(prefix):
            return line[len(prefix):].strip()
    return ""


def cpu_jiffies(lines: list[str]) -> tuple[float, float]:
    """(busy, total) jiffies from the aggregate cpu line."""
    row = scalar(lines, "cpu ").split()
    values = [float(v) for v in row if v.isdigit()]
    if len(values) < 4:
        return 0.0, 0.0
    idle = values[3] + (values[4] if len(values) > 4 else 0.0)   # idle + iowait
    total = sum(values)
    return total - idle, total


def time_in_state(lines: list[str]) -> dict[int, float]:
    out: dict[int, float] = {}
    grab = False
    for line in lines:
        if line.startswith("-- time_in_state"):
            grab = True
            continue
        if line.startswith("--"):
            grab = False
        if grab:
            parts = line.split()
            if len(parts) == 2 and parts[0].isdigit():
                out[int(parts[0])] = float(parts[1])
    return out


def procs(lines: list[str]) -> dict[str, tuple[str, float]]:
    out: dict[str, tuple[str, float]] = {}
    for line in lines:
        m = re.match(r"proc \((.*?)\) (\d+) utime=(\d+) stime=(\d+)", line.strip())
        if m:
            name, pid, utime, stime = m.group(1), m.group(2), float(m.group(3)), float(m.group(4))
            out[f"{name}:{pid}"] = (name, utime + stime)
    return out


def codec(lines: list[str]) -> dict[str, str]:
    out: dict[str, str] = {}
    grab = False
    for line in lines:
        if line.startswith("-- codec"):
            grab = True
            continue
        if grab and line.startswith("--"):
            grab = False
        if grab and not line.startswith("#"):   # "# done" closes the file, it is not a register
            parts = line.split()
            if len(parts) == 2:
                out[parts[0]] = parts[1]
    return out


def section(lines: list[str], head: str) -> list[str]:
    out, grab = [], False
    for line in lines:
        if line.startswith(head):
            grab = True
            continue
        if grab and line.startswith("--"):
            break
        if grab and line.strip():
            out.append(line.strip())
    return out


def main(path: str) -> int:
    text = Path(path).read_text(encoding="utf-8", errors="replace")
    b = blocks(text)
    if "T0" not in b or "T1" not in b:
        print("sample is incomplete (need both T0 and T1)")
        return 1
    t0, t1 = b["T0"], b["T1"]

    up0 = float(scalar(t0, "uptime").split()[0] or 0)
    up1 = float(scalar(t1, "uptime").split()[0] or 0)
    wall = max(up1 - up0, 1e-9)

    busy0, tot0 = cpu_jiffies(t0)
    busy1, tot1 = cpu_jiffies(t1)
    busy = busy1 - busy0
    total = max(tot1 - tot0, 1e-9)

    ctxt = float(scalar(t1, "ctxt") or 0) - float(scalar(t0, "ctxt") or 0)

    s0, s1 = time_in_state(t0), time_in_state(t1)
    spent = {khz: s1.get(khz, 0) - s0.get(khz, 0) for khz in set(s0) | set(s1)}
    spent = {k: v for k, v in spent.items() if v > 0}
    ticks = sum(spent.values()) or 1
    avg_mhz = sum(k * v for k, v in spent.items()) / ticks / 1000.0

    print(f"window            {wall:.0f} s")
    print(f"CPU busy          {100.0 * busy / total:.2f} %   ({busy / HZ:.1f} s of CPU)")
    print(f"average clock     {avg_mhz:.0f} MHz")
    print(f"context switches  {ctxt / wall:.1f} /s")
    for khz, jif in sorted(spent.items(), reverse=True):
        print(f"   {khz / 1000:>6.0f} MHz   {100.0 * jif / ticks:5.1f} %")

    cap0, cap1 = scalar(t0, "capacity"), scalar(t1, "capacity")
    v0, v1 = scalar(t0, "voltage_now"), scalar(t1, "voltage_now")
    charging = scalar(t1, "status")
    print(f"\nbattery           {cap0}% -> {cap1}%   ({charging})")
    if cap0.isdigit() and cap1.isdigit():
        drop = int(cap0) - int(cap1)
        if charging.strip().lower().startswith("dischar") and drop > 0:
            print(f"                  {drop * 3600.0 / wall:.1f} %/hour at this rate "
                  f"({100.0 / (drop * 3600.0 / wall):.1f} h to flat)")
        elif not charging.strip().lower().startswith("dischar"):
            print("                  CHARGING — the level says nothing about drain. Re-run cable-out.")
        else:
            print("                  no whole percent moved; run a longer window for a rate.")
    if v0 and v1:
        print(f"voltage           {int(v0) / 1000.0:.0f} -> {int(v1) / 1000.0:.0f} mV")

    p0, p1 = procs(t0), procs(t1)
    rows = []
    for key, (name, end) in p1.items():
        start = p0.get(key, (name, 0.0))[1]
        used = end - start
        if used > 0:
            rows.append((used, name, key.split(":")[1]))
    if rows:
        print("\nper-process CPU (share of the window)")
        for used, name, pid in sorted(rows, reverse=True)[:8]:
            print(f"   {name:<22} pid {pid:<6} {100.0 * (used / HZ) / wall:5.2f} %  ({used / HZ:.1f} s)")

    c0, c1 = codec(t0), codec(t1)
    changed = [(k, c0.get(k, "?"), c1[k]) for k in c1 if c0.get(k) != c1[k]]
    print("\ncodec (CXD3778GF)")
    live = [k for k, v in c1.items() if v not in ("", "0x00000000", "?")]
    print(f"   non-zero at the end: {', '.join(live) if live else '(none — the codec is idle)'}")
    for key, before, after in changed:
        print(f"   {key:<14} {before} -> {after}")
    if not changed:
        print("   unchanged across the window")

    for label, lines in (("T0", t0), ("T1", t1)):
        alsa = section(lines, "-- alsa")
        print(f"\nALSA {label}: " + ("; ".join(alsa) if alsa else "nothing open"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1] if len(sys.argv) > 1 else "btpower.txt"))
