#!/usr/bin/env bash
# Sandbox test for install_cinderhome.sh's MOUNT LIFECYCLE.
#
# The bug this pins down, observed on a live player 2026-09-21: the script is written for the Sony
# updater, where /system and /data are its own to mount and to take away again. Run on a LIVE
# system it is a guest — both are already mounted — and the tail's blind `umount /data` SUCCEEDED,
# leaving a RUNNING player with no /data until `/system/bin/mount_partition usrdata` put it back.
#
# Two things are checked, because they are two different failure modes of the same assumption:
#   1. cleanup_mounts() unmounts only what WE mounted, and leaves a pre-existing mount alone;
#   2. DATA_MOUNTED is true on a live system. The sentinel test cannot see that on its own — `touch`
#      succeeds on the real /data, so SENTINEL=1, and whether the sentinel is then VISIBLE depends
#      on whether our own mount stacked over and shadowed it. That is why the same script said
#      "not mounted" in one live session (so the cable pass was silently skipped) and "mounted" in
#      the next.
#
# Same shape as test_cable_pass.sh: the block under test is mirrored verbatim and run inside
# `unshare -rm` so the stub mount can do REAL bind mounts.
set -u
SP="$(mktemp -d /tmp/cinder_mounts_test.XXXXXX)"
trap 'rm -rf "$SP"' EXIT
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then printf '  ok    %-52s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-52s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

cat > "$SP/scenario.sh" <<'SCENARIO'
#!/bin/bash
# usage: scenario.sh <sandbox> <live:1|0> [stray:1|0]
# live=1 models a running player: /data and /system are ALREADY mounted (and /system is ro)
# before the installer starts. live=0 models the updater: neither is mounted and /proc is absent.
set -u
R="$1"; LIVE="$2"; STRAY="${3:-0}"
mkdir -p "$R/proc" "$R/data" "$R/system" "$R/realdata" "$R/realsystem" "$R/bin"
touch "$R/realdata/.is_the_partition" "$R/realsystem/.is_the_partition"
# stray=1: a 0-byte sentinel left ON THE PARTITION by an older build's live install (found on the
# A55, 2026-09-23). The next updater install used to read it as "our marker is visible".
[ "$STRAY" = 1 ] && touch "$R/realdata/.cinder_premount"
REAL_MOUNT="$(command -v mount)"; REAL_UMOUNT="$(command -v umount)"

# Stub mount/umount. Binds for real so a mount is genuinely observable, records remount,ro calls
# (which a bind sandbox cannot actually honour), and keeps $R/proc/mounts in step.
cat > "$R/bin/mount" <<EOF
#!/bin/sh
echo "\$*" >> '$R/mountcalls'
last=""; for a in "\$@"; do last="\$a"; done
case " \$* " in
  *remount,ro*) echo "remount-ro \$last" >> '$R/remounts'; exit 0 ;;
  *remount,rw*) exit 0 ;;
esac
case " \$* " in
  *usrdata*|*mmcblk0p28*) src='$R/realdata'; dev=/emmc@usrdata ;;
  *android*)              src='$R/realsystem'; dev=/emmc@android ;;
  *) exit 0 ;;
esac
'$REAL_MOUNT' --bind "\$src" "\$last" 2>/dev/null \
  && echo "\$dev \$last ext4 rw 0 0" >> '$R/proc/mounts' 2>/dev/null
exit 0
EOF
cat > "$R/bin/umount" <<EOF
#!/bin/sh
'$REAL_UMOUNT' "\$1" 2>/dev/null && {
  '$REAL_MOUNT' 2>/dev/null >/dev/null
  grep -v " \$1 " '$R/proc/mounts' > '$R/proc/mounts.new' 2>/dev/null
  mv '$R/proc/mounts.new' '$R/proc/mounts' 2>/dev/null
}
exit 0
EOF
cat > "$R/bin/busybox" <<'EOF'
#!/bin/sh
cmd="$1"; shift
case "$cmd" in
  grep) exec /usr/bin/grep "$@";; cat) exec /usr/bin/cat "$@";;
  mkdir) exec /usr/bin/mkdir "$@";; chmod) exec /usr/bin/chmod "$@";;
  touch) exec /usr/bin/touch "$@";; rm) exec /usr/bin/rm "$@";;
  awk) exec /usr/bin/awk "$@";; *) exit 1;;
esac
EOF
chmod +x "$R/bin/mount" "$R/bin/umount" "$R/bin/busybox"
PATH="$R/bin:$PATH"
BB="$R/bin/busybox"
data_dir="$R/data"; sys_dir="$R/system"; mounts_file="$R/proc/mounts"

if [ "$LIVE" = 1 ]; then
    # a running player: both already mounted before the installer is invoked, /system ro
    printf 'rootfs / rootfs rw 0 0\n' > "$mounts_file"
    "$REAL_MOUNT" --bind "$R/realdata" "$data_dir" 2>/dev/null
    "$REAL_MOUNT" --bind "$R/realsystem" "$sys_dir" 2>/dev/null
    printf '/emmc@usrdata %s ext4 rw,nodev,noexec,noatime 0 0\n' "$data_dir" >> "$mounts_file"
    printf '/emmc@android %s ext4 ro,noatime,nodiratime,data=ordered 0 0\n' "$sys_dir" >> "$mounts_file"
fi
# LIVE=0 is the updater: no /proc/mounts at all, nothing mounted.

# ── block under test: verbatim semantics from install_cinderhome.sh ──────────────────────────
SYSTEM_PREMOUNTED=0
SYSTEM_PREMOUNT_RO=0
DATA_PREMOUNTED=0
if "$BB" grep -q " $sys_dir " "$mounts_file" 2>/dev/null; then
    SYSTEM_PREMOUNTED=1
    case ",$("$BB" awk -v d="$sys_dir" '$2 == d { print $4; exit }' "$mounts_file" 2>/dev/null)," in
        *,ro,*) SYSTEM_PREMOUNT_RO=1 ;;
    esac
fi
"$BB" grep -q " $data_dir " "$mounts_file" 2>/dev/null && DATA_PREMOUNTED=1

cleanup_mounts() {
    if [ "$DATA_PREMOUNTED" = 1 ]; then :; else umount "$data_dir" 2>/dev/null; fi
    if [ "$SYSTEM_PREMOUNTED" = 1 ]; then
        [ "$SYSTEM_PREMOUNT_RO" = 1 ] && mount -o remount,ro "$sys_dir" 2>/dev/null
    else
        umount "$sys_dir" 2>/dev/null
    fi
    true
}

mount -t ext4 -o rw /emmc@android "$sys_dir" 2>/dev/null
mount -o remount,rw /emmc@android "$sys_dir" 2>/dev/null

SENTINEL=0
SENTINEL_TOKEN="cinder-premount $$"
if [ "$DATA_PREMOUNTED" != 1 ]; then
    echo "$SENTINEL_TOKEN" > "$data_dir/.cinder_premount" 2>/dev/null \
        && [ "$("$BB" cat "$data_dir/.cinder_premount" 2>/dev/null)" = "$SENTINEL_TOKEN" ] && SENTINEL=1
fi
data_is_mounted() {
    if [ "$SENTINEL" = 1 ]; then
        [ "$("$BB" cat "$data_dir/.cinder_premount" 2>/dev/null)" = "$SENTINEL_TOKEN" ] && return 1
        return 0
    fi
    "$BB" grep -q " $data_dir " "$mounts_file" 2>/dev/null
}
if [ "$DATA_PREMOUNTED" = 1 ]; then
    :   # already mounted by the running system — its options are not ours to change
else
    mount -t ext4 -o rw /emmc@usrdata "$data_dir" 2>/dev/null
    mount -o remount,rw /emmc@usrdata "$data_dir" 2>/dev/null
    data_is_mounted || mount -t ext4 -o rw /dev/block/mmcblk0p28 "$data_dir" 2>/dev/null
fi
DATA_MOUNTED=0
if [ "$DATA_PREMOUNTED" = 1 ]; then
    DATA_MOUNTED=1
else
    data_is_mounted && DATA_MOUNTED=1
fi
"$BB" rm -f "$data_dir/.cinder_premount" 2>/dev/null
# ── end block under test ─────────────────────────────────────────────────────────────────────

echo "DATA_MOUNTED=$DATA_MOUNTED"
echo "DATA_PREMOUNTED=$DATA_PREMOUNTED"
echo "SYSTEM_PREMOUNT_RO=$SYSTEM_PREMOUNT_RO"
# Nothing may be left on the partition, whether we mounted it or found it mounted.
[ -e "$R/realdata/.cinder_premount" ] && echo "sentinel_on_partition=yes" || echo "sentinel_on_partition=no"
cleanup_mounts
# Did /data survive cleanup? The partition marker is only reachable through a live bind.
[ -e "$data_dir/.is_the_partition" ] && echo "data_still_mounted=yes" || echo "data_still_mounted=no"
[ -e "$sys_dir/.is_the_partition" ] && echo "system_still_mounted=yes" || echo "system_still_mounted=no"
grep -q "remount-ro $sys_dir" "$R/remounts" 2>/dev/null && echo "system_ro_restored=yes" || echo "system_ro_restored=no"
# A live player's /data carries nodev,noexec,noatime. Any mount or remount of it by this script
# replaces those with the kernel's defaults, so on a live system there must be NEITHER.
grep -q "$data_dir" "$R/mountcalls" 2>/dev/null && echo "data_touched=yes" || echo "data_touched=no"
SCENARIO
chmod +x "$SP/scenario.sh"

if ! unshare -rm true 2>/dev/null; then
  echo "skip: user+mount namespaces unavailable here — the mount-lifecycle cases cannot run"
  echo "0 passed, 0 failed (skipped)"
  exit 0
fi

run() { local R; R="$(mktemp -d "$SP/run.XXXXXX")"; unshare -rm bash "$SP/scenario.sh" "$R" "$1" "${2:-0}"; }
field() { echo "$1" | sed -n "s/^$2=//p"; }

echo "── 1. LIVE system: we are a guest, and must leave the mounts as we found them ──"
o="$(run 1)"
check "DATA_PREMOUNTED seen"        "$(field "$o" DATA_PREMOUNTED)"    "1"
check "DATA_MOUNTED (no false-neg)" "$(field "$o" DATA_MOUNTED)"       "1"
check "/data NOT unmounted"         "$(field "$o" data_still_mounted)" "yes"
check "/system NOT unmounted"       "$(field "$o" system_still_mounted)" "yes"
check "/system put back to ro"      "$(field "$o" system_ro_restored)" "yes"
check "/data mount options untouched" "$(field "$o" data_touched)"     "no"
check "no sentinel left on /data"   "$(field "$o" sentinel_on_partition)" "no"

echo "── 2. UPDATER: we mounted them, so we take them away again ──"
o="$(run 0)"
check "DATA_PREMOUNTED clear"       "$(field "$o" DATA_PREMOUNTED)"    "0"
check "DATA_MOUNTED via sentinel"   "$(field "$o" DATA_MOUNTED)"       "1"
check "/data unmounted by cleanup"  "$(field "$o" data_still_mounted)" "no"
check "/system unmounted by cleanup" "$(field "$o" system_still_mounted)" "no"
check "no stray ro remount"         "$(field "$o" system_ro_restored)" "no"
check "updater DOES mount /data"    "$(field "$o" data_touched)"       "yes"
check "no sentinel left on /data"   "$(field "$o" sentinel_on_partition)" "no"

echo "── 3. UPDATER after an older LIVE install left a 0-byte sentinel on the partition ──"
o="$(run 0 1)"
check "stray is not our marker"     "$(field "$o" DATA_MOUNTED)"       "1"
check "stray cleaned up"            "$(field "$o" sentinel_on_partition)" "no"

echo "── 4. LIVE system with the same stray ──"
o="$(run 1 1)"
check "DATA_MOUNTED"                "$(field "$o" DATA_MOUNTED)"       "1"
check "stray cleaned up"            "$(field "$o" sentinel_on_partition)" "no"

echo
printf '%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ]
