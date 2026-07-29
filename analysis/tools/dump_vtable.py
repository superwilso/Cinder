#!/usr/bin/env python3
"""Dump a Sony pst service-CLIENT vtable straight out of the .so — no Ghidra.

WHY. Every `pst::services::*Client` class exports only its FACTORY; every method is virtual, so
calling one from our own code means knowing its vtable INDEX. The libs are built -fno-rtti, so
there is no typeinfo string to anchor on, and they are stripped, so there is no `_ZTV...` symbol
either. What IS deterministic is the factory itself: it allocates the object and writes the vptr,
and the pointer it writes is `group_base + 8` (slot 0 = the first virtual, after the standard
[offset-to-top, typeinfo] header).

So: disassemble `<Class>Factory::CreateInstance`, recover `group_base` through the GOT load it
does, then read the vtable slots out of `.data.rel.ro` via the R_ARM_RELATIVE relocations that
fill them in at load time.

    python3 analysis/tools/dump_vtable.py <lib.so> <FactoryCreateInstanceSymbol> [slots]

Slot addresses are printed with the nearest preceding `@@Base` symbol, which is how a slot gets a
name when the class's own methods are not exported: the ones that ARE named tell you the ordering,
and the rest are identified by what they transact (see RE_findings).
"""
import re, subprocess, sys, struct

OBJDUMP = "/home/sony/arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-objdump"
READELF = "arm-linux-gnueabihf-readelf"


def sections(lib):
    out = subprocess.run([READELF, "-S", "-W", lib], capture_output=True, text=True).stdout
    secs = {}
    for m in re.finditer(r"\[\s*\d+\]\s+(\S+)\s+\S+\s+([0-9a-f]+)\s+([0-9a-f]+)\s+([0-9a-f]+)", out):
        secs[m.group(1)] = (int(m.group(2), 16), int(m.group(3), 16), int(m.group(4), 16))
    return secs  # name -> (vaddr, file_off, size)


def read_word(blob, secs, vaddr):
    for _, (va, off, size) in secs.items():
        if va and va <= vaddr < va + size:
            return struct.unpack_from("<I", blob, off + (vaddr - va))[0]
    return None


def relocs(lib):
    """vaddr -> (type, addend/target). R_ARM_RELATIVE targets live in the slot itself."""
    out = subprocess.run([READELF, "-r", "-W", lib], capture_output=True, text=True).stdout
    r = {}
    for line in out.splitlines():
        m = re.match(r"^([0-9a-f]{8})\s+[0-9a-f]{8}\s+(R_ARM_\S+)\s*(\S+)?", line)
        if m:
            r[int(m.group(1), 16)] = (m.group(2), m.group(3))
    return r


_DIS = {}


def disasm(lib):
    """objdump -d, ONCE. It is ~80k lines on these libs and every helper wants it."""
    if lib not in _DIS:
        _DIS[lib] = subprocess.run([OBJDUMP, "-d", lib], capture_output=True, text=True).stdout
    return _DIS[lib]


def func_symbols(lib):
    """Sorted (vaddr, name) of every function objdump can label — includes @@Base locals."""
    out = disasm(lib)
    syms = []
    for m in re.finditer(r"^([0-9a-f]{8}) <([^>]+)>:", out, re.M):
        syms.append((int(m.group(1), 16), m.group(2)))
    syms.sort()
    return syms


def name_for(syms, addr):
    a = addr & ~1  # Thumb bit
    lo, best = 0, None
    for va, nm in syms:
        if va <= a:
            best = (va, nm)
        else:
            break
    if not best:
        return "?"
    off = a - best[0]
    return best[1] if off == 0 else f"{best[1]}+0x{off:x}"


def group_base(lib, blob, secs, sym):
    """Recover the vtable GROUP base from the factory's `ldr rN,[pc,#k]; add rN,pc; ldr; ldr`."""
    # Line scan, not a DOTALL regex: these disassemblies are ~80k lines and a `(.*?)(?=^\S)`
    # over them backtracks for minutes.
    lines, body, inside = disasm(lib).splitlines(), [], False
    for ln in lines:
        h = re.match(r"^([0-9a-f]{8}) <([^>]+)>:", ln)
        if h:
            if inside:
                break
            inside = h.group(2).startswith(sym)
            continue
        if inside:
            body.append(ln)
    if not body:
        sys.exit(f"symbol not found: {sym}")
    body = "\n".join(body)
    # The literal the `add rN,pc` folds against, and the pc it uses.
    # `ldr rN,[pc,#k]` and its matching `add rN,pc` are separated by other instructions, so pair
    # them up by register rather than trying to match one span.
    lds = re.findall(r"([0-9a-f]+):\s+\S+\s+ldr\s+r(\d+), \[pc, #(\d+)\]", body)
    adds = re.findall(r"([0-9a-f]+):\s+\S+\s+add\s+r(\d+), pc", body)
    cand = [(l, a) for l in lds for a in adds if l[1] == a[1] and int(a[0], 16) > int(l[0], 16)]
    if not cand:
        sys.exit("could not find the pc-relative GOT load in the factory")
    # Score every candidate and take the best. "Resolves into .data.rel.ro" is necessary but NOT
    # sufficient — .data.rel.ro can abut .dynamic, and on libBtCommonService the stack-guard load
    # resolved to a .dynamic pointer that passed a bare range check and produced a table of
    # dynamic tags. A real vtable group has function pointers in .text a few slots in, so that is
    # what gets checked.
    drr = secs.get(".data.rel.ro", (0, 0, 0))
    txt = secs.get(".text", (0, 0, 0))
    rel = relocs(lib)
    best = None
    for (laddr, _r, k), (aaddr, _r2) in cand:
        lit_at = ((int(laddr, 16) + 4) & ~3) + int(k)
        disp = read_word(blob, secs, lit_at)
        if disp is None:
            continue
        got = (disp + int(aaddr, 16) + 4) & 0xFFFFFFFF
        if got % 4:
            continue
        gb = int(rel[got][1], 16) if (got in rel and rel[got][1]) else read_word(blob, secs, got)
        if not gb or not (drr[0] <= gb < drr[0] + drr[2]):
            continue
        # How many of the first slots after the [offset-to-top, typeinfo] header point into .text?
        hits = 0
        for i in range(8):
            va = gb + 8 + 4 * i
            t = int(rel[va][1], 16) if (va in rel and rel[va][1]) else read_word(blob, secs, va)
            if t and txt[0] <= (t & ~1) < txt[0] + txt[2]:
                hits += 1
        if hits and (best is None or hits > best[0]):
            best = (hits, gb, got)
    if best:
        return best[1], best[2]
    sys.exit("no candidate GOT load resolved into .data.rel.ro")


def _unused(lib, blob, secs, sym):
    lit_at = ((int(laddr, 16) + 4) & ~3) + int(k)
    disp = read_word(blob, secs, lit_at)
    got = (disp + int(aaddr, 16) + 4) & 0xFFFFFFFF
    rel = relocs(lib)
    # The GOT slot is filled by a relocation; its target is the vtable group.
    if got in rel:
        _, tgt = rel[got]
        if tgt:
            return int(tgt, 16), got
    w = read_word(blob, secs, got)
    return (w or 0), got


def stub_name(lib, blob, secs, addr):
    """The method name, out of the stub's own trace tag.

    Every client stub opens by building a `std::string` and handing it to
    `ServiceManager::TimeMeasureHolder` — and that string is
    `"<Class>::<Method>"`. So the names are all still in the binary even though the methods are
    not exported: resolve each pc-relative literal the stub loads and keep the one that decodes to
    a `::`-bearing string. Validated against the hand-RE'd LDAC table (slot 12 came back
    `BtTransmitterServiceClient::SetCurrentSource`, which is what `btclient.c` already had).
    """
    # Scan by ADDRESS, not by function header: these libs export only the factory, so every stub
    # sits inside one enormous `<Factory::CreateInstance@@Base>` block and has no header of its
    # own. The tag is built in the prologue, so a short window is enough.
    want = addr & ~1
    body = []
    for ln in disasm(lib).splitlines():
        m = re.match(r"^\s+([0-9a-f]+):\t", ln)
        if not m:
            continue
        a = int(m.group(1), 16)
        if want <= a < want + 0x80:
            body.append(ln)
        elif body:
            break
    lds = re.findall(r"([0-9a-f]+):\s+\S+\s+ldr\s+r(\d+), \[pc, #(\d+)\]", "\n".join(body))
    adds = dict((r, a) for a, r in re.findall(r"([0-9a-f]+):\s+\S+\s+add\s+r(\d+), pc",
                                              "\n".join(body)))
    for laddr, reg, k in lds:
        if reg not in adds:
            continue
        lit = ((int(laddr, 16) + 4) & ~3) + int(k)
        w = read_word(blob, secs, lit)
        if w is None:
            continue
        ptr = (w + int(adds[reg], 16) + 4) & 0xFFFFFFFF
        raw = None
        for _, (va, off, size) in secs.items():
            if va and va <= ptr < va + size:
                raw = blob[off + (ptr - va): off + (ptr - va) + 96]
                break
        if not raw:
            continue
        txt = raw.split(b"\0")[0]
        try:
            t = txt.decode("ascii")
        except UnicodeDecodeError:
            continue
        if "::" in t and t.replace("::", "").replace("_", "").isalnum():
            return t
    return ""


def main():
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    lib, sym = sys.argv[1], sys.argv[2]
    slots = int(sys.argv[3]) if len(sys.argv) > 3 else 48
    blob = open(lib, "rb").read()
    secs = sections(lib)
    gb, got = group_base(lib, blob, secs, sym)
    print(f"# {lib}\n# {sym}\n# GOT slot 0x{got:08x} -> vtable group base 0x{gb:08x}")
    print(f"# primary vptr = group+8 = 0x{gb + 8:08x}   (slot 0 is the first virtual)")
    rel = relocs(lib)
    syms = func_symbols(lib)
    for i in range(slots):
        va = gb + 8 + 4 * i
        tgt = None
        if va in rel:
            _, t = rel[va]
            tgt = int(t, 16) if t else None
        if tgt is None:
            tgt = read_word(blob, secs, va)
        if not tgt:
            print(f"{i:3}  0x{va:08x}  <null / end of table>")
            continue
        nm = stub_name(lib, blob, secs, tgt)
        print(f"{i:3}  0x{va:08x}  -> 0x{tgt:08x}  {nm or name_for(syms, tgt)}")


main()
