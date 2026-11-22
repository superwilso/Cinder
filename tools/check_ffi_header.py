#!/usr/bin/env python3
"""check_ffi_header.py — does player/cinder-ffi/include/cinder.h still match the Rust exports?

The header is hand-written and the C++ shell trusts it; nothing compiled the two against each
other, so a Rust signature change (an int that became a u64, a new argument) linked fine and
broke at run time. This compares every prototype in the header with the `pub extern "C" fn` of
the same name: the function exists, the argument count, and each argument's and the return's
size class (int, 64-bit, float, double, pointer, void).

    tools/check_ffi_header.py            exit 1 on any mismatch
    tools/check_ffi_header.py --self-test
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
HEADER = ROOT / "player/cinder-ffi/include/cinder.h"
RUST = sorted((ROOT / "player/cinder-ffi/src").glob("*.rs"))


def c_class(t):
    t = re.sub(r"\b(const|volatile|struct|enum|extern)\b", "", t).strip()
    if "*" in t or "(" in t:
        return "ptr"
    t = " ".join(t.split())
    if t in ("", "void"):
        return "void"
    if t in ("float",):
        return "f32"
    if t in ("double",):
        return "f64"
    if t in ("long long", "unsigned long long", "int64_t", "uint64_t", "long long int"):
        return "i64"
    if t in ("bool", "char", "unsigned char", "signed char", "uint8_t", "int8_t"):
        return "i8"
    if t in ("short", "unsigned short", "int16_t", "uint16_t"):
        return "i16"
    return "i32"  # int, unsigned, long (32-bit ARM), size_t, int32_t, uint32_t…


def rust_class(t):
    t = t.strip()
    if not t or t == "()":
        return "void"
    if t.startswith(("*", "&", "Option<", "extern", "unsafe extern")):
        return "ptr"
    t = t.split("::")[-1]
    return {
        "f32": "f32", "f64": "f64", "c_float": "f32", "c_double": "f64",
        "i64": "i64", "u64": "i64", "c_longlong": "i64", "c_ulonglong": "i64",
        "i8": "i8", "u8": "i8", "bool": "i8", "c_char": "i8", "c_uchar": "i8", "c_schar": "i8",
        "i16": "i16", "u16": "i16", "c_short": "i16", "c_ushort": "i16",
    }.get(t, "i32")


def split_top(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "<(":
            depth += 1
        elif ch in ">)":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    return [p.strip() for p in out if p.strip()]


def parse_header(text):
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    text = re.sub(r"//[^\n]*", " ", text)
    text = re.sub(r"^\s*#.*$", " ", text, flags=re.M)
    protos = {}
    for m in re.finditer(r"([A-Za-z_][\w\s\*]*?)\b(cinder_\w+)\s*\(([^;{]*?)\)\s*;", text):
        ret, name, params = m.group(1), m.group(2), m.group(3)
        ps = [] if params.strip() in ("", "void") else split_top(params)
        # drop the parameter NAME: the last identifier, unless the whole thing is a type
        def ptype(p):
            if "(" in p:
                return "*"  # function pointer
            p2 = re.sub(r"\[[^\]]*\]", "*", p)
            toks = re.findall(r"\w+|\*", p2)
            if len(toks) > 1 and re.match(r"\w+$", toks[-1]) and toks[-1] not in ("int", "char", "long", "float", "double", "void", "unsigned", "short"):
                toks = toks[:-1]
            return " ".join(toks)
        protos[name] = (c_class(ret.split()[-1] if ret.split() and ret.split()[-1] == "*" else ret),
                        [c_class(ptype(p)) for p in ps])
    return protos


def parse_rust(texts):
    fns = {}
    for text in texts:
        for m in re.finditer(r'pub\s+(?:unsafe\s+)?extern\s+"C"\s+fn\s+(\w+)\s*\(', text):
            i, depth = m.end(), 1
            while depth:
                depth += {"(": 1, ")": -1}.get(text[i], 0)
                i += 1
            params = text[m.end():i - 1]
            ret = re.match(r"\s*(?:->\s*([^{]+?))?\s*(?:where[^{]*)?\{", text[i:])
            ps = [p.split(":", 1)[1] for p in split_top(params) if ":" in p]
            fns[m.group(1)] = (rust_class(ret.group(1) or "" if ret else ""), [rust_class(p) for p in ps])
    return fns


# Exported for one C++ file that declares them itself, deliberately not in the shared header.
DECLARED_ELSEWHERE = {
    "cinder_flac_open", "cinder_flac_info", "cinder_flac_read", "cinder_flac_seek",
    "cinder_flac_close",  # cinder-home/src/probe.cpp, tools/playershim_selftest.cpp
    "cinder_font_probe",  # cinder-home/src/probe.cpp
}


def compare(header, rust):
    errs = []
    for name, (ret, ps) in sorted(header.items()):
        if name not in rust:
            continue  # declared for C/C++ code (cinder-audio, the shell's own helpers): not ours
        rret, rps = rust[name]
        if len(ps) != len(rps):
            errs.append(f"{name}: header has {len(ps)} argument(s), Rust has {len(rps)}")
            continue
        for i, (a, b) in enumerate(zip(ps, rps)):
            if a != b:
                errs.append(f"{name}: argument {i + 1} is {a} in the header, {b} in Rust")
        if ret != rret:
            errs.append(f"{name}: returns {ret} in the header, {rret} in Rust")
    for name in sorted(set(rust) - set(header) - DECLARED_ELSEWHERE):
        if name.startswith("cinder_"):
            errs.append(f"{name}: exported by Rust but not declared in cinder.h")
    return errs


def self_test():
    h = parse_header("""
        /* x */ int cinder_a(int x, const char* s);
        unsigned long long cinder_b(void);
        void cinder_c(float v, int (*cb)(int));
        int cinder_d(int x);
    """)
    r = parse_rust(["""
        #[no_mangle] pub extern "C" fn cinder_a(x: libc::c_int, s: *const c_char) -> libc::c_int {}
        #[no_mangle] pub extern "C" fn cinder_b() -> u64 {}
        #[no_mangle] pub unsafe extern "C" fn cinder_c(v: f32, cb: Option<extern "C" fn(i32) -> i32>) {}
        #[no_mangle] pub extern "C" fn cinder_d(x: u64) -> libc::c_int {}
        #[no_mangle] pub extern "C" fn cinder_e() {}
    """])
    errs = compare(h, r)
    assert errs == [
        "cinder_d: argument 1 is i32 in the header, i64 in Rust",
        "cinder_e: exported by Rust but not declared in cinder.h",
    ], errs
    print("check_ffi_header: self-test ok")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
        sys.exit(0)
    header = parse_header(HEADER.read_text())
    rust = parse_rust(p.read_text() for p in RUST)
    errs = compare(header, rust)
    for e in errs:
        print("  " + e)
    print(f"check_ffi_header: {len(header)} prototypes, {len(rust)} Rust exports, {len(errs)} mismatch(es)")
    sys.exit(1 if errs else 0)
