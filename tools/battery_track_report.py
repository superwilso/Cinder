#!/usr/bin/env python3
"""Analyse a long-run battery track.

The point of a multi-day track is that you cannot hold the player in one state, so the log is a
mixture: charging and discharging, screen on and off, playing and idle. This splits it back apart
and reports a drain rate per state, plus the one number goal #1 actually needs — projected runtime.

Only DISCHARGING segments can say anything about drain; charging ones are reported separately so a
gap in the series is visible rather than silently averaged in.
"""
import sys
from collections import defaultdict

HDR = "epoch uptime capacity voltage status usb backlight cpu_idle cpu_total pcm home".split()


def rows(path):
    out = []
    for line in open(path):
        line = line.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        f = line.split("\t")
        if len(f) < 11:
            continue
        try:
            out.append(dict(
                epoch=int(f[0]), uptime=float(f[1]), cap=int(f[2]), volt=int(f[3]),
                status=f[4], usb=f[5], bl=int(f[6] or 0),
                cpu_idle=int(f[7]), cpu_total=int(f[8]), pcm=f[9], home=int(f[10]),
            ))
        except ValueError:
            continue
    return out


def state_of(r):
    """What the player was doing. Ordered most-specific first."""
    playing = r["pcm"] != "-" and "RUNNING" in r["pcm"]
    screen = r["bl"] > 0
    if playing and screen:
        return "playing, screen on"
    if playing:
        return "playing, screen off"
    if screen:
        return "idle, screen on"
    return "idle, screen off"


def main(path):
    rs = rows(path)
    if len(rs) < 3:
        print("not enough samples yet — let it run longer")
        return

    span_h = (rs[-1]["epoch"] - rs[0]["epoch"]) / 3600.0
    print(f"samples        {len(rs)}")
    print(f"span           {span_h:.2f} h")
    print(f"battery        {rs[0]['cap']}% -> {rs[-1]['cap']}%")

    # Per-state accumulation over consecutive discharging pairs only.
    secs = defaultdict(float)
    dcap = defaultdict(float)
    dmv = defaultdict(float)
    busy = defaultdict(lambda: [0.0, 0.0])  # [busy_jiffies, total_jiffies]
    charge_s = 0.0
    reboots = 0

    for a, b in zip(rs, rs[1:]):
        dt = b["epoch"] - a["epoch"]
        if dt <= 0 or dt > 3600:          # clock jump or a long gap: not a usable interval
            continue
        if b["uptime"] < a["uptime"]:     # the device rebooted between samples
            reboots += 1
            continue
        if a["status"] == "Charging" or b["status"] == "Charging" or a["usb"] == "1":
            charge_s += dt
            continue

        st = state_of(a)
        secs[st] += dt
        dcap[st] += a["cap"] - b["cap"]
        dmv[st] += (a["volt"] - b["volt"]) / 1000.0
        dtot = b["cpu_total"] - a["cpu_total"]
        didle = b["cpu_idle"] - a["cpu_idle"]
        if dtot > 0:
            busy[st][0] += dtot - didle
            busy[st][1] += dtot

    if reboots:
        print(f"reboots        {reboots} (those intervals dropped)")
    print(f"charging       {charge_s / 3600.0:.2f} h (excluded from drain)")

    live = {k: v for k, v in secs.items() if v > 0}
    if not live:
        print("\nno discharging intervals yet — unplug and use it for a while")
        return

    print()
    print(f"{'state':<20} {'hours':>7} {'%/h':>7} {'mV/h':>8} {'cpu busy':>9} {'runtime':>9}")
    print("-" * 64)
    for st in sorted(live, key=lambda k: -live[k]):
        h = live[st] / 3600.0
        pph = dcap[st] / h if h > 0 else 0.0
        mvph = dmv[st] / h if h > 0 else 0.0
        bt = busy[st]
        bpct = (100.0 * bt[0] / bt[1]) if bt[1] else 0.0
        rt = f"{100.0 / pph:.1f} h" if pph > 0.05 else "—"
        print(f"{st:<20} {h:7.2f} {pph:7.2f} {mvph:8.1f} {bpct:8.1f}% {rt:>9}")

    print()
    print("Reading it: %/h is the gauge, mV/h is the cell. The gauge is coarse (1% steps) and lies")
    print("near 100%, so trust mV/h over short spans and %/h over long ones. 'runtime' is 100/%/h —")
    print("a projection from that state alone, not a prediction of mixed use.")
    thin = [s for s in live if live[s] < 1800]
    if thin:
        print()
        print("Thin (<30 min), treat as provisional: " + ", ".join(sorted(thin)))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "battery_track.tsv")
