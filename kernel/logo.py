#!/usr/bin/env python3
"""logo.py - the boot logo partition (`logo`, p12): list, extract, and replace images.

  logo.py list LOGO.bin                       -> index, size and kind of every image
  logo.py extract LOGO.bin OUTDIR             -> OUTDIR/NN.png for the full-screen images
  logo.py replace LOGO.bin OUT.bin N=IMG.png [N=IMG.png ...]
  logo.py check LOGO.bin                      -> repack unchanged, must be byte-identical

Layout (read off the player's own p12 and Walkman One's): MediaTek's 512-byte header (magic
0x58881688, payload size, "LOGO", 0xff from byte 40), then u32 count, u32 total size, count u32
offsets from the start of the payload, then one zlib stream per image. Full-screen images are
480x854 RGB565, little-endian, red in the top five bits. Image 0 is the boot logo the bootloader
shows; 1-3 are the battery screens; the small ones are charging-animation digits and bars.
Walkman One recolours 0, 2 and 3 and changes nothing else. Untouched images keep their original
compressed bytes, so a replace changes only what it names.
"""
import os, struct, sys, zlib

W, H = 480, 854
MAGIC = 0x58881688
PART_SIZE = 3 << 20


def read(path):
    d = open(path, "rb").read()
    magic, size, name = struct.unpack("<II32s", d[:40])
    if magic != MAGIC or name.rstrip(b"\0") != b"LOGO":
        raise SystemExit("not a MediaTek LOGO partition")
    body = d[512:512 + size]
    n, total = struct.unpack("<II", body[:8])
    offs = list(struct.unpack(f"<{n}I", body[8:8 + 4 * n])) + [total]
    return [body[offs[i]:offs[i + 1]] for i in range(n)]


def write(blobs):
    head = 8 + 4 * len(blobs)
    offs, o = [], head
    for b in blobs:
        offs.append(o)
        o += len(b)
    body = struct.pack(f"<II{len(blobs)}I", len(blobs), o, *offs) + b"".join(blobs)
    out = struct.pack("<II32s", MAGIC, len(body), b"LOGO") + b"\xff" * (512 - 40) + body
    if len(out) > PART_SIZE:
        raise SystemExit(f"{len(out)} bytes: larger than the 3 MiB logo partition")
    return out


def to_png(raw, path):
    from PIL import Image
    px = struct.unpack(f"<{W * H}H", raw)
    rgb = bytearray()
    for p in px:
        r, g, b = p >> 11, (p >> 5) & 63, p & 31
        rgb += bytes(((r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)))
    Image.frombytes("RGB", (W, H), bytes(rgb)).save(path)


def from_png(path):
    from PIL import Image
    im = Image.open(path).convert("RGB")
    if im.size != (W, H):
        raise SystemExit(f"{path}: {im.size[0]}x{im.size[1]}, the screen needs {W}x{H}")
    out = bytearray()
    for r, g, b in im.getdata():
        out += struct.pack("<H", ((r >> 3) << 11) | ((g >> 2) << 5) | (b >> 3))
    return bytes(out)


def main(a):
    if len(a) == 2 and a[0] == "list":
        blobs = read(a[1])
        for i, b in enumerate(blobs):
            n = len(zlib.decompress(b))
            print(f"{i:2d}  {len(b):7d} compressed  {n:7d} raw  {'full screen' if n == W * H * 2 else ''}")
    elif len(a) == 3 and a[0] == "extract":
        blobs = read(a[1])
        os.makedirs(a[2], exist_ok=True)
        for i, b in enumerate(blobs):
            raw = zlib.decompress(b)
            if len(raw) == W * H * 2:
                to_png(raw, os.path.join(a[2], f"{i:02d}.png"))
    elif len(a) >= 4 and a[0] == "replace":
        blobs = read(a[1])
        for spec in a[3:]:
            n, png = spec.split("=", 1)
            n = int(n)
            if len(zlib.decompress(blobs[n])) != W * H * 2:
                raise SystemExit(f"image {n} is not a full-screen image")
            blobs[n] = zlib.compress(from_png(png), 9)
        open(a[2], "wb").write(write(blobs))
    elif len(a) == 2 and a[0] == "check":
        d = open(a[1], "rb").read()
        blobs = read(a[1])
        again = write(blobs)
        same = d[:len(again)] == again
        print("identical" if same else "DIFFERS")
        return 0 if same else 1
    else:
        print(__doc__, file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
