#!/usr/bin/env python3
"""reuse_check.py: fail when code the tree already shares is written out again.

Every rule below is a pattern a cleanup removed (CONTRIBUTING.md, "Reuse before you write"). A rule
that fires names the helper to use instead. To change a rule, change the table in CONTRIBUTING.md
in the same commit.

    python3 tools/reuse_check.py              check the tree
    python3 tools/reuse_check.py --self-test  prove every rule still fires on its own example
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FFI = "player/cinder-ffi/src/*.rs"
UI = "player/cinder-ui/src/*.rs"
PLAYER = "player/*/src/**/*.rs"

# On/off settings the FFI loader reads by hand. Anything else that loads as `v == "1"` is an
# opt-in switch and belongs in cinder-ui/src/opt.rs, where save, load, Reset and the row come free.
# A real non-opt-in on/off setting (one that is ON by default, or not on a Settings switch row) can
# be added here.
PLAIN_SWITCHES = {
    "viz_on", "ignore_the", "sensme_follow_time", "viz_peaks", "onboarding", "bt_enhanced",
    "bt_idle_off", "mono", "ambient_on", "big_view",
}

# (name, files, pattern, how many may exist, what to use instead, an example the rule must catch)
RULES = [
    ("ffi-read", FFI,
     r"match cell\(\)\.lock\(\)\.unwrap\(\)\.as_ref\(\) \{\s*Some\(r\)(?: if [^\n]+)? => [^\n]+,\s*(?:None|_) => 0,\s*\}",
     0, "read(|r| ...) in cinder-ffi/src/lib.rs",
     "match cell().lock().unwrap().as_ref() {\n    Some(r) => r.app.x(),\n    None => 0,\n}"),
    ("ffi-write", FFI,
     r"if let Some\(r\) = cell\(\)\.lock\(\)\.unwrap\(\)\.as_mut\(\) \{\n(?![^\n]*\breturn\b)(?!\s*f\(r\);)[^\n]*;\n(?:\s*r\.dirty = true;\n)?\s*\}",
     0, "write(|r| ...) or repaint(|r| ...) in cinder-ffi/src/lib.rs",
     "if let Some(r) = cell().lock().unwrap().as_mut() {\n    r.app.x(1);\n    r.dirty = true;\n}"),
    ("probe-start", "cinder-home/src/probe.cpp",
     r"install_diagnostics\(\);\s*pst::core::Framework& fw = pst::core::Framework::GetReference\(\);\s*wd_arm\(15\);\s*"
     r"fw\.StartForApplication\(std::function<void\(\)>\(&pump_finish\), true\);\s*wd_disarm\(\);\s*g_pump_run = true;",
     1, "start_probe_framework() in probe.cpp",
     "install_diagnostics();\npst::core::Framework& fw = pst::core::Framework::GetReference();\nwd_arm(15);\n"
     "fw.StartForApplication(std::function<void()>(&pump_finish), true);\nwd_disarm();\ng_pump_run = true;"),
    ("next-in", UI, r"Self::ALL\[\(\w+ \+ 1\) % Self::ALL\.len\(\)\]",
     0, "crate::next_in(&Self::ALL, self)", "Self::ALL[(i + 1) % Self::ALL.len()]"),
    ("from-token", UI, r"Self::ALL\s*\.(?:into_)?iter\(\)\s*\.find\(\|\w+\| \w+\.token\(\) == s\)\s*\.unwrap_or_default\(\)",
     0, "crate::from_token(&Self::ALL, Self::token, s)",
     "Self::ALL\n    .into_iter()\n    .find(|b| b.token() == s)\n    .unwrap_or_default()"),
    ("port-80", FFI, r'format!\("\{host\}:80"\)', 1, "cast::resolve", 'format!("{host}:80")'),
    ("pct-encode", PLAYER, r'format!\("%\{\w*:02X\}"\)', 1, "drive::pct_encode", 'format!("%{b:02X}")'),
    ("png", "player/*/Cargo.toml", r"(?m)^image\s*=", 0, "the png crate the player already uses", 'image = "0.25"'),
    ("opt-in", FFI, r'"(?!(?:' + "|".join(sorted(PLAIN_SWITCHES)) + r')")\w+" => r\.app\.set_\w+\(v == "1"\)',
     0, "a variant in cinder-ui/src/opt.rs (or PLAIN_SWITCHES here, if it is not an opt-in)",
     '"new_switch" => r.app.set_new_switch(v == "1")'),
]

# Shared helpers: each is defined once. A second definition is a copy that will drift.
DEFINED_ONCE = [
    # Only names nothing else uses: `read`, `write` and `resolve` mean other things in other modules,
    # and the rules above already catch their copies.
    (FFI, r"fn (tracks_in_order|write_atomic|json_str|private_tmp)\b"),
    (FFI, r"pub fn (pct_encode|pct_decode|url_decode)\b"),
    (UI, r"fn (next_in|from_token)<"),
    ("installer/src/*.rs", r"fn (hex)\(b: &\[u8\]\)"),
    ("cinder-home/src/probe.cpp", r"Framework& (start_probe_framework)\("),
]


def files(glob):
    return sorted(p for p in ROOT.glob(glob) if p.is_file())


def line_of(text, pos):
    return text.count("\n", 0, pos) + 1


def check():
    bad = []
    for name, glob, pat, allowed, use, _ in RULES:
        hits = [(p, line_of(t, m.start())) for p in files(glob)
                for t in [p.read_text(errors="ignore")] for m in re.finditer(pat, t)]
        if len(hits) > allowed:
            for p, ln in hits[allowed:] if allowed else hits:
                bad.append(f"{p.relative_to(ROOT)}:{ln}: [{name}] use {use}")
    for glob, pat in DEFINED_ONCE:
        seen = {}
        for p in files(glob):
            t = p.read_text(errors="ignore")
            for m in re.finditer(pat, t):
                seen.setdefault(m.group(1), []).append(f"{p.relative_to(ROOT)}:{line_of(t, m.start())}")
        for fn, where in seen.items():
            if len(where) > 1:
                bad.append(f"{where[1]}: [defined-once] `{fn}` already exists at {where[0]}; reuse it")
    return bad


def self_test():
    for name, _, pat, _, _, example in RULES:
        assert re.search(pat, example), f"rule {name} no longer catches its own example"
    assert not re.search(RULES[-1][2], '"mono" => r.app.set_mono(v == "1")'), "opt-in rule caught a plain switch"
    print(f"reuse_check: all {len(RULES)} rules catch their examples")


if __name__ == "__main__":
    if "--self-test" in sys.argv:
        self_test()
        sys.exit(0)
    problems = check()
    for p in problems:
        print(p)
    if problems:
        print(f"\nreuse_check: {len(problems)} place(s) repeat shared code. See CONTRIBUTING.md, "
              "\"Reuse before you write\".")
        sys.exit(1)
    print("reuse_check: no shared code written out again")
