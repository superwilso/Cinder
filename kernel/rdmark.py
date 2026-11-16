#!/usr/bin/env python3
"""Copy a ramdisk (gzip newc cpio) with a kernel-log timestamp around every blocking init step.

    rdmark.py IN.cpio.gz OUT.cpio.gz

Each `exec`/`wait` line in init.rc gets `write /dev/kmsg <label>` before and after it, so a
test boot's dmesg shows where the time goes. Every other file keeps its bytes, owner and mode.
For fastboot test boots only; never flash a ramdisk made with it.
"""
import gzip, re, sys

def entries(b):
    i = 0
    while True:
        h = b[i:i + 110]
        f = [int(h[6 + 8 * k:14 + 8 * k], 16) for k in range(13)]
        nsz, fsz = f[11], f[6]
        name = b[i + 110:i + 110 + nsz - 1].decode()
        d0 = (i + 110 + nsz + 3) & ~3
        yield name, h, b[d0:d0 + fsz]
        if name == "TRAILER!!!":
            return
        i = (d0 + fsz + 3) & ~3

def pack(name, h, data):
    h = h[:54] + b"%08X" % len(data) + h[62:]
    out = h + name.encode() + b"\0"
    out += b"\0" * (-len(out) % 4) + data
    return out + b"\0" * (-len(out) % 4)

def mark(rc):
    out, n = [], 0
    for line in rc.decode().split("\n"):
        m = re.match(r"(\s*)(exec|wait)\s+(\S+)(.*)", line)
        if m and not line.lstrip().startswith("#"):
            n += 1
            what = re.sub(r"[^A-Za-z0-9_.]", "_", (m.group(3) + m.group(4)).strip().split("/")[-1])[:40]
            out += [f"{m.group(1)}write /dev/kmsg T{n:02d}<{what}", line, f"{m.group(1)}write /dev/kmsg T{n:02d}>{what}"]
        else:
            out.append(line)
    return "\n".join(out).encode(), n

src = gzip.decompress(open(sys.argv[1], "rb").read())
res, total = b"", 0
for name, h, data in entries(src):
    if name in ("init.rc", "init.hagoromo.rc", "init.project.rc"):
        data, n = mark(data)
        total += n
    res += pack(name, h, data)
open(sys.argv[2], "wb").write(gzip.compress(res + b"\0" * (-len(res) % 512), 9))
print(f"{total} steps marked")
