#!/usr/bin/env bash
# Sandbox test for the issue #14 fix in install_cinderhome.sh:
#   1. the /data mount block sets DATA_MOUNTED correctly (from the mounts table)
#   2. the cable-pass write verifies by read-back when /data is mounted,
#      and the file lands on the PARTITION, not the mountpoint's old (ramdisk) dir
#   3. the pass write WARNs — and does not lie — when /data is not mountable
#   4. the pass write WARNs when the write "succeeds" but reads back wrong
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
# usage: scenario.sh <sandbox> <mount_succeeds:1|0> <sabotage_readback:1|0>
cat > "$SP/scenario.sh" <<'SCENARIO'
#!/bin/bash
set -u
R="$1"; MOUNT_OK="$2"; SABOTAGE="$3"
mkdir -p "$R/proc" "$R/ram_data" "$R/realpart/cinder" "$R/bin"
printf 'rootfs / rootfs rw 0 0\n/emmc@contents %s/contents vfat rw 0 0\n' "$R" > "$R/proc/mounts"

# the "mount" binary for the sandbox: bind the "partition" over the mountpoint on
# success and append the mounts line the real /proc/mounts would then show. Uses the
# system mount's absolute path so it cannot resolve to itself once $R/bin is on PATH.
REAL_MOUNT="$(command -v mount)"
cat > "$R/bin/mount" <<EOF
#!/bin/sh
last=""
for a in "\$@"; do last="\$a"; done
case " \$* " in
  *"/emmc@usrdata"*)
EOF
if [ "$MOUNT_OK" = 1 ]; then
  printf "    '%s' --bind '%s/realpart' \"\$last\" 2>/dev/null && echo \"/emmc@usrdata \$last ext4 rw 0 0\" >> '%s/proc/mounts'\n" \
    "$REAL_MOUNT" "$R" "$R" >> "$R/bin/mount"
fi
cat >> "$R/bin/mount" <<'EOF'
    ;;
esac
exit 0
EOF
chmod +x "$R/bin/mount"

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
  *) exit 1;;
esac
EOF
chmod +x "$R/bin/busybox"

data_dir="$R/ram_data"; mounts_file="$R/proc/mounts"
BB="$R/bin/busybox"
PATH="$R/bin:$PATH"

# ── the block under test: verbatim semantics from install_cinderhome.sh ──
[ -d "$data_dir" ] || "$BB" mkdir -p "$data_dir" 2>/dev/null
mount -t ext4 -o rw /emmc@usrdata "$data_dir" 2>/dev/null
mount -o remount,rw /emmc@usrdata "$data_dir" 2>/dev/null
DATA_MOUNTED=0
"$BB" grep -q " $data_dir " "$mounts_file" 2>/dev/null && DATA_MOUNTED=1
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

run() {  # $1 = mountable 1|0, $2 = sabotage 1|0
  local R; R="$(mktemp -d "$SP/run.XXXXXX")"
  unshare -rm bash "$SP/scenario.sh" "$R" "$1" "$2"
}

echo "── 1. /data mountable (the fixed case) ──"
o="$(run 1 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "1"
check "mount line logged"     "$(echo "$o" | sed -n '1p')"                  "state: /data (/emmc@usrdata) mounted"
check "pass write succeeds"   "$(echo "$o" | sed -n '2p')"                  "cable pass: the next boot starts Cinder with the cable in"
check "pass on the partition" "$(echo "$o" | sed -n 's/^on_partition=//p')" "yes"

echo "── 2. /data NOT mountable (the pre-fix case, now loud) ──"
o="$(run 0 0)"
check "DATA_MOUNTED"          "$(echo "$o" | sed -n 's/^DATA_MOUNTED=//p')" "0"
check "mount WARN logged"     "$(echo "$o" | sed -n '1p')"                  "WARN: /emmc@usrdata could not be mounted at /data"
check "pass write refused"    "$(echo "$o" | sed -n '2p')"                  "WARN: the cable pass could not be written"
check "nothing on partition"  "$(echo "$o" | sed -n 's/^on_partition=//p')" "no"

echo "── 3. write 'succeeds' but read-back fails (the pre-fix lie) ──"
o="$(run 1 1)"
check "pass write refused"    "$(echo "$o" | sed -n '2p')"                  "WARN: the cable pass could not be written"
check "nothing on partition"  "$(echo "$o" | sed -n 's/^on_partition=//p')" "no"

echo
printf '%s passed, %s failed\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ]
