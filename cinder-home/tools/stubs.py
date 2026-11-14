#!/usr/bin/env python3
"""stubs.py: stand-ins for the device libraries cinder-home links against, from their symbol facts.

    python3 cinder-home/tools/stubs.py gen            # read the real libraries -> cinder-home/stubs/*.txt
    python3 cinder-home/tools/stubs.py build OUT      # stubs/*.txt -> OUT/{vendor/sony/lib,ramdisk/lib,lib}
    python3 cinder-home/tools/stubs.py check OUT      # every stand-in reads back as its spec

WHY. The ARM binaries link against Sony's libraries and the player's own runtime (glibc 2.23, libc++,
libcxxrt), which come from a firmware image that cannot go into a public repo. A linker reads only a
library's dynamic tables: its name, what it needs, its version definitions and its dynamic symbols
(name, value, size, type, binding, version, and what kind of section each lives in). `gen` writes
those facts as text, one file per library; `build` writes an ELF file holding exactly those tables
and no code or data. `build.sh` links against them when CINDER_STUBS=OUT, which is how CI builds the
player without the firmware.

THE ORDER IS PART OF THE FACTS. The symbols are kept in the library's own order, the ones it imports
included: the linker meets them in that order, and the order cinder-home's own dynamic string table
comes out in follows it. A stand-in built by an ordinary linker re-sorts them, and cinder-home then
differs from the real build in 11,000 bytes of .dynstr (2026-11-14). Written by hand like this, the
build is byte-identical to one against the real libraries.

Run `gen` again whenever build.sh starts linking another library, and commit stubs/.
"""
import os
import struct
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SPECS = os.path.join(REPO, "cinder-home", "stubs")
DIRS = [
    ("artifacts/rootfs_mnt/vendor/sony/lib", "vendor/sony/lib"),
    ("analysis/ramdisk/lib", "ramdisk/lib"),
    ("artifacts/rootfs_mnt/lib", "lib"),
]
# What build.sh links by name; their DT_NEEDED closure is followed from here.
SEED = [
    f"lib{n}.so"
    for n in (
        "easelcore easelcui pstcore appmgrservice PlayerServiceClient PlayerServiceClientUtil "
        "EffectCtrlDmp PowerMgrServiceClient UsbDeviceAudioPlayerService BtCommonService "
        "BtTransmitterService VolumeService MediaStoreServiceClient NfcService TunerPlayerService "
        "AudioInPlayerService BtPlayerService DisplayService UsbMgrServiceFw ConnMgrService "
        "UsbDeviceConnectionService FuncMgrService SoundServiceSettingsDmp"
    ).split()
] + [
    "libc++.so.1", "libcxxrt.so.1", "libasound.so",
    "libc.so.6", "libdl.so.2", "libpthread.so.0", "libm.so.6",
]

SHT_NOBITS, SHT_DYNSYM, SHT_STRTAB, SHT_HASH, SHT_DYNAMIC = 8, 11, 3, 5, 6
SHT_VERDEF, SHT_VERNEED, SHT_VERSYM = 0x6FFFFFFD, 0x6FFFFFFE, 0x6FFFFFFF
SHF_WRITE, SHF_ALLOC, SHF_EXEC, SHF_TLS = 1, 2, 4, 0x400
DT = dict(NULL=0, NEEDED=1, HASH=4, STRTAB=5, SYMTAB=6, STRSZ=10, SYMENT=11, SONAME=14,
          VERSYM=0x6FFFFFF0, VERDEF=0x6FFFFFFC, VERDEFNUM=0x6FFFFFFD, VERNEED=0x6FFFFFFE,
          VERNEEDNUM=0x6FFFFFFF)
# The kinds of section a symbol can live in, and the section each becomes in a stand-in.
KINDS = {  # kind: (name, type, flags)
    "T": (".text", SHT_NOBITS, SHF_ALLOC | SHF_EXEC),
    "R": (".rodata", SHT_NOBITS, SHF_ALLOC),
    "W": (".data", SHT_NOBITS, SHF_ALLOC | SHF_WRITE),
    "L": (".tbss", SHT_NOBITS, SHF_ALLOC | SHF_WRITE | SHF_TLS),
}


# ── reading a real library ───────────────────────────────────────────────────────────────────────

class Elf:
    def __init__(self, path):
        self.b = b = open(path, "rb").read()
        assert b[:4] == b"\x7fELF" and b[4] == 1 and b[5] == 1, f"{path}: not a 32-bit LE ELF"
        (self.etype, self.machine, _, _, _, shoff, self.flags, _, _, _, shentsize, shnum,
         shstrndx) = struct.unpack_from("<HHIIIIIHHHHHH", b, 16)
        self.osabi = b[7]
        self.sh = [struct.unpack_from("<IIIIIIIIII", b, shoff + i * shentsize) for i in range(shnum)]

    def sec(self, typ):
        for s in self.sh:
            if s[1] == typ:
                return s
        return None

    def data(self, s):
        return self.b[s[4]:s[4] + s[5]]

    def strs(self, s):
        d = self.data(s)
        return lambda off: d[off:d.index(b"\0", off)].decode()


def kind_of(elf, shndx):
    if shndx == 0:
        return "U"
    if shndx == 0xFFF1:
        return "A"
    if shndx >= 0xFF00 or shndx >= len(elf.sh):
        return "A"
    flags = elf.sh[shndx][2]
    if flags & SHF_TLS:
        return "L"
    if flags & SHF_EXEC:
        return "T"
    return "W" if flags & SHF_WRITE else "R"


def gen_one(path):
    e = Elf(path)
    out = [f"elf {e.flags:#x} {e.osabi}"]
    dynsym, versym = e.sec(SHT_DYNSYM), e.sec(SHT_VERSYM)
    s = e.strs(e.sh[dynsym[6]])
    dyn = e.sec(SHT_DYNAMIC)
    needed = []
    for off in range(0, dyn[5], 8):
        tag, val = struct.unpack_from("<iI", e.data(dyn), off)
        if tag == DT["SONAME"]:
            out.append(f"soname {s(val)}")
        elif tag == DT["NEEDED"]:
            needed.append(s(val))
    out += [f"needed {n}" for n in needed]
    vd = e.sec(SHT_VERDEF)
    if vd:
        d, off = e.data(vd), 0
        for _ in range(vd[7]):  # sh_info: the entry count
            ver, flags, ndx, cnt, _hash, aux, nxt = struct.unpack_from("<HHHHIII", d, off)
            names, a = [], off + aux
            for _ in range(cnt):
                name, anext = struct.unpack_from("<II", d, a)
                names.append(s(name))
                a += anext
            out.append(f"verdef {ndx} {flags} " + " ".join(names))
            off += nxt
    vn = e.sec(SHT_VERNEED)
    if vn:
        d, off = e.data(vn), 0
        for _ in range(vn[7]):
            _ver, cnt, file_, aux, nxt = struct.unpack_from("<HHIII", d, off)
            a = off + aux
            for _ in range(cnt):
                _hash, flags, other, name, anext = struct.unpack_from("<IHHII", d, a)
                out.append(f"verneed {s(file_)} {other} {flags} {s(name)}")
                a += anext
            off += nxt
    vs = e.data(versym) if versym else None
    d = e.data(dynsym)
    for i in range(1, dynsym[5] // 16):
        name, value, size, info, other, shndx = struct.unpack_from("<IIIBBH", d, i * 16)
        v = struct.unpack_from("<H", vs, i * 2)[0] if vs else 1
        out.append(f"sym {value:#x} {size} {info} {other} {kind_of(e, shndx)} {v:#x} {s(name)}")
    return out, needed


def find(name):
    for src, dst in DIRS:
        p = os.path.join(REPO, src, name)
        if os.path.exists(p):
            return p, dst
    return None, None


def gen():
    os.makedirs(SPECS, exist_ok=True)
    for f in os.listdir(SPECS):
        if f.endswith(".txt"):
            os.remove(os.path.join(SPECS, f))
    seen, todo = set(), list(SEED)
    while todo:
        name = todo.pop(0)
        if name in seen:
            continue
        seen.add(name)
        path, dst = find(name)
        if not path:
            print(f"note: {name} is needed by a library but not in the firmware folders; skipped")
            continue
        lines, needed = gen_one(path)
        todo += needed
        with open(os.path.join(SPECS, name + ".txt"), "w") as fh:
            fh.write(f"# {dst}/{name}: dynamic tables only (cinder-home/tools/stubs.py gen)\n")
            fh.write(f"dir {dst}\n" + "\n".join(lines) + "\n")
    print(f"{len(os.listdir(SPECS))} libraries -> {os.path.relpath(SPECS, REPO)}/")


# ── writing a stand-in ───────────────────────────────────────────────────────────────────────────

def load(path):
    sp = dict(dir=None, flags=0x5000400, osabi=0, soname=None, needed=[], verdef=[], verneed=[], sym=[])
    for line in open(path):
        line = line.rstrip("\n")
        if not line or line.startswith("#"):
            continue
        key, _, rest = line.partition(" ")
        if key == "dir":
            sp["dir"] = rest
        elif key == "elf":
            f, o = rest.split()
            sp["flags"], sp["osabi"] = int(f, 16), int(o)
        elif key == "soname":
            sp["soname"] = rest
        elif key == "needed":
            sp["needed"].append(rest)
        elif key == "verdef":
            ndx, flags, *names = rest.split()
            sp["verdef"].append((int(ndx), int(flags), names))
        elif key == "verneed":
            file_, other, flags, name = rest.split()
            sp["verneed"].append((file_, int(other), int(flags), name))
        elif key == "sym":
            value, size, info, other, kind, ver, name = rest.split(" ", 6)
            sp["sym"].append((int(value, 16), int(size), int(info), int(other), kind, int(ver, 16), name))
    return sp


def elf_hash(name):
    h = 0
    for c in name.encode():
        h = ((h << 4) + c) & 0xFFFFFFFF
        g = h & 0xF0000000
        if g:
            h ^= g >> 24
        h &= ~g & 0xFFFFFFFF
    return h


class Strtab:
    def __init__(self):
        self.b, self.at = bytearray(b"\0"), {"": 0}

    def add(self, s):
        if s not in self.at:
            self.at[s] = len(self.b)
            self.b += s.encode() + b"\0"
        return self.at[s]


def align(n, a=4):
    return (n + a - 1) // a * a


def write_stub(sp, path):
    st = Strtab()
    for n in sp["needed"]:
        st.add(n)
    if sp["soname"]:
        st.add(sp["soname"])
    # Section indices: 0 null, 1 .dynsym, 2 .dynstr, 3 .hash, 4 .gnu.version, 5 .gnu.version_d,
    # 6 .gnu.version_r, 7 .dynamic, 8.. one per symbol kind, then .shstrtab.
    kinds = [k for k in "TRWL" if any(s[4] == k for s in sp["sym"])]
    kind_ndx = {k: 8 + i for i, k in enumerate(kinds)}
    span = {k: max((s[0] + s[1] for s in sp["sym"] if s[4] == k), default=0) + 4 for k in kinds}

    syms = bytearray(16)
    first_global = 1
    for i, (value, size, info, other, kind, _ver, name) in enumerate(sp["sym"], 1):
        shndx = {"U": 0, "A": 0xFFF1}[kind] if kind in "UA" else kind_ndx[kind]
        syms += struct.pack("<IIIBBH", st.add(name), value, size, info, other, shndx)
        if info >> 4 == 0:  # STB_LOCAL
            first_global = i + 1
    versym = bytearray(2) + b"".join(struct.pack("<H", s[5]) for s in sp["sym"])

    verdef = bytearray()
    for j, (ndx, flags, names) in enumerate(sp["verdef"]):
        last = j == len(sp["verdef"]) - 1
        verdef += struct.pack("<HHHHIII", 1, flags, ndx, len(names), elf_hash(names[0]), 20,
                              0 if last else 20 + 8 * len(names))
        for k, n in enumerate(names):
            verdef += struct.pack("<II", st.add(n), 0 if k == len(names) - 1 else 8)
    files = []
    for f, *_ in sp["verneed"]:
        if f not in files:
            files.append(f)
    verneed = bytearray()
    for j, f in enumerate(files):
        aux = [v for v in sp["verneed"] if v[0] == f]
        last = j == len(files) - 1
        verneed += struct.pack("<HHIII", 1, len(aux), st.add(f), 16, 0 if last else 16 + 16 * len(aux))
        for k, (_f, other, flags, name) in enumerate(aux):
            verneed += struct.pack("<IHHII", elf_hash(name), flags, other, st.add(name),
                                   0 if k == len(aux) - 1 else 16)

    nsym = len(sp["sym"]) + 1
    nbucket = max(1, nsym // 2)
    bucket, chain = [0] * nbucket, [0] * nsym
    names = [""] + [s[6] for s in sp["sym"]]
    for i in range(1, nsym):
        b = elf_hash(names[i]) % nbucket
        chain[i], bucket[b] = bucket[b], i
    hashsec = struct.pack(f"<II{nbucket}I{nsym}I", nbucket, nsym, *bucket, *chain)

    # Lay out: header, phdrs, then the sections back to back; addresses equal file offsets.
    nph = 2 + (1 if "L" in kinds else 0)
    off = 52 + 32 * nph
    lay = {}
    for key, blob in (("dynsym", syms), ("dynstr", None), ("hash", hashsec), ("versym", versym),
                      ("verdef", verdef), ("verneed", verneed)):
        if key == "dynstr":
            continue
        off = align(off)
        lay[key] = (off, blob)
        off += len(blob)
    # .dynstr last of the tables: every name is in it by now; .dynamic needs its size.
    dynamic = []
    for n in sp["needed"]:
        dynamic.append((DT["NEEDED"], st.add(n)))
    if sp["soname"]:
        dynamic.append((DT["SONAME"], st.add(sp["soname"])))
    off = align(off)
    lay["dynstr"] = (off, bytes(st.b))
    off += len(st.b)
    dynamic += [(DT["HASH"], lay["hash"][0]), (DT["STRTAB"], lay["dynstr"][0]),
                (DT["SYMTAB"], lay["dynsym"][0]), (DT["STRSZ"], len(st.b)), (DT["SYMENT"], 16),
                (DT["VERSYM"], lay["versym"][0])]
    if verdef:
        dynamic += [(DT["VERDEF"], lay["verdef"][0]), (DT["VERDEFNUM"], len(sp["verdef"]))]
    if verneed:
        dynamic += [(DT["VERNEED"], lay["verneed"][0]), (DT["VERNEEDNUM"], len(files))]
    dynamic.append((DT["NULL"], 0))
    dynblob = b"".join(struct.pack("<iI", t, v) for t, v in dynamic)
    off = align(off)
    lay["dynamic"] = (off, dynblob)
    off += len(dynblob)
    shstr = Strtab()
    secnames = [".dynsym", ".dynstr", ".hash", ".gnu.version", ".gnu.version_d", ".gnu.version_r",
                ".dynamic"] + [KINDS[k][0] for k in kinds] + [".shstrtab"]
    for n in secnames:
        shstr.add(n)
    off = align(off)
    lay["shstrtab"] = (off, bytes(shstr.b))
    off += len(shstr.b)
    filesz = off
    base = align(filesz, 0x1000)  # the symbol sections sit above the tables, NOBITS
    kaddr = {}
    for k in kinds:
        kaddr[k] = base
        base = align(base + span[k], 0x1000)
    shoff = align(off)

    sh = [(0,) * 10]
    sh.append((shstr.at[".dynsym"], SHT_DYNSYM, SHF_ALLOC, lay["dynsym"][0], lay["dynsym"][0],
               len(syms), 2, first_global, 4, 16))
    sh.append((shstr.at[".dynstr"], SHT_STRTAB, SHF_ALLOC, lay["dynstr"][0], lay["dynstr"][0],
               len(st.b), 0, 0, 1, 0))
    sh.append((shstr.at[".hash"], SHT_HASH, SHF_ALLOC, lay["hash"][0], lay["hash"][0], len(hashsec),
               1, 0, 4, 4))
    sh.append((shstr.at[".gnu.version"], SHT_VERSYM, SHF_ALLOC, lay["versym"][0], lay["versym"][0],
               len(versym), 1, 0, 2, 2))
    sh.append((shstr.at[".gnu.version_d"], SHT_VERDEF if verdef else 0, SHF_ALLOC, lay["verdef"][0],
               lay["verdef"][0], len(verdef), 2, len(sp["verdef"]), 4, 0))
    sh.append((shstr.at[".gnu.version_r"], SHT_VERNEED if verneed else 0, SHF_ALLOC,
               lay["verneed"][0], lay["verneed"][0], len(verneed), 2, len(files), 4, 0))
    sh.append((shstr.at[".dynamic"], SHT_DYNAMIC, SHF_ALLOC | SHF_WRITE, lay["dynamic"][0],
               lay["dynamic"][0], len(dynblob), 2, 0, 4, 8))
    for k in kinds:
        name, typ, flags = KINDS[k]
        # A symbol's value is relative to nothing the linker checks; keeping the real one keeps the
        # real aliasing (a weak and a strong name at one address), which decides copy relocations.
        sh.append((shstr.at[name], typ, flags, 0, filesz, span[k], 0, 0, 4, 0))
    sh.append((shstr.at[".shstrtab"], SHT_STRTAB, 0, 0, lay["shstrtab"][0], len(shstr.b), 0, 0, 1, 0))

    out = bytearray(shoff + 40 * len(sh))
    ident = b"\x7fELF\x01\x01\x01" + bytes([sp["osabi"]]) + bytes(8)
    struct.pack_into("<16sHHIIIIIHHHHHH", out, 0, ident, 3, 40, 1, 0, 52, shoff, sp["flags"], 52, 32,
                     nph, 40, len(sh), len(sh) - 1)
    memtop = max([base] + [filesz])
    ph = [(1, 0, 0, 0, filesz, memtop, 7, 0x1000),  # PT_LOAD
          (2, lay["dynamic"][0], lay["dynamic"][0], lay["dynamic"][0], len(dynblob), len(dynblob), 6, 4)]
    if "L" in kinds:
        ph.append((7, filesz, kaddr["L"], kaddr["L"], 0, span["L"], 4, 4))  # PT_TLS
    for i, p in enumerate(ph):
        struct.pack_into("<IIIIIIII", out, 52 + 32 * i, *p)
    for key, (o, blob) in lay.items():
        out[o:o + len(blob)] = blob
    for i, s in enumerate(sh):
        struct.pack_into("<IIIIIIIIII", out, shoff + 40 * i, *s)
    with open(path, "wb") as fh:
        fh.write(out)


def build(out):
    n = 0
    for f in sorted(os.listdir(SPECS)):
        if not f.endswith(".txt"):
            continue
        sp = load(os.path.join(SPECS, f))
        d = os.path.join(out, sp["dir"])
        os.makedirs(d, exist_ok=True)
        write_stub(sp, os.path.join(d, f[:-4]))
        n += 1
    print(f"{n} stub libraries -> {out}")


def check(out):
    bad = 0
    for f in sorted(os.listdir(SPECS)):
        if not f.endswith(".txt"):
            continue
        want = [l.rstrip("\n") for l in open(os.path.join(SPECS, f)) if not l.startswith(("#", "dir "))]
        sp = load(os.path.join(SPECS, f))
        got, _ = gen_one(os.path.join(out, sp["dir"], f[:-4]))
        # A stand-in's sections are its own, so a symbol's kind is checked, not its section index.
        if want != got:
            bad += 1
            diff = [(a, b) for a, b in zip(want, got) if a != b][:2]
            print(f"{f}: {len(want)} vs {len(got)} lines; first differences {diff}")
    print("all stand-ins match their specs" if not bad else f"{bad} differ")
    return 1 if bad else 0


if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "gen":
        gen()
    elif cmd == "build" and len(sys.argv) == 3:
        build(os.path.abspath(sys.argv[2]))
    elif cmd == "check" and len(sys.argv) == 3:
        sys.exit(check(os.path.abspath(sys.argv[2])))
    else:
        print(__doc__)
        sys.exit(2)
