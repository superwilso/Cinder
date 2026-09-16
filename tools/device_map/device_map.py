#!/usr/bin/env python3
"""device_map.py — take a read-only map of a connected NW-A50-series player over adb.

    python3 tools/device_map/device_map.py --out ../cinder-sony-analysis/device/nw-a55/<label>

What it collects (collect.sh, run on the player) is the part of the device no firmware image can
show: the processes that actually run and which Sony libraries each one hosts, the unix sockets
between them, the kernel's view (config, partitions, interrupts, I2C chips, modules), sysfs, the
ALSA mixer, the input devices, and the ramdisk as booted. Nothing on the player is changed; the
collector writes only its own tarball under /tmp and this script deletes it again.

The output is meant to be PUBLISHED, so before anything is written:
  * identifiers are scrubbed — the serial number, every MAC address (including the Bluetooth
    address in /data/BT_Addr, matched in any spelling), serial-like properties, and the paths of
    the owner's own files that open descriptors point at;
  * a LEAK CHECK then searches every output file for the raw values it scrubbed and for the names
    in the player's Bluetooth device list, and refuses to finish if one survived.
Read SUMMARY.md and skim the rest before committing anyway: a scrubber only knows what it was told.
"""
import argparse
import gzip
import io
import os
import re
import shutil
import subprocess
import sys
import tarfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
MAC = re.compile(r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b")
SENSITIVE_PROP = re.compile(r"serial|imei|meid|mac|bdaddr|btaddr|bt\.addr|address|uuid|\bcid\b|nvp|ssid|"
                            r"device_?id|hwid|cpuid|token|account|password|secret|key", re.I)
# The music volumes, where the only thing a path can add is the name of someone's file. Not /mnt:
# the init scripts name system mount points there.
USER_PATH = re.compile(r"(/(?:contents|contents_ext|data/mnt/[a-z_]+)/)(?!cinder)[^\s'\"<]+")


def adb(*args, binary=False, timeout=600):
    r = subprocess.run(["adb", *args], capture_output=True, timeout=timeout)
    if r.returncode != 0:
        raise SystemExit(f"adb {' '.join(args)} failed: {r.stderr.decode(errors='replace').strip()}")
    return r.stdout if binary else r.stdout.decode(errors="replace")


def device_bytes(path):
    """A small device file's bytes. This player's adbd has no exec-out, and a pty shell rewrites
    newlines, so binary crosses as base64."""
    import base64
    out = adb("shell", f"/xbin/busybox base64 {path} 2>/dev/null")
    return base64.b64decode("".join(out.split())) if out.strip() else b""


def secrets():
    """Raw identifiers, read into memory only, so the scrubber and the leak check know them."""
    s = {"serial": adb("shell", "getprop ro.serialno").strip()}
    bt = device_bytes("/data/BT_Addr")
    hexes = re.findall(rb"[0-9A-Fa-f]{12}", bt) or [bt[:6].hex().encode()] if bt else []
    s["bt_hex"] = sorted({h.decode().lower() for h in hexes if h.strip(b"0")})
    # The paired and seen-nearby device list: a directory of MTK stack files.
    names = b"".join(device_bytes(f"/data/Bluetooth/devdb/{f}")
                     for f in adb("shell", "ls /data/Bluetooth/devdb 2>/dev/null").split())
    words = {w.decode(errors="ignore").strip() for w in re.findall(rb"[\x20-\x7e]{5,}", names)}
    # The player's own name ("NW-A50Series") is in that list too, and is not the owner's data.
    own = {v.lower() for v in re.findall(r"^\[[^\]]+\]: \[(.+)\]\r?$", adb("shell", "getprop"), re.M)}
    s["bt_names"] = sorted(w for w in words
                           if re.search(r"[A-Za-z]", w) and not MAC.fullmatch(w) and w.lower() not in own)
    return s


def scrub(text, sec):
    if sec["serial"]:
        text = re.sub(re.escape(sec["serial"]), "<serial>", text, flags=re.I)
    for h in sec["bt_hex"]:
        pairs = [h[i:i + 2] for i in range(0, 12, 2)]
        for spelling in (h, ":".join(pairs), "".join(reversed(pairs)), ":".join(reversed(pairs))):
            text = re.sub(re.escape(spelling), "<bt-address>", text, flags=re.I)
    text = MAC.sub("xx:xx:xx:xx:xx:xx", text)
    text = re.sub(r"^(\[[^\]]*(?:%s)[^\]]*\]: )\[.*\]$" % SENSITIVE_PROP.pattern,
                  r"\1[<redacted>]", text, flags=re.I | re.M)
    text = re.sub(r"^(Serial\s*:\s*)\S+", r"\1<redacted>", text, flags=re.M)
    text = USER_PATH.sub(r"\1<user file>", text)
    return text


def leaks(root, sec):
    found = []
    terms = [sec["serial"]] + sec["bt_hex"] + [n for n in sec["bt_names"] if len(n) >= 6]
    for dirpath, _, files in os.walk(root):
        for f in files:
            p = os.path.join(dirpath, f)
            if p.endswith(".gz"):
                continue
            data = open(p, encoding="utf-8", errors="replace").read().lower()
            for t in terms:
                if t and t.lower() in data:
                    found.append((os.path.relpath(p, root), t))
    return found


# ── SUMMARY.md ───────────────────────────────────────────────────────────────────────────────
def read(root, rel):
    try:
        return open(os.path.join(root, rel), encoding="utf-8", errors="replace").read()
    except OSError:
        return ""


def summary(root, label):
    out = [f"# Device map — {label}", ""]
    cpu = read(root, "system/proc_cpuinfo.txt")
    mem = re.search(r"MemTotal:\s+(\d+)", read(root, "system/proc_meminfo.txt"))
    props = dict(re.findall(r"^\[([^\]]+)\]: \[(.*)\]$", read(root, "system/getprop.txt"), re.M))
    hw = re.search(r"^Hardware\s*:\s*(.+)$", cpu, re.M)
    out += ["| | |", "|---|---|",
            f"| Model (property) | {props.get('ro.model.number', '?')} |",
            f"| Board | {props.get('ro.product.board', '?')} |",
            f"| SoC (cpuinfo Hardware) | {hw.group(1) if hw else '?'} |",
            f"| CPU cores | {len(re.findall(r'^processor', cpu, re.M))} |",
            f"| RAM | {int(mem.group(1)) // 1024 if mem else '?'} MB |",
            f"| Kernel | {read(root, 'system/proc_version.txt').strip()} |",
            f"| Android base (props) | {props.get('ro.build.version.release', '?')} (SDK {props.get('ro.build.version.sdk', '?')}) |",
            ""]

    # partitions
    dum = read(root, "system/proc_dumchar_info.txt").strip().splitlines()
    if dum:
        out += ["## Partitions (`/proc/dumchar_info`)", "", "```", *dum, "```", ""]

    # mounts
    rows = [l.split() for l in read(root, "system/proc_mounts.txt").splitlines() if l.strip()]
    rows = [r for r in rows if len(r) >= 4 and r[2] not in ("proc", "sysfs", "devpts", "debugfs", "cgroup", "selinuxfs")]
    out += ["## Mounts", "", "| device | mount point | fs | options |", "|---|---|---|---|"]
    out += [f"| `{r[0]}` | `{r[1]}` | {r[2]} | `{r[3]}` |" for r in rows] + [""]

    # services: init rc -> process -> libraries -> sockets
    rc = "".join(read(root, f"init/{f}") for f in sorted(os.listdir(os.path.join(root, "init"))))
    svc = {}
    for m in re.finditer(r"^service\s+(\S+)\s+(.+)$", rc, re.M):
        svc.setdefault(m.group(1), m.group(2).strip())
    procs = []
    for block in read(root, "processes/processes.txt").split("=== pid ")[1:]:
        head, _, body = block.partition("\n")
        pid, _, cmd = head.partition(": ")
        uid = re.search(r"^Uid:\s+(\d+)", body, re.M)
        sony = sorted({os.path.basename(l) for l in body.splitlines() if "/vendor/sony/lib/" in l})
        pre = [l for l in body.splitlines() if l.startswith("LD_PRELOAD")]
        procs.append((int(pid), cmd.strip(), uid.group(1) if uid else "?", sony, pre))
    sockets = {}
    for l in read(root, "ipc/netstat_unix.txt").splitlines():
        m = re.search(r"\s(\d+)/(\S+)\s+(@?\S+)$", l)
        if m and (m.group(3).startswith("@") or m.group(3).startswith("/")):
            sockets.setdefault(int(m.group(1)), set()).add(m.group(3))
    out += ["## Processes that host Sony services", "",
            "Every `hagodaemon` below is one `service` line of the booted init scripts; the library list is "
            "what the process actually MAPPED, and the sockets are the unix sockets it holds. uid 1005 "
            "appears only on the `logwrapper` parents, which are left out.", ""]
    for pid, cmd, uid, sony, pre in sorted(procs, key=lambda p: p[1]):
        if cmd.startswith("/bin/logwrapper") or (not sony and "hagodaemon" not in cmd):
            continue          # init's wrapper; the service itself follows as its child
        out.append(f"### `{cmd}`")
        out.append(f"pid {pid}, uid {uid}" + (f"; {pre[0]}" if pre else ""))
        if sony:
            out.append("")
            out.append("Sony libraries: " + ", ".join(f"`{s}`" for s in sony))
        if pid in sockets:
            out.append("")
            out.append("Sockets: " + ", ".join(f"`{s}`" for s in sorted(sockets[pid])))
        out.append("")
    if svc:
        out += ["### Init services (booted ramdisk)", "", "| service | command |", "|---|---|"]
        out += [f"| `{k}` | `{v}` |" for k, v in sorted(svc.items())] + [""]

    # ALSA, input, I2C, modules
    pcm = read(root, "alsa/proc_asound.txt")
    m = re.search(r"== /proc/asound/pcm\n(.*?)\n\n", pcm, re.S)
    if m:
        out += ["## ALSA PCM devices", "", "```", m.group(1).strip(), "```", ""]
    ev = re.findall(r"add device \d+: (\S+)\n\s+name:\s+\"([^\"]*)\"", read(root, "input/getevent_p.txt"))
    if ev:
        out += ["## Input devices", "", "| node | name |", "|---|---|"] + [f"| `{n}` | {d} |" for n, d in ev] + [""]
    i2c = re.findall(r"== /sys/bus/i2c/devices/([^/]+)/name\n(\S+)", read(root, "sysfs/i2c_devices.txt"))
    if i2c:
        out += ["## I2C devices (bus-address → driver name)", "", "| device | name |", "|---|---|"]
        out += [f"| `{a}` | {n} |" for a, n in i2c] + [""]
    mods = [l.split()[0] for l in read(root, "kernel/lsmod.txt").splitlines()[1:] if l.strip()]
    if mods:
        out += ["## Loaded kernel modules", "", ", ".join(f"`{x}`" for x in mods), ""]
    misc = re.findall(r"== /sys/class/misc/([^/]+)/dev\n(\S+)", read(root, "sysfs/misc_devices.txt"))
    if misc:
        out += ["## Misc character devices (major:minor)", "", ", ".join(f"`{n}` {d}" for n, d in misc), ""]
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--out", required=True, help="output directory (created; must be empty or absent)")
    ap.add_argument("--label", default="NW-A55", help="name used in SUMMARY.md")
    a = ap.parse_args()
    if os.path.exists(a.out) and os.listdir(a.out):
        raise SystemExit(f"{a.out} is not empty")
    if adb("get-state").strip() != "device":
        raise SystemExit("no device")

    sec = secrets()
    adb("push", os.path.join(HERE, "collect.sh"), "/tmp/cinder_map_collect.sh")
    t0 = time.time()
    print(adb("shell", "sh /tmp/cinder_map_collect.sh", timeout=900).strip())
    tar_local = a.out.rstrip("/") + ".tar"
    adb("pull", "/tmp/cinder_map.tar", tar_local, timeout=300)
    blob = open(tar_local, "rb").read()
    os.remove(tar_local)
    adb("shell", "/xbin/busybox rm -f /tmp/cinder_map.tar /tmp/cinder_map_collect.sh")
    print(f"pulled {len(blob)} bytes in {time.time() - t0:.0f} s")

    staging = a.out + ".staging"
    shutil.rmtree(staging, ignore_errors=True)
    with tarfile.open(fileobj=io.BytesIO(blob)) as tf:
        tf.extractall(staging, filter="data")
    src = os.path.join(staging, "cinder_map")
    cfg = os.path.join(src, "kernel", "config.gz")
    if os.path.exists(cfg):
        with gzip.open(cfg) as g, open(os.path.join(src, "kernel", "config.txt"), "wb") as o:
            o.write(g.read())
        os.remove(cfg)
    for dirpath, _, files in os.walk(src):
        for f in files:
            p = os.path.join(dirpath, f)
            raw = open(p, "rb").read()
            if b"\0" in raw[:4096]:
                os.remove(p)          # nothing binary is meant to be in a map
                continue
            open(p, "w", encoding="utf-8").write(scrub(raw.decode("utf-8", errors="replace"), sec))
    open(os.path.join(src, "SUMMARY.md"), "w").write(scrub(summary(src, a.label), sec))

    bad = leaks(src, sec)
    if bad:
        for rel, term in bad[:40]:
            print(f"LEAK: {rel} still contains a scrubbed value ({len(term)} chars)")
        raise SystemExit(f"refusing to write {a.out}: {len(bad)} leak(s); output left in {staging} for inspection")
    os.makedirs(os.path.dirname(os.path.abspath(a.out)), exist_ok=True)
    shutil.move(src, a.out)
    shutil.rmtree(staging, ignore_errors=True)
    print(f"wrote {a.out} — leak check clean; read SUMMARY.md before publishing")


if __name__ == "__main__":
    main()
