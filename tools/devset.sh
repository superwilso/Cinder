#!/bin/bash
# devset.sh — change Cinder's settings on the player over adb, no screens to click through.
#
#   tools/devset.sh [PATTERN]                 list settings (key=value), filtered by PATTERN
#   tools/devset.sh KEY=VALUE [KEY=VALUE…]    set them; a dev build re-reads the file at once
#   tools/devset.sh restore                   put back the settings saved before the first change
#   tools/devset.sh adv [PATTERN]             list /contents/cinder_advanced.conf
#   tools/devset.sh adv KEY=VALUE|-KEY …      add/replace or remove advanced lines (read at start:
#                                             most take effect after a restart)
#
# Settings live in /contents/cinder_settings.conf with a copy in /data/cinder (cinder-ffi's mirror).
# Both are written; the dev build notices /tmp/cinder_settings.reload's new time, loads the file as
# at boot and re-applies sound and EQ. A stable build has no adb, so this is dev-only.
# Before the first change the current file is kept as /data/cinder/cinder_settings.conf.devbak.
set -eu
SET=/contents/cinder_settings.conf MIRROR=/data/cinder/cinder_settings.conf
ADV=/contents/cinder_advanced.conf BAK=/data/cinder/cinder_settings.conf.devbak
adb get-state >/dev/null 2>&1 || { echo "no adb device"; exit 1; }
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT

pull() { adb shell "cat $1 2>/dev/null" | tr -d '\r' > "$T/f"; }
push() {   # push <device path...>: write $T/f over each, keeping the target's owner and mode
    adb push "$T/f" /tmp/cinder_devset.tmp >/dev/null
    for d in "$@"; do adb shell "cat /tmp/cinder_devset.tmp > $d"; done
    adb shell "rm /tmp/cinder_devset.tmp"
}
# edit <KEY=VALUE|-KEY|KEY>…: change $T/f in place; prints what changed
edit() {
    python3 - "$T/f" "$@" <<'EOF'
import sys
path, args = sys.argv[1], sys.argv[2:]
lines = open(path).read().splitlines()
for a in args:
    drop = a.startswith("-")
    k, _, v = a.lstrip("-").partition("=")
    line = k if drop or "=" not in a else f"{k}={v}"
    hits = [i for i, l in enumerate(lines) if l.split("=", 1)[0].strip() == k]
    old = lines[hits[-1]] if hits else None
    for i in reversed(hits):
        del lines[i]
    if not drop:
        lines.append(line)
    print(f"  {k}: {old.partition('=')[2] if old and '=' in old else old or '(unset)'} -> "
          f"{'(removed)' if drop else v if '=' in a else 'on'}")
open(path, "w").write("\n".join(lines) + "\n")
EOF
}

# A change is KEY=VALUE or -KEY with a plain key; anything else alone is a filter. A filter that
# merely contains '=' (11-18: "^eq=") once went in as a setting.
is_change() { [[ "$1" =~ ^-?[A-Za-z_][A-Za-z0-9_]*(=|$) && ( "$1" == *=* || "$1" == -* ) ]]; }
is_filter() { [ $# -eq 0 ] || { [ $# -eq 1 ] && ! is_change "$1"; }; }
check_changes() { for a in "$@"; do is_change "$a" || { echo "not KEY=VALUE or -KEY: $a"; exit 2; }; done; }

case "${1:-}" in
restore)
    adb shell "[ -s $BAK ]" || { echo "no backup at $BAK"; exit 1; }
    adb shell "cat $BAK > $SET; cat $BAK > $MIRROR; touch /tmp/cinder_settings.reload"
    echo "restored settings from $BAK" ;;
adv)
    shift; pull "$ADV"
    if is_filter "$@"; then grep -v '^#' "$T/f" | grep -i -- "${1:-}" || echo "(nothing)"; exit 0; fi
    check_changes "$@"; edit "$@"; push "$ADV"
    echo "advanced settings are read at start: restart the player for most of them" ;;
*)
    pull "$SET"
    if is_filter "$@"; then grep -i -- "${1:-}" "$T/f" || echo "(nothing)"; exit 0; fi
    check_changes "$@"
    adb shell "[ -s $BAK ] || cat $SET > $BAK"
    edit "$@"; push "$SET" "$MIRROR"
    adb shell "touch /tmp/cinder_settings.reload"
    echo "set; a dev build applies it within a second" ;;
esac
