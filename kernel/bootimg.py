#!/usr/bin/env python3
"""bootimg.py - unpack and repack the player's boot image (the `bootimg` partition, p8;
recovery, p9, has the same layout with its ramdisk named RECOVERY).

  bootimg.py unpack BOOT.img OUTDIR          -> OUTDIR/zImage, OUTDIR/ramdisk.cpio.gz
  bootimg.py pack ZIMAGE RAMDISK OUT.img [CMDLINE|fastboot]
                                             -> a boot image laid out exactly as Sony's
  bootimg.py check BOOT.img                  -> unpack + pack in memory, must be byte-identical

Layout (stock NW-A50 series, read off the shipped image): Android boot header v0, page 2048,
kernel at 0x10008000, ramdisk at 0x11000000, tags at 0x10000100, empty name and command line
(the LK writes the command line on a normal boot).

`fastboot boot` is different: LK then uses ONLY the header's command line plus lcm=/fps= and its
common tail (boot reason, serial, icx_pm_helper.*). The base a normal boot builds is missing, so a
fastboot test image needs FASTBOOT_CMDLINE in its header (without vram= the kernel takes the
screen's memory: garbage, then a black screen, 2026-11-15). Never flash an image with it: a normal
boot would add the same arguments a second time. Kernel and ramdisk each carry MediaTek's 512-byte header:
magic 0x58881688, payload size, name ("KERNEL" / "ROOTFS"), 0xff from byte 40. The header's id
field is mkbootimg's SHA-1 over kernel, ramdisk and second stage with their sizes.
"""
import hashlib, os, struct, sys

PAGE = 2048
KADDR, RADDR, SADDR, TAGS = 0x10008000, 0x11000000, 0x10F00000, 0x10000100
MTK_MAGIC = 0x58881688


def mtk_wrap(name, payload):
    h = struct.pack("<II32s", MTK_MAGIC, len(payload), name.encode()) + b"\xff" * (512 - 40)
    return h + payload


def mtk_unwrap(blob, names):
    magic, size, nm = struct.unpack("<II32s", blob[:40])
    name = nm.rstrip(b"\0").decode()
    if magic != MTK_MAGIC or name not in names or size != len(blob) - 512:
        raise SystemExit(f"not a MediaTek {'/'.join(names)} block")
    return blob[512:], name


def pad(b):
    return b + b"\0" * (-len(b) % PAGE)


def unpack(img):
    if img[:8] != b"ANDROID!":
        raise SystemExit("not an Android boot image")
    ks, ka, rs, ra, ss, sa, tags, page = struct.unpack("<8I", img[8:40])
    if page != PAGE or ss != 0 or (ka, ra, tags) not in ((KADDR, RADDR, TAGS), FASTBOOT_ADDRS):
        raise SystemExit(f"unexpected layout: page {page} kernel {ka:#x} ramdisk {ra:#x} tags {tags:#x} second {ss}")
    k = img[PAGE:PAGE + ks]
    roff = PAGE + len(pad(k))
    r = img[roff:roff + rs]
    return mtk_unwrap(k, ["KERNEL"])[0], *mtk_unwrap(r, ["ROOTFS", "RECOVERY"])


# ...and DRAM addresses. The header's 0x10008000 etc. are rewritten by a normal boot; `fastboot
# boot` copies to them as written, i.e. 7 MB into the register space at 0x10000000 (screen garbage,
# then a hang, even with Sony's own kernel, 2026-11-15). DRAM starts at 0x80000000; these match
# the kernel's Makefile.boot (zreladdr, params_phys) and where a normal boot puts the ramdisk, and
# stay below LK's download buffer at 0x85000000.
FASTBOOT_ADDRS = (0x80008000, 0x84000000, 0x80000100)
FASTBOOT_CMDLINE = ("console=/dev/null root=/dev/ram vmalloc=496M slub_max_order=0 slub_debug=O "
                    "vram=6291456 androidboot.selinux=disabled")


def pack(zimage, ramdisk, rname="ROOTFS", cmdline="", addrs=(KADDR, RADDR, TAGS)):
    k, r = mtk_wrap("KERNEL", zimage), mtk_wrap(rname, ramdisk)
    sha = hashlib.sha1()
    for part in (k, r, b""):
        sha.update(part)
        sha.update(struct.pack("<I", len(part)))
    ka, ra, ta = addrs
    hdr = b"ANDROID!" + struct.pack("<10I", len(k), ka, len(r), ra, 0, SADDR, ta, PAGE, 0, 0)
    c = cmdline.encode()
    if len(c) >= 512:
        raise SystemExit("command line longer than the header's 512 bytes")
    hdr += b"\0" * 16 + c + b"\0" * (512 - len(c)) + sha.digest() + b"\0" * 12
    return pad(hdr) + pad(k) + pad(r)


def main(a):
    if len(a) == 3 and a[0] == "unpack":
        z, r, _ = unpack(open(a[1], "rb").read())
        os.makedirs(a[2], exist_ok=True)
        open(os.path.join(a[2], "zImage"), "wb").write(z)
        open(os.path.join(a[2], "ramdisk.cpio.gz"), "wb").write(r)
    elif len(a) in (4, 5) and a[0] == "pack":
        fb = a[4:] == ["fastboot"]
        cmd = FASTBOOT_CMDLINE if fb else (a[4] if len(a) == 5 else "")
        out = pack(open(a[1], "rb").read(), open(a[2], "rb").read(), cmdline=cmd,
                   addrs=FASTBOOT_ADDRS if fb else (KADDR, RADDR, TAGS))
        if len(out) > 16 << 20:
            raise SystemExit(f"{len(out)} bytes: larger than the 16 MiB bootimg partition")
        open(a[3], "wb").write(out)
    elif len(a) == 2 and a[0] == "check":
        img = open(a[1], "rb").read()
        again = pack(*unpack(img))
        same = img[:len(again)] == again and not img[len(again):].strip(b"\0")
        print("identical" if same else "DIFFERS")
        return 0 if same else 1
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
