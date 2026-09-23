#!/usr/bin/env bash
# Sandbox test for the issue #14 fix in install_cinderhome.sh:
#   1. the /data mount block sets DATA_MOUNTED correctly (from the mounts table)
#   2. the cable-pass write verifies by read-back when /data is mounted,
#      and the file lands on the PARTITION, not the mountpoint's old (ramdisk) dir
#   3. the pass write WARNs — and does not lie — when /data is not mountable
#   4. the pass write WARNs when the write "succeeds" but reads back wrong
#   5. the ramdisk sentinel decides it when /proc is not mounted in the updater
#   6. the raw-partition retry (mmcblk0p28) covers a /dev with no /emmc@* aliases
#
# Runs the block under test inside `unshare -rm` (user+mount namespace) so the
# stub mount can do REAL bind mounts: the "partition" and the mountpoint are
# genuinely two different directories until the mount succeeds, exactly like
# the device's ramdisk /data and /emmc@usrdata.
#
# The inner script is generated to a file (not passed through `declare -f`),
# because a function round-tripped through `bash -c` re-quotes its heredocs and
# silently eats the stub's expansions.
set -u
SP="$(mktemp -d /tmp/cinder_pass_test.XXXXXX)"
trap 'rm -rf "$SP"' EXIT
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then printf '  ok    %-52s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-52s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

# ── the inner scenario runner (runs INSIDE the namespace) ───────────────────────────────────
# usage: scenario.sh <sandbox> <mountable:alias|p28|none> <sabotage_readback:1|0> <proc:1|0>
cat > "$SP/scenario.sh" <<'SCENARIO'
#!/bin/bash
set -u
R="$1"; MOUNTABLE="$2"; SABOTAGE="$3"; PROC_OK="${4:-1}"
mkdir -p "$R/proc" "$R/ram_data" "$R/realpart/cinder" "$R/bin"
# PROC_OK=0 is the updater that mounts no /proc: every read of the mounts table fails, and
# only the sentinel can tell a real mount from a ramdisk write.
[ "$PROC_OK" = 1 ] \
  && printf 'rootfs / rootfs rw 0 0\n/emmc@contents %s/contents vfat rw 0 0\n' "$R" > "$R/proc/mounts"

# the "mount" binary for the sandbox: bind the "partition" over the mountpoint on
# success and append the mounts line the real /proc/mounts would then show. Uses the
# system mount's absolute path so it cannot resolve to itself once $R/bin is on PATH.
REAL_MOUNT="$(command -v mount)"
# which spelling of the partition this sandbox answers to: the /emmc@usrdata alias, the raw
# node the retry uses, or neither. A bind that has already happened is left alone, so a second
# call cannot stack a mount the way the real one would.
case "$MOUNTABLE" in alias) WORKS="/emmc@usrdata";; p28) WORKS="/dev/block/mmcblk0p28";; *) WORKS="__none__";; esac
cat > "$R/bin/mount" <<EOF
#!/bin/sh
last=""
for a in "\$@"; do last="\$a"; done
case " \$* " in
  *"$WORKS"*)
    [ -e "\$last/.mounted" ] && exit 0
    '$REAL_MOUNT' --bind '$R/realpart' "\$last" 2>/dev/null \
      && echo "$WORKS \$last ext4 rw 0 0" >> '$R/proc/mounts' 2>/dev/null
    ;;
esac
exit 0
EOF
chmod +x "$R/bin/mount"
touch "$R/realpart/.mounted"

# host tools stand in for busybox ($BB in the real script is the busybox BINARY,
# and every call is "$BB" <applet> — so the sandbox needs a dispatcher, not a dir)
cat > "$R/bin/busybox" <<'EOF'
#!/bin/sh
cmd="$1"; shift
case "$cmd" in
  grep) exec /usr/bin/grep "$@";;
  cat)  exec /usr/bin/cat "$@";;
  mkdir) exec /usr/bin/mkdir "$@";;
  chmod) exec /usr/bin/chmod "$@";;
  touch) exec /usr/bin/touch "$@";;
  rm)   exec /usr/bin/rm "$@";;
  *) exit 1;;
esac
EOF
chmod +x "$R/bin/busybox"

data_dir="$R/ram_data"; mounts_file="$R/proc/mounts"
BB="$R/bin/busybox"
PATH="$R/bin:$PATH"

# ── the block under test: verbatim semantics from install_cinderhome.sh ──
[ -d "$data_dir" ] || "$BB" mkdir -p "$data_dir" 2>/dev/null
SENTINEL=0
SENTINEL_TOKEN="cinder-premount $$"
echo "$SENTINEL_TOKEN" > "$data_dir/.cinder_premount" 2>/dev/null \
    && [ "$("$BB" cat "$data_dir/.cinder_premount" 2>/dev/null)" = "$SENTINEL_TOKEN" ] && SENTINEL=1
data_is_mounted() {
    if [ "$SENTINEL" = 1 ]; then
        [ "$("$BB" cat "$data_dir/.cinder_premount" 2>/dev/null)" = "$SENTINEL_TOKEN" ] && return 1
        return 0
    fi
    "$BB" grep -q " $data_dir " "$mounts_file" 2>/dev/null
}
mount -t ext4 -o rw /emmc@usrdata "$data_dir" 2>/dev/null
mount -o remount,rw /emmc@usrdata "$data_dir" 2>/dev/null
data_is_mounted || mount -t ext4 -o rw /dev/block/mmcblk0p28 "$data_dir" 2>/dev/null
DATA_MOUNTED=0
data_is_mounted && DATA_MOUNTED=1
"$BB" rm -f "$data_dir/.cinder_premount" 2>/dev/null
[ "$DATA_MOUNTED" = 1 ] && echo "state: /data (/emmc@usrdata) mounted" \
                        || echo "WARN: /emmc@usrdata could not be mounted at /data"

# the pass write, with an optional sabotage: the write "succeeds" but the
# read-back is wrong (the shape of the pre-fix ramdisk lie)
"$BB" mkdir -p "$data_dir/cinder" 2>/dev/null
if [ "$SABOTAGE" = 1 ]; then
    echo 1 > "$data_dir/cinder/cable_pass_once" 2>/dev/null
    rm -f "$data_dir/cinder/cable_pass_once" 2>/dev/null   # read back below finds nothing
else
    echo 1 > "$data_dir/cinder/cable_pass_once" 2>/dev/null
fi
if [ "$DATA_MOUNTED" = 1 ] \
   && [ "$("$BB" cat "$data_dir/cinder/cable_pass_once" 2>/dev/null)" = "1" ]; then
    "$BB" chmod 644 "$data_dir/cinder/cable_pass_once" 2>/dev/null
    echo "cable pass: the next boot starts Cinder with the cable in"
else
    echo "WARN: the cable pass could not be written"
fi
echo "DATA_MOUNTED=$DATA_MOUNTED"
[ -e "$R/realpart/cinder/cable_pass_once" ] && echo "on_partition=yes" || echo "on_partition=no"
SCENARIO
chmod +x "$SP/scenario.sh"

# The whole harness needs a user+mount namespace for the stub mount's real bind mounts.
# Where the runtime forbids them (some containers, some macOS setups) there is nothing to
# test — say so and pass, rather than fail ten cases about the environment, not the script.
if ! unshare -rm true 2>/dev/null; then
  echo "skip: user+mount namespaces unavailable here — the cable-pass cases cannot run"
  echo "0 passed, 0 failed (skipped)"
  exit 0
fi

run() {  # $1 = mountable alias|p28|none, $2 = sabotage 1|0, $3 = /proc mounted 1|0
  local R; R="$(mktemp -d "$SP/run.XXXXXX")"
  unshare -rm bash "$SP/scenario.sh" "$R" "$1" "$2" "${3:-1}"
}

echo "── 1. /data mountable (the fixed case) ──"
o="$(run alias 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "1"
check "mount line logged"     "$(echo "$o" | sed -n '1p')"                  "state: /data (/emmc@usrdata) mounted"
check "pass write succeeds"   "$(echo "$o" | sed -n '2p')"                  "cable pass: the next boot starts Cinder with the cable in"
check "pass on the partition" "$(echo "$o" | sed -n 's/^on_partition=//p')" "yes"

echo "── 2. /data NOT mountable (the pre-fix case, now loud) ──"
o="$(run none 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "0"
check "mount WARN logged"     "$(echo "$o" | sed -n '1p')"                  "WARN: /emmc@usrdata could not be mounted at /data"
check "pass write refused"    "$(echo "$o" | sed -n '2p')"                  "WARN: the cable pass could not be written"
check "nothing on partition"  "$(echo "$o" | sed -n 's/^on_partition=//p')" "no"

echo "── 3. write 'succeeds' but read-back fails (the pre-fix lie) ──"
o="$(run alias 1)"
check "pass write refused"    "$(echo "$o" | sed -n '2p')"                  "WARN: the cable pass could not be written"
check "nothing on partition"  "$(echo "$o" | sed -n 's/^on_partition=//p')" "no"

echo "── 4. mounted, but the updater has no /proc (the sentinel decides) ──"
o="$(run alias 0 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "1"
check "pass write succeeds"   "$(echo "$o" | sed -n '2p')"                  "cable pass: the next boot starts Cinder with the cable in"
check "pass on the partition" "$(echo "$o" | sed -n 's/^on_partition=//p')" "yes"

echo "── 5. no /emmc@* aliases — the raw-partition retry carries it ──"
o="$(run p28 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "1"
check "pass write succeeds"   "$(echo "$o" | sed -n '2p')"                  "cable pass: the next boot starts Cinder with the cable in"
check "pass on the partition" "$(echo "$o" | sed -n 's/^on_partition=//p')" "yes"

echo "── 6. nothing mountable AND no /proc (the sentinel says no, loudly) ──"
o="$(run none 0 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "0"
check "pass write refused"    "$(echo "$o" | sed -n '2p')"                  "WARN: the cable pass could not be written"
check "nothing on partition"  "$(echo "$o" | sed -n 's/^on_partition=//p')" "no"

echo
printf '%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ]
