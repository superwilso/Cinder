#!/usr/bin/env python3
"""find_biquads.py — find second-order IIR filter coefficients in a binary without disassembling it.

    find_biquads.py <file> [--min-run N]

Written for the NW-ZX100's DSP firmware (analysis/RE_clear_bass.md), whose processor has no
disassembler: a filter's coefficients are recognisable as NUMBERS, whatever code reads them.

A biquad normalised to a0 = 1 is b0 b1 b2 a1 a2. For the filters a bass control uses — poles near
DC — a1 lies between -2 and -1 and a2 just under 1, and for peaking and shelving filters b1 and b2
stay close to a1 and a2. That pattern is very unlikely in code or in other data, so every fixed
point encoding is tried (16/24/32-bit, both byte orders, several Q formats, the a-terms stored
negated or first), and candidates that sit in RUNS — tables — are reported with the gain at DC
and at Nyquist and the corner frequency they imply at 44.1 kHz.
"""
import argparse
import cmath
import math
import struct
import sys


def decode(buf, width, endian):
    n = len(buf) // width
    if width == 2:
        return struct.unpack(f"{endian}{n}h", buf[:n * 2])
    if width == 4:
        return struct.unpack(f"{endian}{n}i", buf[:n * 4])
    out = []
    for i in range(n):
        b = buf[i * 3:i * 3 + 3]
        v = int.from_bytes(b, "little" if endian == "<" else "big", signed=True)
        out.append(v)
    return out


def response(b0, b1, b2, a1, a2, f, fs=44100.0):
    z = cmath.exp(-2j * math.pi * f / fs)
    return abs((b0 + b1 * z + b2 * z * z) / (1 + a1 * z + a2 * z * z))


def is_bass_biquad(b0, b1, b2, a1, a2):
    if not (1.0 < -a1 < 2.0 and 0.80 < a2 < 1.0 and -a1 < 1.0 + a2):
        return False
    if not (0.25 < b0 < 4.0) or abs(b1 / b0 - a1) > 0.15 or abs(b2 / b0 - a2) > 0.15:
        return False
    dc = abs((b0 + b1 + b2) / (1 + a1 + a2))
    ny = abs((b0 - b1 + b2) / (1 - a1 + a2))
    return 1 / 8 < dc < 8 and 0.5 < ny < 2.0 and abs(20 * math.log10(dc)) > 0.05


def corner(b0, b1, b2, a1, a2):
    """Frequency where the gain is halfway (in dB) between DC and 20 kHz."""
    g_dc, g_hi = 20 * math.log10(response(b0, b1, b2, a1, a2, 1)), 20 * math.log10(response(b0, b1, b2, a1, a2, 20000))
    mid = (g_dc + g_hi) / 2
    f = 1.0
    while f < 20000:
        if (20 * math.log10(response(b0, b1, b2, a1, a2, f)) - mid) * (g_dc - mid) <= 0:
            return f
        f *= 1.05
    return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("file")
    ap.add_argument("--min-run", type=int, default=3, help="report runs of at least this many filters")
    a = ap.parse_args()
    data = open(a.file, "rb").read()
    found = []
    for width in (4, 3, 2):
        for endian in ("<", ">"):
            for align in range(width):
                vals = decode(data[align:], width, endian)
                qs = {4: (30, 29, 28, 27, 26, 24), 3: (22, 21, 20), 2: (14, 13, 12)}[width]
                for q in qs:
                    s = float(1 << q)
                    fv = [v / s for v in vals]
                    for i in range(len(fv) - 5):
                        w = fv[i:i + 5]
                        for name, (b0, b1, b2, a1, a2) in (
                                ("b,a", (w[0], w[1], w[2], w[3], w[4])),
                                ("b,-a", (w[0], w[1], w[2], -w[3], -w[4])),
                                ("a,b", (w[2], w[3], w[4], w[0], w[1])),
                                ("-a,b", (w[2], w[3], w[4], -w[0], -w[1]))):
                            if is_bass_biquad(b0, b1, b2, a1, a2):
                                off = align + i * width
                                found.append((off, width, endian, q, name, (b0, b1, b2, a1, a2)))
    found.sort()
    # group into runs: same encoding, offsets 5*width apart (a packed table) or up to 64 bytes apart
    runs, cur = [], []
    for f in found:
        if cur and (f[1:5] != cur[-1][1:5] or f[0] - cur[-1][0] > 64):
            runs.append(cur)
            cur = []
        cur.append(f)
    if cur:
        runs.append(cur)
    for r in runs:
        if len(r) < a.min_run:
            continue
        off, width, endian, q, name, _ = r[0]
        print(f"run @0x{off:x}: {len(r)} filters, int{width * 8}{'le' if endian == '<' else 'be'} Q{q} order {name}")
        for f in r:
            b0, b1, b2, a1, a2 = f[5]
            dc = 20 * math.log10(response(b0, b1, b2, a1, a2, 1))
            fc = corner(b0, b1, b2, a1, a2)
            print(f"   0x{f[0]:06x}  DC {dc:+6.2f} dB  ~{fc or 0:7.0f} Hz  "
                  f"[{b0:.6f} {b1:.6f} {b2:.6f} | {a1:.6f} {a2:.6f}]")
    print(f"{len(found)} candidate filters, {sum(1 for r in runs if len(r) >= a.min_run)} runs of >= {a.min_run}", file=sys.stderr)


if __name__ == "__main__":
    main()
