#!/usr/bin/env bash
# cinder-install.sh — one-command build + adb install for Cinder on the NW-A55.
#
# Pure-Linux, adb-only. No MSC mode, no .UPG, no usbipd. Builds the dev channel
# (or stable with --stable), pushes the binary to /data/local/tmp (ext4, safe
# from MSC), kills the running cinder-home cleanly with the no-respawn flag
# armed (so /system isn't busy), does an atomic temp→cmp→mv swap, reboots.
# Keeps a one-step rollback on /data.
#
# Usage:
#   tools/cinder-install.sh                # build dev + install + reboot
#   tools/cinder-install.sh --no-build     # skip build, install existing dist/dev/
#   tools/cinder-install.sh --stable       # use stable channel (NO adb next boot!)
#   tools/cinder-install.sh --full         # also push + chmod 4755 the setuid helpers
#   tools/cinder-install.sh --rollback     # restore previous binary from /data/cinder/
#   tools/cinder-install.sh --logs         # tail /contents/cinderhome.log
#   tools/cinder-install.sh --status       # device + install health check
#   tools/cinder-install.sh -h             # this help
#
# The launcher (cinderhome-launch.sh) is installed on EVERY run, not just --full. It is the
# recovery ladder — bad-boot counter, escapes, crash supervisor — and must never be older than the
# app it supervises. --status reports whether the device's copy matches this tree.
#
# Prereqs:
#   - adb in PATH (apt install android-tools-adb)
#   - dev channel already installed (adb only exists once cinder-home dev is running)
#   - bash cinder-home/build.sh <channel> works (cross toolchain set up)
#
# FIRST INSTALL: this script CANNOT do the first install — adb only exists after
# the dev channel is running. For the one-time first install, put the device in
# MSC mode and run:  sudo tools/flash.sh install

set -euo pipefail

# ─── paths ─────────────────────────────────────────────────────────────────
SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$SELF/.." && pwd)"

# ─── colors ────────────────────────────────────────────────────────────────
if [ -t 1 ]; then
    C_R=$'\e[31m'; C_G=$'\e[32m'; C_Y=$'\e[33m'; C_B=$'\e[36m'; C_0=$'\e[0m'
else
    C_R=; C_G=; C_Y=; C_B=; C_0=
fi
info() { printf '%s==>%s %s\n' "$C_B" "$C_0" "$*"; }
ok()   { printf '%s ok %s %s\n' "$C_G" "$C_0" "$*"; }
say()  { printf '%s\n' "$*"; }
warn() { printf '%swarn%s %s\n' "$C_Y" "$C_0" "$*" >&2; }
die()  { printf '%s err %s %s\n' "$C_R" "$C_0" "$*" >&2; exit 1; }

# ─── args ──────────────────────────────────────────────────────────────────
CHANNEL="dev"
DO_BUILD=1
MODE="install"
FULL=0
while [ $# -gt 0 ]; do
    case "$1" in
        --no-build) DO_BUILD=0; shift;;
        --stable)   CHANNEL="stable"; shift;;
        --dev)      CHANNEL="dev"; shift;;
        --full)     FULL=1; shift;;
        --logs)     MODE="logs"; shift;;
        --status)   MODE="status"; shift;;
        --rollback) MODE="rollback"; shift;;
        -h|--help)  awk 'NR==1 { next } /^#/ { sub(/^# ?/, ""); print; next } /^$/ { next } { exit }' "$0"; exit 0;;
        *) die "unknown arg: $1 (try --help)";;
    esac
done

DIST="$REPO/cinder-home/dist/$CHANNEL"
BIN="$DIST/cinder-home"
INSTALL_PATH="/system/vendor/unknown321/bin/cinder-home"
STAGE="/data/local/tmp/cinder-home.new"
BACKUP="/data/cinder/cinder-home.last"
NORESPAWN="/data/cinder/no_respawn"
HELPERS_DIR="/system/vendor/unknown321/bin"

# THE LAUNCHER. Until 2026-08-26 this script installed the binary and the setuid helpers and left
# cinderhome-launch.sh alone, because only a full .UPG flash rewrites it. That was wrong, and the
# device proved it: cinder-home was dated 08-26, the helpers 08-25, and the launcher 08-12 —
# two weeks and several features stale.
#
# WHY THAT MATTERS MORE THAN A STALE BINARY: the launcher IS the recovery ladder. It is the
# bad-boot counter, the cable/flag escapes, and the crash supervisor. A device carrying a launcher
# older than the app it supervises is running its escapes at one version and the thing they rescue
# at another — the exact inversion the ladder rule exists to forbid. Concretely, that device had
# no crash supervisor at all, so an ordinary SIGPIPE left it with no Home app and burned a
# bad-boot life on the reboot that followed.
#
# So the launcher now ships on EVERY install, from the single source of truth it has always had —
# the heredoc inside deploy/install_cinderhome.sh — and it is verified on the host before the
# device is touched.
LAUNCHER_SRC="$REPO/cinder-home/deploy/install_cinderhome.sh"
LAUNCHER_PATH="/system/vendor/unknown321/bin/cinderhome-launch.sh"
LAUNCHER_STAGE="/data/local/tmp/cinderhome-launch.sh.new"
LAUNCHER_BACKUP="/data/cinder/cinderhome-launch.sh.last"

# ─── helpers ───────────────────────────────────────────────────────────────
require_adb() {
    command -v adb >/dev/null 2>&1 || die "adb not in PATH (apt install android-tools-adb)"
    [ "$(adb get-state 2>/dev/null)" = "device" ] || die "no adb device connected (run: adb devices)"
}

# Cut the launcher out of the installer heredoc. Same awk as cinder-home/tools/test_launcher.sh,
# deliberately — if the two ever extracted differently, the launcher the tests exercise would not
# be the launcher the device runs, and the tests would be worth nothing.
extract_launcher() {  # $1 = destination path
    awk "/<<'LAUNCH_EOF'/{f=1;next} /^LAUNCH_EOF\$/{f=0} f" "$LAUNCHER_SRC" > "$1"
}

# Verify an extracted launcher on the HOST, before any of it reaches the device.
#
# This is the one file where a silent corruption is unrecoverable from software: a launcher that
# does not exec anything leaves a device that boots to nothing, and the only way back is the
# cable-at-boot escape, which is a rung this script has just borrowed. So the checks are
# deliberately blunt and every one of them is fatal.
verify_launcher() {  # $1 = path to the extracted launcher
    local f="$1" lines
    [ -s "$f" ] || die "launcher extraction produced an empty file — is the LAUNCH_EOF heredoc still in $LAUNCHER_SRC?"
    lines="$(wc -l < "$f")"
    [ "$lines" -ge 200 ] || die "launcher extraction produced only $lines lines — expected 200+; the heredoc markers have moved"
    [ "$(head -1 "$f")" = "#!/system/bin/sh" ] || die "extracted launcher does not start with #!/system/bin/sh"
    # The same sanity grep install_cinderhome.sh runs before it writes the file. If the launcher
    # cannot exec cinder-home, installing it is how a device stops booting.
    grep -q 'exec "\$HOME_BIN"' "$f" || die "extracted launcher never execs \$HOME_BIN — refusing to install it"
    # Catch a truncation or a mangled quote that the greps above would sail past.
    bash -n "$f" 2>/dev/null || die "extracted launcher is not valid shell (bash -n failed) — refusing to install it"
}

# ─── status mode ───────────────────────────────────────────────────────────
if [ "$MODE" = "status" ]; then
    require_adb
    info "device: $(adb shell getprop ro.product.model 2>/dev/null | tr -d '\r')"
    info "running cinder-home:"
    # `ps | grep`, not pidof — this device has neither pidof nor pgrep, and `pidof` there prints
    # "pidof: not found" and exits non-zero, which reads as "not running" for any process at all.
    adb shell 'p=$(ps 2>/dev/null | grep /system/vendor/unknown321/bin/cinder-home | grep -v grep | awk "{print \$2}" | head -1); [ -n "$p" ] && echo "    pid: $p — running" || echo "    not running"' 2>/dev/null
    info "installed binary:"
    # This device's toolbox `ls -la` prints `perms user group size date time name` — SEVEN fields,
    # with no link-count column and no owner/group split into more. The GNU layout these field
    # numbers were written for has nine, so \$5 was the DATE and the line reported "2026-08-26
    # bytes". Size is \$4, date \$5, time \$6.
    adb shell "[ -f $INSTALL_PATH ] && ls -la $INSTALL_PATH | awk '{print \"    \"\$4\" bytes, mtime \"\$5\" \"\$6}' || echo '    NOT INSTALLED'" 2>/dev/null
    info "rollback available:"
    adb shell "[ -f $BACKUP ] && echo '    yes: $BACKUP' || echo '    no'" 2>/dev/null
    info "helpers installed:"
    for h in cinder-umount cinder-power cinder-msc cinder-clock cinder-fm cinder-voltable cinder-battery cinder-gpunode; do
        adb shell "[ -f $HELPERS_DIR/$h ] && echo '    $h: present' || echo '    $h: -'" 2>/dev/null
    done
    # LAUNCHER FRESHNESS. Worth its own line because a stale launcher is invisible from every
    # other symptom: the app runs, the helpers are present, and the recovery ladder is quietly a
    # different version from the thing it is meant to rescue. That went unnoticed for two weeks.
    info "launcher:"
    _lt="$(mktemp)"
    extract_launcher "$_lt"
    _repo_md5="$(md5sum "$_lt" | awk '{print $1}')"
    _dev_md5="$(adb shell "md5sum $LAUNCHER_PATH 2>/dev/null" 2>/dev/null | awk '{print $1}' | tr -d '\r')"
    rm -f "$_lt"
    if [ -z "$_dev_md5" ]; then
        echo "    NOT INSTALLED — the device has no $LAUNCHER_PATH"
    elif [ "$_dev_md5" = "$_repo_md5" ]; then
        echo "    up to date ($_repo_md5)"
    else
        echo "    STALE — device $_dev_md5, repo $_repo_md5"
        echo "    the recovery ladder on the device is not the one in this tree."
        echo "    run tools/cinder-install.sh to bring it level."
    fi
    adb shell "ls -la $LAUNCHER_PATH 2>/dev/null | awk '{print \"    \"\$4\" bytes, mtime \"\$5\" \"\$6}'" 2>/dev/null
    info "last 10 log lines:"
    adb shell 'tail -10 /contents/cinderhome.log 2>/dev/null || echo "(no log)"' 2>/dev/null | sed 's/^/    /'
    exit 0
fi

# ─── logs mode ─────────────────────────────────────────────────────────────
if [ "$MODE" = "logs" ]; then
    require_adb
    info "tailing /contents/cinderhome.log (Ctrl-C to stop)"
    exec adb shell 'tail -f /contents/cinderhome.log 2>/dev/null || tail -f /tmp/cinderhome.log 2>/dev/null'
fi

# ─── build the on-device swap script (used by install + rollback) ──────────
make_swap_script() {
    local src="$1"  # source path on device
    cat <<SWAPEOF
#!/system/bin/sh
# generated by cinder-install.sh — atomic swap of cinder-home on /system.
set -e

STAGE="$src"
INSTALL_PATH="$INSTALL_PATH"
BACKUP="$BACKUP"
NORESPAWN="$NORESPAWN"
HELPERS_DIR="$HELPERS_DIR"
LAUNCHER_STAGE="$LAUNCHER_STAGE"
LAUNCHER_PATH="$LAUNCHER_PATH"
LAUNCHER_BACKUP="$LAUNCHER_BACKUP"
FULL=$FULL

echo "[swap] source: \$STAGE"
echo "[swap] target: \$INSTALL_PATH"

# 1. backup current binary (for --rollback)
mkdir -p /data/cinder
if [ -f "\$INSTALL_PATH" ]; then
    cp "\$INSTALL_PATH" "\$BACKUP"
    echo "[swap] backed up current binary -> \$BACKUP"
fi

# 2. arm no-respawn so killing cinder-home doesn't trigger the supervisor
touch "\$NORESPAWN"

# 3. remount /system rw and install the HELPERS FIRST — before anything kills the app.
#
# WHY THE ORDER MATTERS: killing cinder-home makes appmgr reboot the device (its SIGCHLD handler
# calls android_reboot when the foreground app goes away), and that reboot lands whenever it lands.
# With the helper loop after the kill it was a RACE, and it showed: one run installed 1 of 6
# helpers, the next installed 3 of 6, both reporting success. The helpers are ordinary files that
# have nothing to do with the running process, so they go in while the device is still calm.
#    remount /system rw (try remount first, fall back to mount-by-source —
#    matches install_cinderhome.sh line 57-58)
echo "[swap] remounting /system rw"
if ! mount -o remount,rw /system 2>/dev/null; then
    mount -t ext4 -o rw,remount /emmc@android /system 2>/dev/null || {
        rm "\$NORESPAWN" 2>/dev/null
        echo "[swap] FAIL: could not remount /system rw"
        exit 1
    }
fi

#    helpers if --full
if [ "\$FULL" = "1" ]; then
    for h in cinder-umount cinder-power cinder-msc cinder-clock cinder-fm cinder-voltable cinder-battery cinder-gpunode; do
        if [ -f "/data/local/tmp/\$h.new" ]; then
            cp "/data/local/tmp/\$h.new" "\$HELPERS_DIR/\$h.tmp"
            if cmp "/data/local/tmp/\$h.new" "\$HELPERS_DIR/\$h.tmp"; then
                # ORDER MATTERS: chown CLEARS the setuid bit, so it has to come FIRST. The other
                # way round (2026-08-18) silently shipped cinder-umount as 0755 — the helper still
                # existed, still ran, and could no longer unmount /contents for USB-MSC, which is
                # the whole reason it is setuid.
                chown root:root "\$HELPERS_DIR/\$h.tmp" 2>/dev/null || true
                chmod 4755 "\$HELPERS_DIR/\$h.tmp"
                mv "\$HELPERS_DIR/\$h.tmp" "\$HELPERS_DIR/\$h"
                # Report what is ACTUALLY on disk, not what we asked for — that is how the missing
                # setuid bit went unnoticed.
                #
                # NOTE, and it bit twice on 2026-08-18: this heredoc is UNQUOTED, so the host
                # expands command substitutions, backticks and variables while GENERATING the
                # script. That applies to COMMENTS too. An unescaped substitution here ran on
                # the host with both vars unset and spliced a whole directory listing into the
                # middle of this loop, breaking it. Escape every dollar; no backticks in prose.
                echo "[swap] installed helper: \$h (\$(ls -l "\$HELPERS_DIR/\$h" 2>/dev/null | cut -c1-10))"
            else
                rm "\$HELPERS_DIR/\$h.tmp" 2>/dev/null
                echo "[swap] WARN: \$h copy mismatch, skipped"
            fi
            # The device rm and mv do NOT accept a -f flag; they parse it as a filename and
            # fail with "rm failed for -f" or "Invalid cross-device link". That aborted this
            # very loop after the first helper on 2026-08-18. Redirect stderr, do not force.
            rm "/data/local/tmp/\$h.new" 2>/dev/null
        fi
    done
fi

# 3b. install the LAUNCHER, still in the calm phase — before anything is killed.
#
# The launcher is the recovery ladder, so this is the most dangerous single write this script
# makes: a launcher that cannot exec leaves a device that boots to nothing. Three things keep
# that safe. The host verified the file before it was pushed (shebang, exec line, shell syntax).
# The old launcher is copied to /data first, so --rollback can put it back. And the new one is
# written to a .tmp, byte-compared, and only then moved into place, so an interrupted copy can
# never be the file appmgr execs.
#
# A failure here is NOT fatal to the install. The old launcher is left exactly as it was, which is
# the state the device has been booting from all along; the binary swap below still runs. Being
# stale is a much smaller problem than being broken.
if [ -f "\$LAUNCHER_STAGE" ]; then
    if [ -f "\$LAUNCHER_PATH" ]; then
        cp "\$LAUNCHER_PATH" "\$LAUNCHER_BACKUP" 2>/dev/null && \
            echo "[swap] backed up launcher -> \$LAUNCHER_BACKUP"
    fi
    cp "\$LAUNCHER_STAGE" "\$LAUNCHER_PATH.tmp" 2>/dev/null
    if cmp "\$LAUNCHER_STAGE" "\$LAUNCHER_PATH.tmp" 2>/dev/null; then
        chmod 0755 "\$LAUNCHER_PATH.tmp"
        mv "\$LAUNCHER_PATH.tmp" "\$LAUNCHER_PATH"
        echo "[swap] installed launcher: \$LAUNCHER_PATH"
    else
        # No -f on this device's rm; it parses the flag as a filename and fails.
        rm "\$LAUNCHER_PATH.tmp" 2>/dev/null
        echo "[swap] WARN: launcher copy mismatch — KEEPING the existing launcher"
    fi
    rm "\$LAUNCHER_STAGE" 2>/dev/null
fi


# 4. kill cinder-home (launcher stays alive; appmgr won't reboot)
#
# THERE IS NO 'pidof' ON THIS DEVICE. It printed "pidof: not found" and every check below then
# read as "not running", so on 2026-08-18 the swap replaced the binary UNDER the live process and
# reported success — the file changed, /proc/<pid>/exe went '(deleted)', and the old code kept
# running until a reboot. Find the pid from ps instead, and match the full install path so a
# grep of this script's own command line cannot match.
cinder_pid() { ps 2>/dev/null | grep "\$INSTALL_PATH" | grep -v grep | awk '{print \$2}' | head -1; }
PID=\$(cinder_pid)
if [ -n "\$PID" ]; then
    echo "[swap] killing cinder-home (pid \$PID)"
    kill \$PID 2>/dev/null || true
    for i in 1 2 3 4 5 6 7 8 9 10; do
        [ -z "\$(cinder_pid)" ] && break
        sleep 1
    done
    if [ -n "\$(cinder_pid)" ]; then
        echo "[swap] WARN: still alive after 10s, sending KILL"
        kill -9 \$(cinder_pid) 2>/dev/null || true
        sleep 1
    fi
else
    echo "[swap] cinder-home not running (ok)"
fi

# 5. atomic swap: temp -> cmp -> mv
echo "[swap] staging \$STAGE -> \$INSTALL_PATH.tmp"
cp "\$STAGE" "\$INSTALL_PATH.tmp"
if ! cmp "\$STAGE" "\$INSTALL_PATH.tmp"; then
    rm "\$INSTALL_PATH.tmp" "\$NORESPAWN" 2>/dev/null
    mount -o remount,ro /system 2>/dev/null || true
    echo "[swap] FAIL: staged copy mismatch"
    exit 1
fi
chmod 755 "\$INSTALL_PATH.tmp"
mv "\$INSTALL_PATH.tmp" "\$INSTALL_PATH"
echo "[swap] installed: \$INSTALL_PATH"

# 7. cleanup + remount ro + reboot
rm "\$STAGE" "\$NORESPAWN" 2>/dev/null
sync
mount -o remount,ro /system 2>/dev/null || true
echo "[swap] done — rebooting"
sync
reboot
SWAPEOF
}

# ─── rollback mode ─────────────────────────────────────────────────────────
if [ "$MODE" = "rollback" ]; then
    require_adb
    info "rolling back to $BACKUP"
    adb shell "[ -f $BACKUP ]" 2>/dev/null \
        || die "no rollback binary at $BACKUP (nothing to roll back to — run a normal install first)"
    # Roll the LAUNCHER back too, when there is one to roll back to. A rollback is what you reach
    # for when the last install made the device worse, and since that install now also replaces the
    # recovery ladder, leaving the new ladder in place would undo half of what was asked for.
    # Staging the backup where the swap script already looks keeps this to one code path.
    if adb shell "[ -f $LAUNCHER_BACKUP ]" 2>/dev/null; then
        adb shell "cp $LAUNCHER_BACKUP $LAUNCHER_STAGE" 2>/dev/null \
            && info "staged the previous launcher for rollback"
    else
        info "no previous launcher at $LAUNCHER_BACKUP — leaving the installed one alone"
    fi
    SWAP_SCRIPT="$(mktemp)"
    make_swap_script "$BACKUP" > "$SWAP_SCRIPT"
    adb push "$SWAP_SCRIPT" /data/local/tmp/_cinder_swap.sh >/dev/null
    adb shell "chmod 755 /data/local/tmp/_cinder_swap.sh"
    rm -f "$SWAP_SCRIPT"
    adb shell "sh /data/local/tmp/_cinder_swap.sh" || die "rollback swap failed (see above)"
    ok "rollback complete — device rebooting to previous binary"
    exit 0
fi

# ─── install mode (default) ────────────────────────────────────────────────
# 1. build (unless --no-build)
if [ "$DO_BUILD" = 1 ]; then
    info "building cinder-home ($CHANNEL channel)…"
    bash "$REPO/cinder-home/build.sh" "$CHANNEL" || die "build failed"
fi

[ -f "$BIN" ] || die "binary not found: $BIN (did the build succeed?)"
BIN_SIZE="$(stat -c %s "$BIN")"
info "binary: $BIN ($BIN_SIZE bytes)"

# 2. device + first-install check
require_adb
info "device: $(adb shell getprop ro.product.model 2>/dev/null | tr -d '\r')"
adb shell "[ -f $INSTALL_PATH ]" 2>/dev/null \
    || die "cinder-home is NOT installed at $INSTALL_PATH.
This script can only UPDATE an existing install (adb only exists once the
dev channel is running). For the one-time first install, put the device in
MSC mode and run:  sudo tools/flash.sh install"

# 3. push binary to /data/local/tmp (ext4, safe from MSC mode)
info "pushing binary to $STAGE…"
adb push "$BIN" "$STAGE" >/dev/null
ok "pushed $BIN_SIZE bytes"

# 3b. push the LAUNCHER — always, not behind --full.
#
# The launcher is the recovery ladder; it must never be older than the app it supervises (see the
# comment on LAUNCHER_SRC). Extracted and fully verified on the host first, so a bad extraction
# fails here — with the device untouched — rather than on /system.
# Deliberately NOT cleaned up via `trap ... EXIT`: step 5b installs the trap that restores rung 0
# (the cable-at-boot escape), and a second EXIT trap replaces the first rather than adding to it.
# Losing that restore would leave the device's strongest escape disarmed. The file is removed
# inline below instead.
LAUNCHER_TMP="$(mktemp)"
extract_launcher "$LAUNCHER_TMP"
verify_launcher  "$LAUNCHER_TMP"
LAUNCHER_MD5="$(md5sum "$LAUNCHER_TMP" | awk '{print $1}')"
DEV_LAUNCHER_MD5="$(adb shell "md5sum $LAUNCHER_PATH 2>/dev/null" 2>/dev/null | awk '{print $1}' | tr -d '\r')"
if [ "$LAUNCHER_MD5" = "$DEV_LAUNCHER_MD5" ]; then
    info "launcher: already current on device ($LAUNCHER_MD5) — will re-verify, not rewrite"
else
    info "launcher: device has ${DEV_LAUNCHER_MD5:-none}, repo has $LAUNCHER_MD5 — updating"
fi
info "pushing launcher to $LAUNCHER_STAGE…"
adb push "$LAUNCHER_TMP" "$LAUNCHER_STAGE" >/dev/null
ok "pushed launcher ($(wc -l < "$LAUNCHER_TMP") lines)"
rm -f "$LAUNCHER_TMP"

# 4. push helpers if --full
if [ "$FULL" = 1 ]; then
    info "pushing setuid helpers (--full)…"
    for h in cinder-umount cinder-power cinder-msc cinder-clock cinder-fm cinder-voltable cinder-battery; do
        if [ -f "$DIST/$h" ]; then
            adb push "$DIST/$h" "/data/local/tmp/$h.new" >/dev/null
            ok "  staged $h"
        fi
    done
    if [ "$CHANNEL" = "dev" ] && [ -f "$DIST/cinder-gpunode" ]; then
        adb push "$DIST/cinder-gpunode" "/data/local/tmp/cinder-gpunode.new" >/dev/null
        ok "  staged cinder-gpunode (dev-only)"
    fi
fi

# 5. upload + run the swap script
SWAP_SCRIPT="$(mktemp)"
make_swap_script "$STAGE" > "$SWAP_SCRIPT"
# SANITY-GATE THE GENERATED SCRIPT. The heredoc that builds it is unquoted, so anything unescaped
# in it — including in a COMMENT — is executed on the HOST at generation time and its output is
# spliced into the script body. That happened twice on 2026-08-18 and both times the result still
# ran, still reported success, and silently skipped five of six helpers. A splice is easy to spot
# even when the cause is not: real lines here never start with a directory listing.
if grep -qE '^(total [0-9]+|[-dlbcps][rwxsStT-]{9}[ .])' "$SWAP_SCRIPT"; then
    say ""
    grep -nE '^(total [0-9]+|[-dlbcps][rwxsStT-]{9}[ .])' "$SWAP_SCRIPT" | head -3
    rm -f "$SWAP_SCRIPT"
    die "generated swap script contains spliced command output — an unescaped \$( ) or backtick in make_swap_script"
fi
if ! sh -n "$SWAP_SCRIPT" 2>/dev/null; then
    rm -f "$SWAP_SCRIPT"
    die "generated swap script is not valid sh"
fi
info "uploading swap script…"
adb push "$SWAP_SCRIPT" /data/local/tmp/_cinder_swap.sh >/dev/null
adb shell "chmod 755 /data/local/tmp/_cinder_swap.sh"
rm -f "$SWAP_SCRIPT"

# 5b. BORROW rung 0 for exactly one boot.
#
# The launcher treats ANY cable at boot as the escape to stock (`usb_connected` reads
# android_usb/state and power_supply/usb/{online,present} — there is no adb-vs-charging
# distinction). So a flash over adb would reboot straight into the Sony player every time, which is
# useless for verifying the thing you just installed.
#
# The opt-out is therefore taken here and GIVEN BACK below, as close to the boot as possible. It is
# a loan, not a setting: leaving it set silently removes the one escape that depends on nothing.
# If this script dies before the restore, the trap still puts it back on the next adb contact, and
# rung 1 (the bad-boot counter, MAXBAD=4) covers the window regardless.
CABLE_FLAG=/data/cinder/cable_escape_off
restore_cable_escape() {
    adb wait-for-device >/dev/null 2>&1 || return 0
    adb shell "rm $CABLE_FLAG 2>/dev/null; sync" >/dev/null 2>&1 || true
    if adb shell "[ -e $CABLE_FLAG ] && echo set" 2>/dev/null | grep -q set; then
        warn "could NOT remove $CABLE_FLAG — rung 0 is still disabled, remove it by hand"
    else
        ok "rung 0 restored (cable-at-boot -> stock is armed again)"
    fi
}
# INT and TERM as well as EXIT: an untrapped SIGTERM kills the shell WITHOUT running an EXIT trap,
# and that is exactly what happened on 2026-08-18 — the run was terminated while waiting for the
# device and left rung 0 borrowed until the next session noticed. The escape must survive this
# script being killed, or it is not much of an escape.
trap 'restore_cable_escape; exit 143' INT TERM
trap restore_cable_escape EXIT
info "borrowing rung 0 for this boot (cable-at-boot escape off)…"
adb shell "touch $CABLE_FLAG; sync" >/dev/null 2>&1 || warn "could not set $CABLE_FLAG — this boot will land on STOCK if a cable is attached"

info "running swap on device…"
adb shell "sh /data/local/tmp/_cinder_swap.sh" || die "swap failed (see above — rollback with: $0 --rollback)"

# device reboots here, adb connection drops — that's expected
info "waiting for the device to come back…"
if adb wait-for-device >/dev/null 2>&1; then
    # Give rung 0 back FIRST, before any deeper verification: if the new build is going to misbehave
    # this is the moment you most want the escape armed, not the moment after a health check that
    # might itself hang.
    restore_cable_escape
    trap - EXIT
    PID_BACK="$(adb shell 'ps 2>/dev/null | grep /system/vendor/unknown321/bin/cinder-home | grep -v grep | awk "{print \$2}" | head -1' 2>/dev/null | tr -d '\r')"
    if [ -n "$PID_BACK" ]; then ok "cinder-home running (pid $PID_BACK)"
    else warn "cinder-home not seen yet — check: $0 --status"; fi
else
    warn "device did not come back on adb; rung 0 may still be borrowed — check by hand"
fi
ok "install complete"
say ""
say "  next:"
say "    $0 --status       check first-boot health"
say "    $0 --logs         tail the boot log"
say "    $0 --rollback     restore the previous binary if this one's bad"
