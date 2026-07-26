#!/usr/bin/env bash
# Functional test for the generated cinderhome-launch.sh. Extracts the launcher out of the
# installer heredoc, rewrites its absolute paths into a sandbox, and drives each recovery path.
set -u
SP="${TMPDIR:-/tmp}/cinder_launcher_test.$$"; mkdir -p "$SP"; trap 'rm -rf "$SP"' EXIT
SRC="$(cd "$(dirname "$0")/.." && pwd)/deploy/install_cinderhome.sh"
PASS=0; FAIL=0

build_launcher() {   # $1 = sandbox root
  local R="$1"
  awk "/<<'LAUNCH_EOF'/{f=1;next} /^LAUNCH_EOF\$/{f=0} f" "$SRC" \
  | sed -e "s#/data/cinder#$R/data/cinder#g" \
        -e "s#/contents#$R/contents#g" \
        -e "s#/proc/mounts#$R/proc/mounts#g" \
        -e "s#/sys/class#$R/sys/class#g" \
        -e "s#/system/vendor/sony/bin/HgrmMediaPlayerApp#$R/stock#g" \
        -e "s#/system/vendor/unknown321/bin/cinder-home#$R/cinder#g" \
        -e "s#/system/vendor/unknown321/bin/ldac-run.sh#$R/noldac#g" \
        -e "s#^sleep 3\$#sleep 0#" -e "s#    sleep 3\$#    sleep 0#" \
        -e "s#^sync\$#true#" -e "s#; sync#; true#g" \
  > "$R/launch.sh"
  chmod +x "$R/launch.sh"
}

# $1 name, $2 expected (cinder|stock), rest: setup commands run with $R set
scenario() {
  local name="$1" want="$2"; shift 2
  local R; R="$(mktemp -d "$SP/lt.XXXXXX")"
  mkdir -p "$R/data/cinder" "$R/contents" "$R/proc" "$R/sys/class/android_usb/android0" \
           "$R/sys/class/power_supply/usb"
  printf '#!/bin/sh\necho STOCK\n'  > "$R/stock";  chmod +x "$R/stock"
  printf '#!/bin/sh\necho CINDER\n' > "$R/cinder"; chmod +x "$R/cinder"
  printf 'rootfs / rootfs rw 0 0\n/emmc@contents %s/contents vfat rw 0 0\n' "$R" > "$R/proc/mounts"
  echo DISCONNECTED > "$R/sys/class/android_usb/android0/state"
  echo 0            > "$R/sys/class/power_supply/usb/online"
  ( export R; eval "$*" )          # scenario-specific setup
  build_launcher "$R"
  # cinder's stdout is redirected into the log by design, so look there too.
  local got; got="$(sh "$R/launch.sh" 2>/dev/null | tail -1)"
  [ -n "$got" ] || got="$(cat "$R/contents/cinderhome.log" "$R/data/cinder/cinderhome.log" 2>/dev/null | tail -1)"
  case "$got" in
    CINDER) got=cinder;; STOCK) got=stock;; *) got="none($got)";;
  esac
  if [ "$got" = "$want" ]; then
    printf '  ok    %-46s -> %s\n' "$name" "$got"; PASS=$((PASS+1))
  else
    printf '  FAIL  %-46s -> %s (want %s)\n' "$name" "$got" "$want"; FAIL=$((FAIL+1))
  fi
  # export the sandbox for post-checks
  LAST_R="$R"
}

echo "launcher recovery matrix:"

scenario "healthy boot, no cable"                 cinder ':'
scenario "cable at boot (the fs-free escape)"     stock  'echo CONFIGURED > $R/sys/class/android_usb/android0/state'
scenario "cable + /data opt-out"                  cinder 'echo CONFIGURED > $R/sys/class/android_usb/android0/state; : > $R/data/cinder/cable_escape_off'
scenario "cable + /contents opt-out"              cinder 'echo CONFIGURED > $R/sys/class/android_usb/android0/state; : > $R/contents/cinderhome_cable_off'
scenario "power_supply online only"               stock  'echo 1 > $R/sys/class/power_supply/usb/online'
scenario "/contents NOT mounted (the brick)"      stock  'printf "rootfs / rootfs rw 0 0\n" > $R/proc/mounts'
scenario "/data unwritable -> no safety net"      stock  'chmod 555 $R/data/cinder'
scenario "counter 2 of MAXBAD 4 -> still tries"    cinder 'echo 2 > $R/data/cinder/bootcount'
scenario "counter 4 hits MAXBAD -> latch"         stock  'echo 3 > $R/data/cinder/bootcount; touch -d "2020-01-01" $R/cinder 2>/dev/null'
scenario "already latched (off set)"              stock  ': > $R/data/cinder/off; touch -d "2020-01-01" $R/cinder'
scenario "MSC escape cinderhome_off"              stock  ': > $R/contents/cinderhome_off'
scenario "MSC cinderhome_clear un-latches"        cinder ': > $R/data/cinder/off; : > $R/data/cinder/DISABLED_badboot; echo 9 > $R/data/cinder/bootcount; : > $R/contents/cinderhome_clear; touch -d "2020-01-01" $R/cinder'
scenario "self-heal: binary newer than latch"     cinder ': > $R/data/cinder/DISABLED_badboot; : > $R/data/cinder/off; touch -d "2020-01-01" $R/data/cinder/DISABLED_badboot $R/data/cinder/off'
scenario "binary missing"                         stock  'rm -f $R/cinder'
scenario "garbage counter treated as 0"           cinder 'printf "\x00\xff junk" > $R/data/cinder/bootcount'

# the log-redirect trap that caused the brick: unwritable log dir must NOT stop the exec
scenario "log path unwritable -> still execs"     cinder 'chmod 555 $R/contents'

echo
echo "post-checks:"
R="$LAST_R"
scenario "counter persists across a failed boot"  cinder 'echo 1 > $R/data/cinder/bootcount'
n=$(cat "$LAST_R/data/cinder/bootcount" 2>/dev/null)
if [ "$n" = "2" ]; then printf '  ok    %-46s -> %s\n' "counter incremented 1 -> 2" "$n"; PASS=$((PASS+1))
else printf '  FAIL  %-46s -> %s (want 2)\n' "counter incremented 1 -> 2" "$n"; FAIL=$((FAIL+1)); fi

rm -rf "$SP"/lt.*
echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
