#!/usr/bin/env python3
"""firmware_catalogue.py — describe an extracted Walkman root filesystem without copying it.

    python3 tools/firmware_catalogue.py <rootfs> <out-dir> [--label TEXT] [--vendor-prefix PATH ...]

For people doing RE without the firmware (or without the device): what is in the image, where,
how big, which binaries need which libraries, and every symbol Sony's own binaries export,
demangled. It copies no firmware file except plain-text init scripts and build properties, and
records a SHA-256 per file so someone holding the real image can check they have the same one.

Writes:
  README.md            what this is and how it was produced
  manifest.tsv         path, type, mode, size, sha256 (files), link target (symlinks)
  elf.tsv              path, class/machine, SONAME, NEEDED, interpreter, stripped?
  symbols/<path>.txt   demangled dynamic exports of every ELF under the vendor prefixes
  prototypes.txt       Sony's demangled C++ prototypes kept in .rodata (`strings | grep ^virtual`)
  init/                init*.rc, ueventd*.rc and *.prop files found in the tree
"""
import argparse
import hashlib
import os
import re
import stat
import subprocess
import sys

READELF = "arm-linux-gnueabihf-readelf"
NM = "arm-linux-gnueabihf-nm"


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def is_elf(path):
    try:
        with open(path, "rb") as f:
            return f.read(4) == b"\x7fELF"
    except OSError:
        return False


def run(*cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, errors="replace")
    return r.stdout


def elf_row(root, rel):
    p = os.path.join(root, rel)
    hdr = run(READELF, "-h", p)
    dyn = run(READELF, "-d", p)
    phd = run(READELF, "-l", p)
    sec = run(READELF, "-S", p)
    machine = re.search(r"Machine:\s+(.+)", hdr)
    kind = re.search(r"Type:\s+(\S+)", hdr)
    soname = re.search(r"\(SONAME\).*\[(.*)\]", dyn)
    needed = re.findall(r"\(NEEDED\).*\[(.*)\]", dyn)
    interp = re.search(r"Requesting program interpreter: (.*)\]", phd)
    stripped = ".symtab" not in sec
    return "\t".join([rel, kind.group(1) if kind else "?", machine.group(1).strip() if machine else "?",
                      soname.group(1) if soname else "", ",".join(needed),
                      interp.group(1) if interp else "", "stripped" if stripped else "symbols"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("rootfs")
    ap.add_argument("out")
    ap.add_argument("--label", default="")
    ap.add_argument("--vendor-prefix", action="append", default=[],
                    help="tree(s) whose ELF exports are listed in symbols/ (default: vendor/sony, usr/local)")
    a = ap.parse_args()
    root = os.path.abspath(a.rootfs)
    prefixes = a.vendor_prefix or ["vendor/sony", "usr/local", "devel/usr/local"]
    os.makedirs(os.path.join(a.out, "symbols"), exist_ok=True)
    os.makedirs(os.path.join(a.out, "init"), exist_ok=True)

    manifest, elves, protos = [], [], set()
    for dirpath, dirnames, files in os.walk(root):
        dirnames.sort()
        for name in sorted(dirnames + files):
            p = os.path.join(dirpath, name)
            rel = os.path.relpath(p, root)
            st = os.lstat(p)
            mode = oct(stat.S_IMODE(st.st_mode))
            if stat.S_ISLNK(st.st_mode):
                manifest.append(f"{rel}\tlink\t{mode}\t\t\t{os.readlink(p)}")
            elif stat.S_ISDIR(st.st_mode):
                if name in dirnames:
                    manifest.append(f"{rel}\tdir\t{mode}\t\t\t")
            elif stat.S_ISREG(st.st_mode):
                manifest.append(f"{rel}\tfile\t{mode}\t{st.st_size}\t{sha256(p)}\t")
                if is_elf(p):
                    elves.append(rel)
                base = os.path.basename(rel)
                if re.fullmatch(r"(init.*\.rc|ueventd.*\.rc|.*\.prop|fstab.*)", base) and st.st_size < 1 << 20:
                    flat = rel.replace("/", "__")
                    with open(p, "rb") as src, open(os.path.join(a.out, "init", flat), "wb") as dst:
                        dst.write(src.read())

    with open(os.path.join(a.out, "manifest.tsv"), "w") as f:
        f.write("path\ttype\tmode\tsize\tsha256\tlink_target\n")
        f.write("\n".join(manifest) + "\n")
    with open(os.path.join(a.out, "elf.tsv"), "w") as f:
        f.write("path\ttype\tmachine\tsoname\tneeded\tinterpreter\tsymtab\n")
        for rel in elves:
            f.write(elf_row(root, rel) + "\n")

    listed = 0
    for rel in elves:
        if not any(rel.startswith(pfx + "/") for pfx in prefixes):
            continue
        p = os.path.join(root, rel)
        syms = run(NM, "-D", "--defined-only", "-C", p)
        if syms.strip():
            out = os.path.join(a.out, "symbols", rel.replace("/", "__") + ".txt")
            with open(out, "w") as f:
                f.write(syms)
            listed += 1
        for s in run("strings", "-n", "8", p).splitlines():
            if s.startswith("virtual ") or re.match(r"^(static )?[\w:<>,\s\*&]+ pst::[\w:]+\(.*\)", s):
                protos.add(s)
    with open(os.path.join(a.out, "prototypes.txt"), "w") as f:
        f.write("\n".join(sorted(protos)) + "\n")

    with open(os.path.join(a.out, "README.md"), "w") as f:
        f.write(f"# Firmware catalogue — {a.label or os.path.basename(root)}\n\n"
                "Generated by Cinder's `tools/firmware_catalogue.py` from an extracted root filesystem. "
                "No firmware binary is included: a file's presence, size, mode and SHA-256 are, so an "
                "image you extract yourself can be checked against this one.\n\n"
                f"* `manifest.tsv` — {len(manifest)} entries\n"
                f"* `elf.tsv` — {len(elves)} ELF files: type, machine, SONAME, NEEDED, interpreter, symtab\n"
                f"* `symbols/` — demangled dynamic exports of {listed} binaries under "
                f"{', '.join('`' + p + '`' for p in prefixes)}\n"
                f"* `prototypes.txt` — {len(protos)} demangled prototypes Sony's binaries keep as strings\n"
                "* `init/` — init scripts and property files found in the tree (plain text)\n")
    print(f"{a.out}: {len(manifest)} entries, {len(elves)} ELF, {listed} symbol lists, {len(protos)} prototypes")


if __name__ == "__main__":
    main()
