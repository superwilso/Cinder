#!/usr/bin/env bash
# map_player.sh — copy what the player's code reads (sysfs, procfs, properties, the installed
# files) into a directory tree on this machine, for offline RE and for the virtual player.
#
#   tools/map_player.sh OUT_DIR        needs adb (the dev channel) and a player on the cable
#
# READ-ONLY, and it reads nothing that has ever done harm. Nothing on the player is changed: the script and
# its path list sit in /data/local/tmp while it runs, and every value comes back over adb, base64. Skipped, whatever the source asks for:
#   /proc/regmon/*                 a read is a write: you set `target` first
#   /proc/icx_audio_*              a PARTIAL read wedges the codec until a restart
#   /sys/power/wait_for_fb_*       blocks, and is half of the app's suspend handshake
#   */duty, /dev/*, debugfs        the backlight PWM panics on a write; device nodes are not data
# (busybox 1.23 here: its base64 has no -w, so the line breaks go with tr, and its timeout takes -t.)
# Every file read is under `timeout 2`, so a node that blocks costs two seconds, not the run.
#
# Layout of OUT_DIR:
#   fs/<absolute path>    each file's contents, at its device path: point the harness's
#                         CINDER_HARNESS_FSROOT (or the simulator) at OUT_DIR/fs
#   cmd/<name>.txt        command output: getprop, ps, mounts, dmesg, file listings
#   skipped.txt           what was asked for and not read, and why
#   run/                  the virtual player's working copy (cargo run -p cinder-sim --bin player)
set -euo pipefail

out="${1:?usage: tools/map_player.sh OUT_DIR}"
here="$(cd "$(dirname "$0")/.." && pwd)"
command -v adb >/dev/null || { echo "map_player: adb not found" >&2; exit 1; }
[ "$(adb get-state 2>/dev/null)" = device ] || { echo "map_player: no player on adb" >&2; exit 1; }
mkdir -p "$out/fs" "$out/cmd"

# The paths the code names, from the source, %d/%s turned into globs.
mapfile -t named < <(grep -rhoE '"/(sys|proc)/[A-Za-z0-9_./%@:+-]+' \
    "$here"/cinder-home/src "$here"/cinder-audio/src "$here"/cinder-aac/src \
    "$here"/player/cinder-ffi/src "$here"/player/cinder-ui/src 2>/dev/null \
    | tr -d '"' | sed 's/%[ds]/*/g; s:/$::' | sort -u)

# Whole directories worth having beyond the named files: one level of files each.
dirs=(
    /sys/class/power_supply/*/ /sys/class/android_usb/android0/ /sys/class/switch/*/
    /sys/class/thermal/thermal_zone*/ /sys/devices/system/cpu/cpu*/cpufreq/ /sys/power/
    /sys/devices/platform/mt_usb/ /sys/devices/platform/mt_usb/musb-hdrc.0/
    /sys/devices/platform/icx_pm_helper/ /sys/class/leds/lcd-backlight/ /sys/class/backlight/*/
    /sys/module/mt_sleep/parameters/ /sys/module/icx_pm_helper/parameters/
)
procs=(/proc/cpuinfo /proc/meminfo /proc/version /proc/mounts /proc/uptime /proc/loadavg
       /proc/partitions /proc/cmdline /proc/modules /proc/interrupts /proc/stat)

# Runs ON the player. Prints "path<TAB>base64" per readable file, "SKIP<TAB>path<TAB>why" per refusal.
device_script='
BB=/xbin/busybox; [ -x $BB ] || BB=/system/xbin/busybox
bad() {
    case "$1" in
        /proc/regmon*) echo "a read needs a write to target first";;
        /proc/icx_audio*) echo "a partial read wedges the codec";;
        /sys/power/wait_for_fb*) echo "blocks; the app'"'"'s suspend handshake";;
        */duty|/dev/*|/sys/kernel/debug*) echo "not data, or harmful";;
        /proc/self*|/proc/\**) echo "per-process";;
        *) return 1;;
    esac
}
one() {
    [ -f "$1" ] || return 0
    if why=$(bad "$1"); then printf "SKIP\t%s\t%s\n" "$1" "$why"; return 0; fi
    v=$($BB timeout -t 2 $BB head -c 65536 "$1" 2>/dev/null | $BB base64 | $BB tr -d "\n") || true
    printf "%s\t%s\n" "$1" "$v"
}
while read -r p; do
    case "$p" in
        */) for f in $p*; do one "$f"; done;;
        *) for f in $p; do
               if [ -d "$f" ]; then for g in "$f"/*; do one "$g"; done; else one "$f"; fi
           done;;
    esac
done < /data/local/tmp/cinder_map.paths
'
# The script and its list go to /data/local/tmp (the installer's scratch area) and are removed at
# the end: this adbd has no exec-out stdin, and a command line this long is refused.
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
printf '%s\n' "$device_script" > "$tmp/cinder_map.sh"
printf '%s\n' "${named[@]}" "${dirs[@]}" "${procs[@]}" > "$tmp/cinder_map.paths"
adb push "$tmp/cinder_map.sh" "$tmp/cinder_map.paths" /data/local/tmp/ >/dev/null
echo "map_player: reading $(( ${#named[@]} + ${#dirs[@]} + ${#procs[@]} )) paths…"
n=0; s=0
: > "$out/skipped.txt"
while IFS=$'\t' read -r a b c; do
    if [ "$a" = SKIP ]; then printf '%s\t%s\n' "$b" "$c" >> "$out/skipped.txt"; s=$((s + 1)); continue; fi
    [ -n "$a" ] || continue
    mkdir -p "$out/fs$(dirname "$a")"
    printf '%s' "$b" | base64 -d > "$out/fs$a" 2>/dev/null || true
    n=$((n + 1))
done < <(adb shell sh /data/local/tmp/cinder_map.sh </dev/null | tr -d '\r')
adb shell rm /data/local/tmp/cinder_map.sh /data/local/tmp/cinder_map.paths </dev/null >/dev/null 2>&1 || true

# Cinder's own state and the library, so the virtual player boots into this player's UI: the
# settings, the resume queue, the library database (read only; Sony's scanner owns it) and the
# album-cover cache. Personal data: keep OUT_DIR out of git.
for f in /contents/cinder_settings.conf /db/MTPDB.dat /data/cinder/queue.conf \
         /data/cinder/resume.conf /data/cinder/artcache; do
    mkdir -p "$out/fs$(dirname "$f")"
    adb pull "$f" "$out/fs$(dirname "$f")/" >/dev/null 2>&1 || echo "  (not on this player: $f)"
done

# Command output, and listings of what is installed (names and sizes, no contents). Each command
# is ONE string: adb joins its arguments with spaces, so `sh -c 'a; b'` passed as words arrives as
# `sh -c a` and a bare `cat` then waits on stdin for good. stdin is /dev/null for the same reason.
cmd() { adb shell "$2" </dev/null 2>/dev/null | tr -d '\r' > "$out/cmd/$1.txt" || true; }
cmd getprop        'getprop'
cmd ps             '/xbin/busybox ps'
cmd dmesg          'dmesg'
cmd df             '/xbin/busybox df'
cmd lsmod          'cat /proc/modules'
cmd asound         'cat /proc/asound/cards; for f in /proc/asound/card*/pcm*/info; do echo "== $f"; cat $f; done'
cmd ls_sony_lib    'ls -l /system/vendor/sony/lib'
cmd ls_system_lib  'ls -l /system/lib'
cmd ls_cinder      '/xbin/busybox ls -lR /system/vendor/cinder'
cmd ls_data_cinder 'ls -l /data/cinder'
cmd ls_contents    'ls -l /contents'
cmd init_rc        'for f in /*.rc; do echo "== $f"; cat $f; done'
cmd asound_conf    'cat /etc/asound.conf'

echo "map_player: $n files, $s skipped -> $out (fs/ is the device tree, cmd/ the listings)"
