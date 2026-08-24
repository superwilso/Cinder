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
        -e "s#^    sleep 1\$#    sleep 0#" \
        -e "s#^sync\$#true#" -e "s#; sync#; true#g" \
        -e "s#^RESPAWN_HEALTHY_S=30#RESPAWN_HEALTHY_S=2#" \
        -e "s#^RESPAWN_MAX_TOTAL=10#RESPAWN_MAX_TOTAL=4#" \
  > "$R/launch.sh"
  chmod +x "$R/launch.sh"
}

# ── crash-supervisor stubs ────────────────────────────────────────────────────────────────────
# Each records its own invocation count in $R/runs so a scenario can assert how many times the
# launcher actually started cinder-home — the whole point of the supervisor is invisible otherwise.
stub_head() { printf '#!/bin/sh\necho CINDER\nn=$(cat "%s/runs" 2>/dev/null)\ncase "$n" in ""|*[!0-9]*) n=0;; esac\nn=$((n+1)); echo "$n" > "%s/runs"\n' "$1" "$1"; }
# $1=R $2=exit code $3=seconds to run first
stub_always()       { { stub_head "$1"; printf 'sleep %s\nexit %s\n' "$3" "$2"; } > "$1/cinder"; chmod +x "$1/cinder"; }
# $1=R $2=exit code $3=how many of the first runs die that way (the rest run 3 s and exit 0)
stub_crash_then_ok(){ { stub_head "$1"; printf '[ "$n" -le %s ] && exit %s\nsleep 3\nexit 0\n' "$3" "$2"; } > "$1/cinder"; chmod +x "$1/cinder"; }
# dies AND removes itself — the hot-swap-lands-mid-session case
stub_vanish()       { { stub_head "$1"; printf 'rm -f "%s/cinder"\nexit 139\n' "$1"; } > "$1/cinder"; chmod +x "$1/cinder"; }
runs_of() { cat "$1/runs" 2>/dev/null || echo 0; }
check() {  # $1 label $2 got $3 want
  if [ "$2" = "$3" ]; then printf '  ok    %-46s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-46s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi
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
  # cinder's stdout is redirected into the log by design and the supervisor logs to BOTH, so
  # gather everything and look for the markers rather than trusting the last line of one stream.
  # STOCK wins when both appear: handing over to the Sony player is terminal, and with the
  # supervisor a scenario can legitimately run cinder several times before getting there.
  local all
  all="$( { sh "$R/launch.sh" 2>/dev/null
            cat "$R/contents/cinderhome.log" "$R/data/cinder/cinderhome.log" 2>/dev/null; } )"
  local got=none
  case "$all" in *CINDER*) got=cinder;; esac
  case "$all" in *STOCK*)  got=stock;;  esac
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
# THE SAFETY NET CANNOT BE ARMED. Two ways of saying it, because one of them lies when the test
# runs as root: chmod 555 does not stop uid 0, so on a root shell the counter write SUCCEEDS, the
# launcher correctly starts cinder, and the case reports a failure that is about the tester rather
# than the launcher. (On the device the launcher runs as uid 100 and 555 does block it — which is
# why the case is kept rather than replaced.) The ENOTDIR variant blocks everyone, so it is the one
# that actually holds the launcher to account here.
if [ "$(id -u)" = 0 ]; then
  printf '  skip  %-46s -> chmod does not bind uid 0\n' "/data unwritable (as root)"
else
  scenario "/data unwritable -> no safety net"    stock  'chmod 555 $R/data/cinder'
fi
scenario "/data/cinder is a FILE -> no safety net" stock 'rm -rf $R/data/cinder; : > $R/data/cinder'
scenario "counter 2 of MAXBAD 4 -> still tries"    cinder 'echo 2 > $R/data/cinder/bootcount'
scenario "counter 4 hits MAXBAD -> latch"         stock  'echo 3 > $R/data/cinder/bootcount; touch -d "2020-01-01" $R/cinder 2>/dev/null'
scenario "already latched (off set)"              stock  ': > $R/data/cinder/off; touch -d "2020-01-01" $R/cinder'
scenario "MSC escape cinderhome_off"              stock  ': > $R/contents/cinderhome_off'
scenario "MSC cinderhome_clear un-latches"        cinder ': > $R/data/cinder/off; : > $R/data/cinder/DISABLED_badboot; echo 9 > $R/data/cinder/bootcount; : > $R/contents/cinderhome_clear; touch -d "2020-01-01" $R/cinder'
scenario "self-heal: binary newer than latch"     cinder ': > $R/data/cinder/DISABLED_badboot; : > $R/data/cinder/off; touch -d "2020-01-01" $R/data/cinder/DISABLED_badboot $R/data/cinder/off'
scenario "binary missing"                         stock  'rm -f $R/cinder'
scenario "garbage counter treated as 0"           cinder 'printf "\x00\xff junk" > $R/data/cinder/bootcount'
# One-shot "Boot to stock", armed from Cinder's Settings row. Fires once, from either filesystem,
# and must NOT spend a bad-boot life (it is a deliberate choice, not a failed boot).
scenario "one-shot boot-to-stock (/data)"         stock  ': > $R/data/cinder/once_stock'
scenario "one-shot boot-to-stock (/contents)"     stock  ': > $R/contents/cinderhome_once'
scenario "one-shot fires even when latched clear" stock  ': > $R/data/cinder/once_stock; echo 1 > $R/data/cinder/bootcount'

# the log-redirect trap that caused the brick: unwritable log dir must NOT stop the exec
scenario "log path unwritable -> still execs"     cinder 'chmod 555 $R/contents'

# The one-shot must be self-undoing: consumed on the boot it fires, so the NEXT boot is Cinder
# again. Without that it would be a one-way trip for anyone without a cable to undo it.
scenario "one-shot is consumed (arms once)"       stock  ': > $R/data/cinder/once_stock; echo 0 > $R/data/cinder/bootcount'
if [ ! -f "$LAST_R/data/cinder/once_stock" ]; then
    printf '  ok    %-46s -> %s\n' "one-shot flag consumed" "gone"; PASS=$((PASS+1))
else
    printf '  FAIL  %-46s -> %s\n' "one-shot flag consumed" "still present"; FAIL=$((FAIL+1))
fi
n=$(cat "$LAST_R/data/cinder/bootcount" 2>/dev/null)
if [ "$n" = "0" ]; then
    printf '  ok    %-46s -> %s\n' "one-shot costs no bad-boot life" "$n"; PASS=$((PASS+1))
else
    printf '  FAIL  %-46s -> %s (want 0)\n' "one-shot costs no bad-boot life" "$n"; FAIL=$((FAIL+1))
fi

echo
echo "post-checks:"
R="$LAST_R"
scenario "counter persists across a failed boot"  cinder 'echo 1 > $R/data/cinder/bootcount'
n=$(cat "$LAST_R/data/cinder/bootcount" 2>/dev/null)
if [ "$n" = "2" ]; then printf '  ok    %-46s -> %s\n' "counter incremented 1 -> 2" "$n"; PASS=$((PASS+1))
else printf '  FAIL  %-46s -> %s (want 2)\n' "counter incremented 1 -> 2" "$n"; FAIL=$((FAIL+1)); fi

echo
echo "crash supervisor:"
# Sandbox constants (see build_launcher): MAX_FAST=3, MAX_TOTAL=4, HEALTHY_S=2. A "healthy" stub
# therefore runs 3 s; a "fast crash" exits immediately.

scenario "SEGV once -> respawns and survives"     cinder 'stub_crash_then_ok $R 139 1'
check "  ran cinder-home twice" "$(runs_of "$LAST_R")" 2

scenario "3 fast crashes -> hands boot to stock"  stock  'stub_always $R 139 0'
check "  gave up after MAX_FAST" "$(runs_of "$LAST_R")" 3

# Slow crashes must NOT trip the consecutive counter — each healthy run resets it — but the
# absolute per-boot cap still ends the loop, so a 3-s crash cycle cannot spin forever.
scenario "slow crashes reset the tally, cap ends it" stock 'stub_always $R 134 3'
check "  gave up after MAX_TOTAL" "$(runs_of "$LAST_R")" 4

# The three deaths that mean "do not bring me back". Each must run exactly once and leave the
# reboot decision to appmgr, exactly as before the supervisor existed.
scenario "rc 42 (watchdog) is NOT respawned"      cinder 'stub_always $R 42 0'
check "  ran once" "$(runs_of "$LAST_R")" 1
scenario "rc 0 (boot-to-stock) is NOT respawned"  cinder 'stub_always $R 0 0'
check "  ran once" "$(runs_of "$LAST_R")" 1
scenario "SIGTERM (143) is NOT respawned"         cinder 'stub_always $R 143 0'
check "  ran once" "$(runs_of "$LAST_R")" 1

# An rc nobody recognised falls through to the old behaviour rather than respawning — a bug in
# the accounting degrades to the pre-supervisor net instead of disabling it.
scenario "unknown rc 7 is NOT respawned"          cinder 'stub_always $R 7 0'
check "  ran once" "$(runs_of "$LAST_R")" 1

# The escape for the escape: both kill switches restore the plain exec.
scenario "kill switch (/data) -> single exec"     cinder 'stub_always $R 139 0; : > $R/data/cinder/no_respawn'
check "  ran once" "$(runs_of "$LAST_R")" 1
scenario "kill switch (USB-MSC) -> single exec"   cinder 'stub_always $R 139 0; : > $R/contents/cinderhome_norespawn'
check "  ran once" "$(runs_of "$LAST_R")" 1

# A hot-swap that lands between two launches must not burn the whole budget on rc=127.
scenario "binary vanishes mid-session -> stock"   stock  'stub_vanish $R'
check "  ran once" "$(runs_of "$LAST_R")" 1

# ── STATIC: flags this device's toolbox does not accept ───────────────────────────────────────
# The launcher runs at normal boot with the DEVICE's tools, and /bin/mv is toolbox. Toolbox mv
# rejects `-f` outright — "failed on '-f'", rc 255, and it moves NOTHING (measured on device
# 2026-08-19). The log rotation carried `mv -f` for weeks and therefore never ran once, so every
# boot destroyed the previous boot's log: the exact evidence the rotation exists to keep. This
# sandbox cannot catch that, because the host's GNU mv accepts the flag — so it is asserted on the
# text instead. (`rm -f` is fine: toolbox rm complains about the flag and still removes the files.)
R="$(mktemp -d "$SP/static.XXXXXX")"; build_launcher "$R"
# Code lines only — the comment above the rotation names the broken form on purpose.
if grep -n '^[^#]*mv -f' "$R/launch.sh" >/dev/null 2>&1; then
    printf '  FAIL  %-46s -> %s\n' "launcher uses no 'mv -f' (toolbox rejects it)" "$(grep -c '^[^#]*mv -f' "$R/launch.sh") use(s)"
    FAIL=$((FAIL+1))
else
    printf '  ok    %-46s -> none\n' "launcher uses no 'mv -f' (toolbox rejects it)"
    PASS=$((PASS+1))
fi

rm -rf "$SP"/lt.*
echo
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
