#!/usr/bin/env python3
"""Extract reference tables from the NW-A55 kernel image (firmware 1.02).

vmlinux.bin is the XZ-decompressed zImage payload; it loads at 0xc0008000, so a
kernel virtual address maps to a file offset by subtracting that. Every address
below came from /proc/kallsyms on the device, so the symbol names are the
kernel's own, not guesses.
"""
import struct, sys

BASE = 0xc0008000
d = open(sys.argv[1], "rb").read()

def u32(addr, n=1):
    o = addr - BASE
    return struct.unpack("<%dI" % n, d[o:o + 4 * n])

def cstr(addr):
    o = addr - BASE
    return d[o:d.index(b"\0", o)].decode("latin1")

# SPM wake-source bit names, in R12 bit order. The run is the argument table of
# spm_output_wake_reason's "wake up by %s" printk.
WAKE_SRC = ["CPU", "PCM_TIMER", "TWAM", "TS", "KP", "GPT", "EINT", "CONN_WDT",
            "CEC", "IRRX", "LOW_BAT", "CONN", "PCM_WDT", "USB_CD", "USB_PDN",
            "DBGSYS", "UART0", "AFE", "THERM", "CIRQ", "CM4", "SYSPWREQ",
            "ETHERNET", "CPU0_IRQ", "CPU1_IRQ", "CPU2_IRQ", "CPU3_IRQ"]

def decode_wakesrc(v):
    return [WAKE_SRC[i] for i in range(len(WAKE_SRC)) if v >> i & 1]

def codec_regmap():
    """cxd3778gf_customer_info is really the regmon reg_info table: a header
    followed by {const char *name; u32 regnum} pairs, 210 of them."""
    out, a = [], 0xc0bc7d90 + 0x28
    for i in range(600):
        p, v = u32(a + i * 8, 2)
        if not 0xc0a00000 < p < 0xc0b00000:
            break
        out.append((v, cstr(p)))
    return out

def power_gs(count_addr, table_addr, label):
    """MTK golden power settings: {volatile u32 *reg; u32 mask; u32 golden}.
    The count is in words, three per entry."""
    words = u32(count_addr)[0]
    ptr = u32(table_addr)[0]
    v = u32(ptr, words)
    return label, [(v[i], v[i + 1], v[i + 2]) for i in range(0, words, 3)]

if __name__ == "__main__":
    what = sys.argv[2]
    if what == "wakesrc":
        v = u32(0xc0b50458)[0]
        print("spm_sleep_wakesrc = %#010x" % v)
        for i, n in enumerate(WAKE_SRC):
            print("  bit %-2d %-10s %s" % (i, n, "ARMED" if v >> i & 1 else "-"))
    elif what == "codec":
        for r, n in codec_regmap():
            print("%#06x  %s" % (r, n))
    elif what == "gs":
        for lbl, ca, ta in [("audio_playback", 0xc0b983c0, 0xc0b983c4),
                            ("idle",           0xc0b983c8, 0xc0b983cc),
                            ("dpidle",         0xc0b983b8, 0xc0b983bc)]:
            label, rows = power_gs(ca, ta, lbl)
            print("\n=== %s (%d regs) ===" % (label, len(rows)))
            for a, m, g in rows:
                print("  %#010x  mask=%#010x  golden=%#010x" % (a, m, g))
