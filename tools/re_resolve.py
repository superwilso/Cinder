#!/usr/bin/env python3
"""Annotate Ghidra output of a PIC ARM library: `DAT_000183a8 + 0x1819e` -> the string it points at.

usage: re_resolve.py <elf> < listing.c        (reads the listing on stdin, prints it annotated)
"""
import re, struct, sys

def load(path):
    b = open(path, "rb").read()
    phoff, phentsize, phnum = struct.unpack_from("<I", b, 28)[0], *struct.unpack_from("<HH", b, 42)
    segs = []
    for i in range(phnum):
        t, off, va, _, fsz = struct.unpack_from("<IIIII", b, phoff + i * phentsize)
        if t == 1:
            segs.append((va, off, fsz))
    base = 0 if min(s[0] for s in segs) else 0x10000   # Ghidra loads a zero-based library at 0x10000
    def rd(va, n):
        va -= base
        for v, o, s in segs:
            if v <= va < v + s:
                return b[o + va - v:o + va - v + n]
        return b""
    return rd

def globals_map(elf, rd):
    """Static std::string objects: an _INIT function takes the object's address, then the literal it is built from."""
    import os
    d = "decomp_w1" if "w1_rootfs" in elf else "decomp"
    lst = os.path.join(os.path.dirname(__file__), "..", "artifacts", d, os.path.basename(elf) + ".c")
    g, last, age, init = {}, None, 9, False
    pat = re.compile(r"DAT_([0-9a-f]{8}) \+ (0x[0-9a-f]+)")
    try:
        f = open(lst)
    except OSError:
        return g
    for line in f:
        if line.startswith("// ========"):
            init = "_INIT_" in line
            continue
        if not init:
            continue
        age += 1
        m = pat.search(line)
        if not m:
            continue
        w = rd(int(m.group(1), 16), 4)
        if len(w) < 4:
            continue
        tgt = (struct.unpack("<I", w)[0] + int(m.group(2), 16)) & 0xffffffff
        s = rd(tgt, 80).split(b"\0")[0]
        if len(s) >= 2 and all(32 <= c < 127 for c in s):
            if last is not None and age <= 3 and last not in g:
                g[last] = s.decode()
            last = None
        else:
            last, age = tgt, 0
    return g

def main():
    rd = load(sys.argv[1])
    g = globals_map(sys.argv[1], rd)
    def sub(m):
        w = rd(int(m.group(1), 16), 4)
        if len(w) < 4:
            return m.group(0)
        tgt = (struct.unpack("<I", w)[0] + int(m.group(2), 16)) & 0xffffffff
        s = rd(tgt, 80).split(b"\0")[0]
        if len(s) >= 2 and all(32 <= c < 127 for c in s):
            return '"%s"' % s.decode()
        if tgt in g:
            return "G\"%s\"" % g[tgt]
        return "%s/*->%x*/" % (m.group(0), tgt)
    pat = re.compile(r"DAT_([0-9a-f]{8}) \+ (0x[0-9a-f]+)")
    for line in sys.stdin:
        sys.stdout.write(pat.sub(sub, line))

if __name__ == "__main__":
    main()
